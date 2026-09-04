//! 托管模式的数据库词法检索。
//!
//! `chunks.text` 才是正文真值；这条通道不依赖某一台实例的本地 Tantivy 目录。

use sqlx::PgPool;
use utopia_core::AppResult;
use uuid::Uuid;

/// 在一个知识库的现行分块里做数据库词法检索，按相关性返回 chunk id。
pub async fn lexical_search(
    _pool: &PgPool,
    _kb_id: Uuid,
    _query: &str,
    _limit: i64,
) -> AppResult<Vec<Uuid>> {
    todo!("implemented after the failing database-backed test")
}
