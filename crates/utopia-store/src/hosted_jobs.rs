//! 托管环境一次请求内的有限队列执行。
//!
//! 领域任务、载荷、重试次数与退避仍由原 `jobs` 表表达；这里只把永久轮询改成
//! Workflow 可反复调用的一小步。认领仍用 `FOR UPDATE SKIP LOCKED`，所以多实例
//! 同时 tick 也不会拿到同一任务。

use serde::Serialize;
use sqlx::PgPool;
use std::future::Future;
use utopia_core::AppResult;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct DrainReport {
    pub processed: usize,
    pub due_remaining: bool,
}

/// 只回收明确超出租约的任务。托管环境绝不能把「另一台实例正在跑」当成孤儿。
pub async fn recover_stale(pool: &PgPool, lease_seconds: i64) -> AppResult<u64> {
    let lease_seconds = lease_seconds.max(1);
    let result = sqlx::query(
        "UPDATE jobs
            SET status = 'queued', locked_at = NULL,
                run_at = LEAST(run_at, now()), updated_at = now()
          WHERE status = 'running'
            AND locked_at IS NOT NULL
            AND locked_at < now() - make_interval(secs => $1::float8)",
    )
    .bind(lease_seconds as f64)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

async fn claim_one(pool: &PgPool) -> AppResult<Option<crate::jobs::Job>> {
    Ok(sqlx::query_as(
        "UPDATE jobs SET status = 'running', locked_at = now(),
                attempts = attempts + 1, updated_at = now()
         WHERE id = (
             SELECT id FROM jobs
             WHERE status = 'queued' AND run_at <= now()
             ORDER BY run_at, id
             FOR UPDATE SKIP LOCKED
             LIMIT 1
         )
         RETURNING id, kind, payload, attempts, max_attempts",
    )
    .fetch_optional(pool)
    .await?)
}

async fn mark_done(pool: &PgPool, id: i64) -> AppResult<()> {
    sqlx::query(
        "UPDATE jobs
            SET status = 'done', locked_at = NULL, last_error = NULL, updated_at = now()
          WHERE id = $1",
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

fn retry_delay(attempts: i32, max_attempts: i32, terminal: bool) -> Option<i64> {
    if terminal || attempts >= max_attempts {
        return None;
    }
    Some(30i64 * i64::from(attempts) * i64::from(attempts))
}

async fn mark_failed(
    pool: &PgPool,
    job: &crate::jobs::Job,
    error: &anyhow::Error,
) -> AppResult<()> {
    let text = format!("{error:#}");
    match retry_delay(
        job.attempts,
        job.max_attempts,
        utopia_core::is_terminal(error),
    ) {
        Some(backoff_seconds) => {
            sqlx::query(
                "UPDATE jobs
                    SET status = 'queued', locked_at = NULL, last_error = $2,
                        run_at = now() + make_interval(secs => $3::float8),
                        updated_at = now()
                  WHERE id = $1",
            )
            .bind(job.id)
            .bind(text)
            .bind(backoff_seconds as f64)
            .execute(pool)
            .await?;
        }
        None => {
            sqlx::query(
                "UPDATE jobs
                    SET status = 'failed', locked_at = NULL, last_error = $2, updated_at = now()
                  WHERE id = $1",
            )
            .bind(job.id)
            .bind(text)
            .execute(pool)
            .await?;
        }
    }
    Ok(())
}

async fn due_remaining(pool: &PgPool) -> AppResult<bool> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1 FROM jobs
              WHERE status = 'queued' AND run_at <= now()
         )",
    )
    .fetch_one(pool)
    .await?)
}

/// 最多处理 `max_jobs` 条，随后把「现在是否还有到期任务」交给 Workflow 决定是否续跑。
pub async fn drain<F, Fut>(
    pool: &PgPool,
    max_jobs: usize,
    handler: F,
) -> anyhow::Result<DrainReport>
where
    F: Fn(crate::jobs::Job) -> Fut,
    Fut: Future<Output = anyhow::Result<()>>,
{
    let mut processed = 0usize;
    for _ in 0..max_jobs {
        let Some(job) = claim_one(pool).await? else {
            break;
        };
        let outcome = handler(job.clone()).await;
        match outcome {
            Ok(()) => mark_done(pool, job.id).await?,
            Err(error) => {
                tracing::warn!(
                    job_id = job.id,
                    kind = %job.kind,
                    error = %error,
                    "托管任务执行失败"
                );
                mark_failed(pool, &job, &error).await?;
            }
        }
        processed += 1;
    }
    Ok(DrainReport {
        processed,
        due_remaining: due_remaining(pool).await?,
    })
}

#[cfg(test)]
mod tests {
    use super::retry_delay;

    #[test]
    fn hosted_retry_policy_matches_the_local_worker() {
        assert_eq!(retry_delay(1, 3, false), Some(30));
        assert_eq!(retry_delay(2, 3, false), Some(120));
        assert_eq!(retry_delay(3, 3, false), None);
        assert_eq!(retry_delay(1, 3, true), None);
    }
}
