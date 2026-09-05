use sqlx::PgPool;
use uuid::Uuid;

#[tokio::test]
async fn hosted_search_returns_only_live_matching_chunks() -> anyhow::Result<()> {
    let Some(url) = utopia_store::test_db::url() else {
        return Ok(());
    };
    let pool = PgPool::connect(&url).await?;
    let (org, workspace, kb) = (Uuid::now_v7(), Uuid::now_v7(), Uuid::now_v7());
    let (live_doc, deleted_doc) = (Uuid::now_v7(), Uuid::now_v7());
    let (wanted, superseded, deleted) = (Uuid::now_v7(), Uuid::now_v7(), Uuid::now_v7());

    sqlx::query("INSERT INTO organizations (id, name) VALUES ($1, $2)")
        .bind(org)
        .bind(format!("hosted-search-{org}"))
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO workspaces (id, org_id, name) VALUES ($1, $2, 'hosted-search')")
        .bind(workspace)
        .bind(org)
        .execute(&pool)
        .await?;
    sqlx::query(
        "INSERT INTO knowledge_bases (id, workspace_id, name) VALUES ($1, $2, 'hosted-search')",
    )
    .bind(kb)
    .bind(workspace)
    .execute(&pool)
    .await?;
    sqlx::query(
        "INSERT INTO documents (id, kb_id, filename, sha256)
         VALUES ($1, $3, 'live.txt', $4), ($2, $3, 'deleted.txt', $5)",
    )
    .bind(live_doc)
    .bind(deleted_doc)
    .bind(kb)
    .bind("1".repeat(64))
    .bind("2".repeat(64))
    .execute(&pool)
    .await?;
    sqlx::query("UPDATE documents SET deleted_at = now() WHERE id = $1")
        .bind(deleted_doc)
        .execute(&pool)
        .await?;
    sqlx::query(
        "INSERT INTO chunks (id, kb_id, document_id, seq, text, superseded_at)
         VALUES
           ($1, $4, $5, 0, 'alpha UTOPIA_HOSTED_SEARCH_MARKER omega', NULL),
           ($2, $4, $5, 1, 'UTOPIA_HOSTED_SEARCH_MARKER stale', now()),
           ($3, $4, $6, 0, 'UTOPIA_HOSTED_SEARCH_MARKER deleted', NULL)",
    )
    .bind(wanted)
    .bind(superseded)
    .bind(deleted)
    .bind(kb)
    .bind(live_doc)
    .bind(deleted_doc)
    .execute(&pool)
    .await?;

    let result =
        utopia_store::hosted_search::lexical_search(&pool, kb, "UTOPIA_HOSTED_SEARCH_MARKER", 10)
            .await;

    sqlx::query("DELETE FROM organizations WHERE id = $1")
        .bind(org)
        .execute(&pool)
        .await?;

    assert_eq!(result?, vec![wanted]);
    Ok(())
}

#[tokio::test]
async fn hosted_search_treats_blank_queries_as_no_results() -> anyhow::Result<()> {
    let Some(url) = utopia_store::test_db::url() else {
        return Ok(());
    };
    let pool = PgPool::connect(&url).await?;
    let result = utopia_store::hosted_search::lexical_search(&pool, Uuid::nil(), "   ", 10).await?;
    assert!(result.is_empty());
    Ok(())
}
