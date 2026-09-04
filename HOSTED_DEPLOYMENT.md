# Utopia Hosted Deployment

This edition preserves Utopia's Rust domain engine and existing Vite UI while replacing host-local persistence and permanent process loops with managed infrastructure.

## Runtime topology

```text
Browser
  |
  v
Vercel project
  |- /control/*  -> Next.js control service
  |                  |- Vercel Workflow
  |                  `- Private Blob signed-URL broker
  `- /*          -> Rust/Axum container service
                     |- Supabase Postgres + pgvector
                     `- Vercel Private Blob through BlobStore
```

The control service receives a private service binding named `UTOPIA_INTERNAL_URL` for the Rust service. Rust reaches the Blob signing endpoint through `UTOPIA_CONTROL_PLANE_URL`; when it is absent, the server derives the same-deployment public origin from `VERCEL_URL`.

## Required resources

1. A dedicated Supabase Postgres project or database for Utopia.
2. A Vercel project importing this repository and using the root `vercel.json`.
3. A **Private** Vercel Blob store attached to the Vercel project.
4. Vercel Workflow enabled for the project.

Do not point migrations at an unrelated database.

## Required environment variables

Set these for Preview and Production unless a narrower scope is intentional.

| Variable | Service | Secret | Purpose |
|---|---|---:|---|
| `UTOPIA_DATABASE_URL` | Rust | yes | Pooled runtime Supabase Postgres connection |
| `UTOPIA_MIGRATION_URL` | deploy/CLI | yes | Direct or migration-role Postgres connection |
| `UTOPIA_SECRET_KEY` | Rust | yes | 32-byte AES credential-sealing key; generate once and retain |
| `UTOPIA_JWT_SECRET` | Rust | yes | Shared multi-instance JWT signing secret |
| `UTOPIA_CONTROL_PLANE_TOKEN` | both | yes | Bearer token for internal tick and Blob signing calls |
| `CRON_SECRET` | control | yes | Vercel Cron authorization token |

The container image supplies these hosted defaults:

```dotenv
UTOPIA_HOSTED=true
UTOPIA_MIGRATE_ON_STARTUP=false
UTOPIA_BLOB_BACKEND=vercel
UTOPIA_LEXICAL_BACKEND=postgres
UTOPIA_DATA_DIR=/tmp/utopia
UTOPIA_DB_MAX_CONNECTIONS=8
```

Generate secrets locally without printing them into logs:

```bash
openssl rand -hex 32      # UTOPIA_SECRET_KEY
openssl rand -base64 48   # UTOPIA_JWT_SECRET
openssl rand -hex 32      # UTOPIA_CONTROL_PLANE_TOKEN
openssl rand -hex 32      # CRON_SECRET
```

`UTOPIA_SECRET_KEY` encrypts stored connection strings and API keys. Losing or casually rotating it makes already sealed values unreadable.

## Database migration

The hosted container deliberately does not run DDL on every cold start. Run the repository migrations before deploying a version that introduces migrations:

```bash
sqlx migrate run --database-url "$UTOPIA_MIGRATION_URL"
sqlx migrate run --database-url "$UTOPIA_MIGRATION_URL"
```

The second run must be a successful no-op. Verify the required extension and tables:

```sql
select extname from pg_extension where extname = 'vector';
select to_regclass('public.documents');
select to_regclass('public.chunks');
select to_regclass('public.jobs');
select to_regclass('public.knowledge_bases');
```

## Vercel deployment

The root `vercel.json` defines two services:

- `utopia`: `Dockerfile.vercel`, container runtime
- `control`: `control-plane/`, Next.js + Workflow

It routes `/control/*` to the control service and everything else to Rust. The scheduler starts `/control/jobs/tick` once per minute. The route returns immediately with a Workflow run ID; durable steps invoke the bounded Rust tick until the queue is empty or the round cap is reached.

Deploy a preview first:

```bash
vercel link
vercel deploy
```

Inspect both service build logs. Do not promote a preview with a failed service, missing Workflow registration, invalid service binding, or invalid `vercel.json`.

After the preview passes the hosted smoke test:

```bash
vercel deploy --prod
```

## Verification

Repository gates:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace

cd web && pnpm install --frozen-lockfile && pnpm build
cd ../control-plane && pnpm install --frozen-lockfile && pnpm check
```

Hosted end-to-end verification:

```bash
BASE_URL="https://<deployment>" \
UTOPIA_CONTROL_PLANE_TOKEN="<redacted>" \
./scripts/smoke-hosted.sh
```

For a protected preview, also provide `VERCEL_AUTOMATION_BYPASS_SECRET`. To prove persistence across deployments, redeploy and set `POST_DEPLOY_BASE_URL` to the new deployment URL before rerunning the script.

The smoke test proves:

```text
health -> auth -> upload -> Private Blob -> Workflow -> Postgres chunks -> ready -> search
```

## Operational notes

- Postgres is canonical for hosted lexical and vector retrieval. Hosted correctness does not depend on a local Tantivy index.
- Vercel Blob objects are immutable and addressed as `files/{sha256}`.
- The Postgres `jobs` table remains Utopia's business/audit projection. Workflow supplies durable hosted orchestration around bounded queue drains.
- Process-local SSE and live-answer reattachment remain best-effort in the MVP. Canonical data remains in Postgres.
- Uploads through the existing Rust multipart endpoint are suitable only below Vercel's request-body limit. Direct browser-to-Blob large uploads are a separate follow-up.

## Rollback

1. Roll Vercel back to the previous deployment.
2. Keep the dedicated database; migrations are forward-only and should not be destructively reversed.
3. Disable hosted behavior or use the existing Docker deployment when needed.
4. Delete Blob objects only after confirming that no current document or document version references their SHA-256.
