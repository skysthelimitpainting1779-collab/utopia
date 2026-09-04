//! 托管模式的数据库词法检索。
//!
//! `chunks.text` 才是正文真值；这条通道不依赖某一台实例的本地 Tantivy 目录。
//! P0 同时用 Postgres `simple` 全文检索与字面包含兜底。前者给英文等分词语料排序，
//! 后者保证中文、代码符号与长标识符不会因词典缺失而静默归零。

use sqlx::PgPool;
use utopia_core::AppResult;
use uuid::Uuid;

/// 在一个知识库的现行、未删除文档分块里做数据库词法检索，按相关性返回 chunk id。
pub async fn lexical_search(
    pool: &PgPool,
    kb_id: Uuid,
    query: &str,
    limit: i64,
) -> AppResult<Vec<Uuid>> {
    let query = query.trim();
    if query.is_empty() || limit <= 0 {
        return Ok(Vec::new());
    }
    let limit = limit.clamp(1, 200);
    let rows: Vec<(Uuid,)> = sqlx::query_as(
        "WITH needle AS (
             SELECT websearch_to_tsquery('simple', $2) AS tsq
         ), ranked AS (
             SELECT c.id,
                    (strpos(lower(c.text), lower($2)) > 0) AS literal_match,
                    ts_rank_cd(to_tsvector('simple', c.text), needle.tsq) AS text_rank,
                    c.seq
               FROM chunks c
               JOIN documents d ON d.id = c.document_id
              CROSS JOIN needle
              WHERE c.kb_id = $1
                AND c.superseded_at IS NULL
                AND d.deleted_at IS NULL
                AND d.purged_at IS NULL
                AND (
                    to_tsvector('simple', c.text) @@ needle.tsq
                    OR strpos(lower(c.text), lower($2)) > 0
                )
         )
         SELECT id
           FROM ranked
          ORDER BY literal_match DESC, text_rank DESC, seq ASC, id ASC
          LIMIT $3",
    )
    .bind(kb_id)
    .bind(query)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(id,)| id).collect())
}
