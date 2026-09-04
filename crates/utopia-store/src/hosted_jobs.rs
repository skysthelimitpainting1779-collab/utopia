//! 托管环境一次请求内的有限队列执行。
//!
//! 真正实现写在数据库集成测试之后；本模块不取代 `jobs` 的领域语义，只把
//! 永久 `loop` 改成可由 Workflow 反复调用的一小步。

use serde::Serialize;
use sqlx::PgPool;
use std::future::Future;
use utopia_core::AppResult;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct DrainReport {
    pub processed: usize,
    pub due_remaining: bool,
}

pub async fn recover_stale(_pool: &PgPool, _lease_seconds: i64) -> AppResult<u64> {
    todo!("implemented after the failing database-backed test")
}

pub async fn drain<F, Fut>(
    _pool: &PgPool,
    _max_jobs: usize,
    _handler: F,
) -> anyhow::Result<DrainReport>
where
    F: Fn(crate::jobs::Job) -> Fut,
    Fut: Future<Output = anyhow::Result<()>>,
{
    todo!("implemented after the failing database-backed test")
}
