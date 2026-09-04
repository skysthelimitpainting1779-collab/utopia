#!/usr/bin/env bash
# Hosted end-to-end smoke: auth -> Private Blob -> Workflow -> Postgres chunks -> search.
#
# Required:
#   BASE_URL=https://<preview-or-production>.vercel.app
#   UTOPIA_CONTROL_PLANE_TOKEN=<same secret configured on both Vercel services>
#
# Optional for protected previews:
#   VERCEL_AUTOMATION_BYPASS_SECRET=<deployment-protection bypass secret>
#
# Optional persistence check after a redeploy/new deployment:
#   POST_DEPLOY_BASE_URL=https://<new-deployment>.vercel.app
set -euo pipefail

BASE="${BASE_URL:-${1:-}}"
TOKEN="${UTOPIA_CONTROL_PLANE_TOKEN:-}"
BYPASS="${VERCEL_AUTOMATION_BYPASS_SECRET:-}"
POST_DEPLOY_BASE="${POST_DEPLOY_BASE_URL:-}"

[ -n "$BASE" ] || { echo "BASE_URL is required" >&2; exit 2; }
[ -n "$TOKEN" ] || { echo "UTOPIA_CONTROL_PLANE_TOKEN is required" >&2; exit 2; }
BASE="${BASE%/}"
POST_DEPLOY_BASE="${POST_DEPLOY_BASE%/}"

JAR="$(mktemp)"
BODY="$(mktemp)"
DOCDIR="$(mktemp -d)"
HEADERS="$(mktemp)"
trap 'rm -f "$JAR" "$BODY" "$HEADERS"; rm -rf "$DOCDIR"' EXIT

MARKER="UTOPIA_HOSTED_SMOKE_$(date +%s)_${RANDOM}"
EMAIL="hosted-smoke-$(date +%s)-${RANDOM}@test.local"
DOC="$DOCDIR/hosted-smoke.md"

curl_base=(curl -fsS)
if [ -n "$BYPASS" ]; then
  curl_base+=(
    -H "x-vercel-protection-bypass: $BYPASS"
    -H "x-vercel-set-bypass-cookie: true"
  )
fi

request() {
  "${curl_base[@]}" "$@"
}

json_get() {
  local expression="$1"
  python3 -c "import json,sys; value=json.load(sys.stdin); print($expression)"
}

step() { printf '\n--- %s\n' "$1"; }
fail() { echo "FAIL: $1" >&2; exit 1; }

step "Rust health"
request "$BASE/api/v1/health" | grep -q '"status":"ok"' || fail "Rust health"

step "Control-plane health"
request "$BASE/control/health" | grep -q '"status":"ok"' || fail "control-plane health"

step "Register hosted smoke account"
printf '{"email":"%s","password":"password123","display_name":"Hosted Smoke"}' "$EMAIL" > "$BODY"
request -c "$JAR" -H 'Content-Type: application/json' --data-binary @"$BODY" \
  "$BASE/api/v1/auth/register" | grep -q '"user"' || fail "register"

step "Create isolated workspace"
printf '{"name":"Hosted Smoke WS"}' > "$BODY"
WS_JSON=$(request -b "$JAR" -H 'Content-Type: application/json' --data-binary @"$BODY" \
  "$BASE/api/v1/workspaces")
WS_ID=$(printf '%s' "$WS_JSON" | json_get 'value["id"]')
[ -n "$WS_ID" ] || fail "workspace id missing"

step "Create isolated knowledge base"
printf '{"name":"Hosted Smoke KB"}' > "$BODY"
KB_JSON=$(request -b "$JAR" -H 'Content-Type: application/json' --data-binary @"$BODY" \
  "$BASE/api/v1/workspaces/$WS_ID/kbs")
KB_ID=$(printf '%s' "$KB_JSON" | json_get 'value["id"]')
[ -n "$KB_ID" ] || fail "knowledge-base id missing"

step "Upload content-addressed test document"
printf '# Hosted Utopia\n\n%s proves durable Blob, Workflow, Postgres, and search integration.\n' "$MARKER" > "$DOC"
SHA=$(python3 -c 'import hashlib,sys; print(hashlib.sha256(open(sys.argv[1], "rb").read()).hexdigest())' "$DOC")
UPLOAD_JSON=$(request -b "$JAR" -F "files=@$DOC" "$BASE/api/v1/kbs/$KB_ID/documents")
DOC_ID=$(printf '%s' "$UPLOAD_JSON" | json_get 'value["created"][0]["id"]')
[ -n "$DOC_ID" ] || fail "document id missing"

step "Verify Private Blob object at files/{sha256}"
printf '{"pathname":"files/%s","operation":"head"}' "$SHA" > "$BODY"
PRESIGN_JSON=$(request \
  -H "Authorization: Bearer $TOKEN" \
  -H 'Content-Type: application/json' \
  --data-binary @"$BODY" \
  "$BASE/control/blob/presign")
HEAD_URL=$(printf '%s' "$PRESIGN_JSON" | json_get 'value["presignedUrl"]')
[ -n "$HEAD_URL" ] || fail "presigned HEAD URL missing"
HEAD_CODE=$(request -I -o /dev/null -w '%{http_code}' "$HEAD_URL")
case "$HEAD_CODE" in 2*|3*) ;; *) fail "Private Blob HEAD returned $HEAD_CODE";; esac

step "Start durable drain Workflow"
WORKFLOW_JSON=$(request -X POST -H "Authorization: Bearer $TOKEN" "$BASE/control/jobs/tick")
RUN_ID=$(printf '%s' "$WORKFLOW_JSON" | json_get 'value["runId"]')
[ -n "$RUN_ID" ] || fail "Workflow run id missing"

step "Wait for document processing"
STATUS=""
for _ in $(seq 1 90); do
  DOCS_JSON=$(request -b "$JAR" "$BASE/api/v1/kbs/$KB_ID/documents?limit=200")
  STATUS=$(printf '%s' "$DOCS_JSON" | DOC_ID="$DOC_ID" python3 -c '
import json,os,sys
value=json.load(sys.stdin)
for doc in value.get("docs", []):
    if doc.get("id") == os.environ["DOC_ID"]:
        print(doc.get("status", ""))
        break
')
  case "$STATUS" in
    ready) break ;;
    failed) fail "document processing failed" ;;
  esac
  sleep 2
done
[ "$STATUS" = "ready" ] || fail "document processing timed out at status '$STATUS'"

search_marker() {
  local base="$1"
  printf '{"q":"%s"}' "$MARKER" > "$BODY"
  request -b "$JAR" -H 'Content-Type: application/json' --data-binary @"$BODY" \
    "$base/api/v1/kbs/$KB_ID/search" | grep -q "$MARKER"
}

step "Verify Postgres-authoritative hosted search"
search_marker "$BASE" || fail "unique marker was not returned by search"

if [ -n "$POST_DEPLOY_BASE" ]; then
  step "Verify persistence through a new deployment URL"
  request "$POST_DEPLOY_BASE/api/v1/health" | grep -q '"status":"ok"' || fail "post-deploy health"
  search_marker "$POST_DEPLOY_BASE" || fail "marker missing after redeploy"
fi

cat <<EVIDENCE
=== HOSTED SMOKE PASSED ===
base_url=$BASE
workspace_id=$WS_ID
knowledge_base_id=$KB_ID
document_id=$DOC_ID
sha256=$SHA
workflow_run_id=$RUN_ID
marker=$MARKER
persistence_redeploy_checked=$([ -n "$POST_DEPLOY_BASE" ] && echo true || echo false)
EVIDENCE
