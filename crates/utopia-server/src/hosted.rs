//! Vercel Workflow 调用的有限托管执行入口。
//!
//! 这条路不暴露给普通会话：控制面用部署级 Bearer 密钥调用。每次请求只排到期
//! 调度项并处理很少几条任务，完成后把 `due_remaining` 返回给 Workflow 决定是否续步。

use axum::extract::{Query, State};
use axum::http::{header, HeaderMap};
use axum::routing::post;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq;
use utopia_core::{AppError, AppResult};

use crate::error::ApiResult;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct TickQuery {
    #[serde(default = "default_max_jobs")]
    max_jobs: usize,
    #[serde(default = "default_lease_seconds")]
    lease_seconds: i64,
}

const fn default_max_jobs() -> usize {
    1
}

const fn default_lease_seconds() -> i64 {
    15 * 60
}

#[derive(Debug, Serialize)]
pub struct TickResponse {
    pub scheduled_sources: usize,
    pub scheduled_inference: usize,
    pub recovered_stale: u64,
    pub processed: usize,
    pub due_remaining: bool,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/_internal/hosted/tick", post(tick))
        .with_state(state)
}

fn authorized(headers: &HeaderMap, expected: Option<&str>) -> bool {
    let Some(expected) = expected.filter(|value| !value.is_empty()) else {
        return false;
    };
    let Some(actual) = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
    else {
        return false;
    };
    actual.as_bytes().ct_eq(expected.as_bytes()).into()
}

async fn schedule_sources_once(state: &AppState) -> AppResult<usize> {
    let mut scheduled = 0usize;
    for source in utopia_store::sources::due_sources(&state.pool).await? {
        if utopia_store::sources::mark_queued(&state.pool, source.id).await? {
            utopia_store::jobs::enqueue(
                &state.pool,
                "sync_source",
                serde_json::json!({ "source_id": source.id }),
            )
            .await?;
            scheduled += 1;
        }
    }
    Ok(scheduled)
}

async fn schedule_inference_once(state: &AppState) -> AppResult<usize> {
    let due = utopia_store::reasoning::due_for_inference(&state.pool).await?;
    for kb_id in &due {
        utopia_store::jobs::enqueue_unless_queued(
            &state.pool,
            "materialize_inferences",
            serde_json::json!({ "kb_id": kb_id }),
        )
        .await?;
    }
    Ok(due.len())
}

async fn tick(
    State(state): State<AppState>,
    Query(query): Query<TickQuery>,
    headers: HeaderMap,
) -> ApiResult<Json<TickResponse>> {
    if !state.hosted {
        return Err(AppError::NotFound.into());
    }
    if !authorized(&headers, state.control_plane_token.as_deref()) {
        return Err(AppError::Unauthorized.into());
    }

    let scheduled_sources = schedule_sources_once(&state).await?;
    let scheduled_inference = schedule_inference_once(&state).await?;
    let recovered_stale = utopia_store::hosted_jobs::recover_stale(
        &state.pool,
        query.lease_seconds.clamp(60, 24 * 60 * 60),
    )
    .await?;

    let worker_state = state.clone();
    let report =
        utopia_store::hosted_jobs::drain(&state.pool, query.max_jobs.clamp(1, 8), move |job| {
            let state = worker_state.clone();
            async move {
                let result = crate::dispatch(&state, &job).await.map_err(|error| {
                    if crate::alerting::hopeless(&error) {
                        error.context(utopia_core::Terminal)
                    } else {
                        error
                    }
                });
                if let Err(error) = &result {
                    crate::alerting::observe_job_failure(&state, &job, error).await;
                }
                result
            }
        })
        .await
        .map_err(AppError::Other)?;

    Ok(Json(TickResponse {
        scheduled_sources,
        scheduled_inference,
        recovered_stale,
        processed: report.processed,
        due_remaining: report.due_remaining,
    }))
}

#[cfg(test)]
mod tests {
    use super::authorized;
    use axum::http::{header, HeaderMap, HeaderValue};

    #[test]
    fn internal_authorization_requires_an_exact_bearer_token() {
        let mut headers = HeaderMap::new();
        assert!(!authorized(&headers, Some("secret")));

        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer wrong"),
        );
        assert!(!authorized(&headers, Some("secret")));

        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer secret"),
        );
        assert!(authorized(&headers, Some("secret")));
        assert!(!authorized(&headers, None));
    }
}
