# Vercel Hosted MVP Plan

**Base commit:** `86dbac424eacac34b5c5abd86b42646b7c02d753`

**Goal:** Preserve Utopia's Rust domain engine, ontology, bitemporal graph, review system, audit ledger, auth, Postgres schema, pgvector retrieval, and existing web UI while adapting infrastructure boundaries for Vercel.

## P0 architecture

- Rust/Axum + compiled Vite UI run as a Vercel container service.
- Supabase Postgres supplies the existing SQLx database and `vector` extension.
- Vercel Private Blob stores immutable source bytes at `files/{sha256}` through Utopia's existing `BlobStore` interface.
- Hosted lexical retrieval reads live `chunks.text` from Postgres and fuses with existing pgvector results through the existing RRF function.
- Utopia's existing `jobs` table and `dispatch` semantics remain intact.
- Hosted mode replaces the permanent polling worker with an authenticated, bounded tick/drain endpoint called by Vercel Workflow.
- Existing local Docker behavior remains the default when hosted mode is disabled.

## P0 acceptance gates

1. Existing backend formatting, Clippy, tests, and build pass.
2. Existing web build passes.
3. Hosted control-plane build passes.
4. Existing migrations run twice against a dedicated Postgres database.
5. Hosted health, registration, login, upload, Blob persistence, job execution, chunk persistence, and search are proven end to end.
6. Data remains available after a new deployment or instance.
7. Internal routes reject missing or invalid authorization.
8. No secret is committed or printed.

## Explicit P1 deferrals

- Direct browser-to-Blob uploads above the normal function payload limit.
- PGroonga multilingual search tuning.
- Shared cross-instance SSE/chat reattachment.
- Replacing every Utopia job with a bespoke Workflow.

## Change boundaries

Expected changes are limited to configuration, Blob storage adapter selection, hosted lexical retrieval, finite job execution, internal hosted routes, the TypeScript Workflow control plane, and Vercel deployment configuration. Domain semantics are out of scope.
