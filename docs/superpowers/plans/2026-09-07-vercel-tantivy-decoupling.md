# Vercel Tantivy Decoupling Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete the hosted Utopia port so Vercel/Postgres mode has no runtime dependency on a writable Tantivy index while preserving Tantivy for self-hosted deployments.

**Architecture:** Keep Postgres as the canonical hosted lexical backend and pgvector as the vector backend. Make Tantivy initialization conditional on the configured lexical backend, represent the local index as optional state, and ensure only the self-hosted/Tantivy retrieval path can access it. Hosted startup must succeed even when the local data directory is unwritable.

**Tech Stack:** Rust, Axum, Tantivy, SQLx/Postgres, pgvector, Vercel Services, Vercel Workflow.

**Spec:** `HOSTED_DEPLOYMENT.md`

## Global Constraints

- Preserve existing self-hosted Tantivy behavior.
- Preserve hosted Postgres lexical retrieval and RRF fusion behavior.
- Do not introduce another database, queue, or search service.
- Hosted correctness must not depend on process-local filesystem state.
- Existing Vercel Workflow job orchestration remains unchanged.

---

### Task 1: Make SearchIndex optional in application state

**Files:**
- Modify: `crates/utopia-server/src/state.rs`
- Modify: `crates/utopia-server/src/main.rs`
- Modify: `crates/utopia-server/src/retrieval.rs`

**Interfaces:**
- `AppState.search` becomes `Option<Arc<SearchIndex>>`.
- `AppState::new` accepts `Option<Arc<SearchIndex>>`.
- `lexical_backend=postgres` constructs state with `None`.
- `lexical_backend=tantivy` constructs state with `Some(SearchIndex)`.

- [ ] **Step 1:** Add a state-level test that Postgres lexical mode can construct without a Tantivy index.
- [ ] **Step 2:** Run the focused test and confirm it fails under the current mandatory `Arc<SearchIndex>` API.
- [ ] **Step 3:** Change `AppState.search` and constructor input to `Option<Arc<SearchIndex>>`.
- [ ] **Step 4:** Change startup so `SearchIndex::open` and `reindex_if_empty` run only when `lexical_backend == "tantivy"`.
- [ ] **Step 5:** Change the Tantivy retrieval arm to require the index with an explicit configuration error rather than silently creating one.
- [ ] **Step 6:** Run the focused state/retrieval tests and confirm they pass.

### Task 2: Add hosted no-filesystem regression coverage

**Files:**
- Modify: existing hosted integration/unit test file nearest server startup/configuration.

**Interfaces:**
- Hosted config uses `UTOPIA_LEXICAL_BACKEND=postgres`.
- Test data directory points to a path that cannot be used for index creation.

- [ ] **Step 1:** Add a regression test proving hosted/Postgres mode does not invoke Tantivy initialization.
- [ ] **Step 2:** Run the regression test and confirm it fails before the production change.
- [ ] **Step 3:** Apply the conditional initialization change from Task 1.
- [ ] **Step 4:** Re-run and confirm PASS.

### Task 3: Verify the full hosted port

**Files:**
- Modify: `HOSTED_DEPLOYMENT.md` only if runtime notes need clarification.

- [ ] **Step 1:** Run `cargo fmt --all --check`.
- [ ] **Step 2:** Run `cargo clippy --workspace --all-targets -- -D warnings`.
- [ ] **Step 3:** Run `cargo test --workspace`.
- [ ] **Step 4:** Run `cargo build --workspace`.
- [ ] **Step 5:** Run control-plane checks and existing hosted smoke tests.
- [ ] **Step 6:** Deploy Preview, verify `health -> auth -> upload -> Blob -> Workflow -> Postgres chunks -> ready -> search`.
- [ ] **Step 7:** Confirm runtime logs in Postgres lexical mode no longer show Tantivy index initialization.
- [ ] **Step 8:** Promote only after Preview passes the existing persistence/smoke gates.
