#!/usr/bin/env python3
"""Apply the narrow hosted-runtime integration to main.rs.

Kept as an idempotent, reviewable transform because the upstream file is large and changes
frequently. Every replacement has an explicit anchor and fails closed on source drift.
"""

from pathlib import Path
import textwrap

PATH = Path("crates/utopia-server/src/main.rs")
text = PATH.read_text(encoding="utf-8")


def replace_once(old: str, new: str, label: str) -> None:
    global text
    if new in text:
        return
    if old not in text:
        raise SystemExit(f"source drift: missing anchor for {label}")
    text = text.replace(old, new, 1)


replace_once(
    "mod github_issues;\nmod ingest_sources;",
    "mod github_issues;\nmod hosted;\nmod ingest_sources;",
    "hosted module declaration",
)

replace_once(
    "    reindex_if_empty(&pool, &search).await;",
    "    if cfg.lexical_backend == \"tantivy\" {\n"
    "        reindex_if_empty(&pool, &search).await;\n"
    "    }",
    "hosted lexical startup",
)

replace_once(
    "    let state = AppState::new(pool.clone(), &cfg, search, jwt_secret);",
    "    let state = AppState::new(pool.clone(), &cfg, search, jwt_secret)?;",
    "fallible AppState constructor",
)

start_anchor = "    // 任务分发：新任务类型在这里注册\n"
end_anchor = "    let app = api::router(state, &cfg);\n"
wrapped_marker = "    if !cfg.hosted {\n        // 任务分发：新任务类型在这里注册\n"
if wrapped_marker not in text:
    start = text.find(start_anchor)
    end = text.find(end_anchor)
    if start < 0 or end < 0 or end <= start:
        raise SystemExit("source drift: cannot locate permanent worker/scheduler block")
    prefix = text[:start]
    block = text[start:end]
    suffix = text[end:]
    text = (
        prefix
        + "    if !cfg.hosted {\n"
        + textwrap.indent(block, "    ")
        + "    }\n\n"
        + suffix
    )

replace_once(
    "    let app = api::router(state, &cfg);\n"
    "    let listener = tokio::net::TcpListener::bind(&cfg.bind_addr).await?;\n"
    "    tracing::info!(\"Utopia 服务启动于 http://{}\", cfg.bind_addr);",
    "    let app = api::router(state.clone(), &cfg).merge(hosted::router(state));\n"
    "    let bind_addr = cfg.runtime_bind_addr()?;\n"
    "    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;\n"
    "    tracing::info!(\"Utopia 服务启动于 http://{}\", bind_addr);",
    "hosted router and Vercel PORT",
)

replace_once(
    "    if let Ok(addr) = cfg.bind_addr.parse::<std::net::SocketAddrV4>() {",
    "    if let Ok(addr) = bind_addr.parse::<std::net::SocketAddrV4>() {",
    "runtime IPv4 address",
)

PATH.write_text(text, encoding="utf-8")
print(f"patched {PATH}")
