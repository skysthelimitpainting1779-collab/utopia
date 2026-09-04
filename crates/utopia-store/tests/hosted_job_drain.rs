use sqlx::PgPool;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use uuid::Uuid;

#[tokio::test]
async fn one_hosted_drain_claims_executes_and_finishes_one_job() -> anyhow::Result<()> {
    let Some(url) = utopia_store::test_db::url() else {
        return Ok(());
    };
    let pool = PgPool::connect(&url).await?;
    let marker = Uuid::now_v7().to_string();
    let (job_id,): (i64,) = sqlx::query_as(
        "INSERT INTO jobs (kind, payload)
         VALUES ('hosted_test', jsonb_build_object('marker', $1::text)) RETURNING id",
    )
    .bind(&marker)
    .fetch_one(&pool)
    .await?;
    let calls = Arc::new(AtomicUsize::new(0));
    let handler_calls = calls.clone();
    let expected = marker.clone();

    let report = utopia_store::hosted_jobs::drain(&pool, 1, move |job| {
        let calls = handler_calls.clone();
        let expected = expected.clone();
        async move {
            assert_eq!(job.kind, "hosted_test");
            assert_eq!(job.payload["marker"], expected);
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    })
    .await?;

    let status: String = sqlx::query_scalar("SELECT status FROM jobs WHERE id = $1")
        .bind(job_id)
        .fetch_one(&pool)
        .await?;
    sqlx::query("DELETE FROM jobs WHERE id = $1")
        .bind(job_id)
        .execute(&pool)
        .await?;

    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(status, "done");
    assert_eq!(report.processed, 1);
    Ok(())
}

#[tokio::test]
async fn stale_recovery_does_not_steal_a_fresh_running_job() -> anyhow::Result<()> {
    let Some(url) = utopia_store::test_db::url() else {
        return Ok(());
    };
    let pool = PgPool::connect(&url).await?;
    let marker = Uuid::now_v7().to_string();
    let rows: Vec<(i64,)> = sqlx::query_as(
        "INSERT INTO jobs (kind, payload, status, locked_at)
         VALUES
           ('hosted_stale_test', jsonb_build_object('marker', $1::text), 'running', now() - interval '1 hour'),
           ('hosted_fresh_test', jsonb_build_object('marker', $1::text), 'running', now())
         RETURNING id",
    )
    .bind(&marker)
    .fetch_all(&pool)
    .await?;
    let ids: Vec<i64> = rows.into_iter().map(|(id,)| id).collect();

    utopia_store::hosted_jobs::recover_stale(&pool, 300).await?;
    let states: Vec<(i64, String)> =
        sqlx::query_as("SELECT id, status FROM jobs WHERE id = ANY($1) ORDER BY id")
            .bind(&ids)
            .fetch_all(&pool)
            .await?;
    sqlx::query("DELETE FROM jobs WHERE id = ANY($1)")
        .bind(&ids)
        .execute(&pool)
        .await?;

    assert_eq!(states.len(), 2);
    assert_eq!(states[0].1, "queued");
    assert_eq!(states[1].1, "running");
    Ok(())
}
