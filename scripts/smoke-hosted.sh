#!/usr/bin/env bash
# Hosted end-to-end smoke: auth -> Private Blob -> Workflow -> Postgres chunks -> search.
#
# Required:
#   BASE_URL=https://<preview-or-production>.vercel.app
#
# Preview CI:
#   VERCEL_TRUSTED_OIDC_TOKEN=<short-lived GitHub Actions OIDC token>
#   EXPECTED_SHA=<exact preview commit>
#
# Production/manual smoke may instead provide:
#   UTOPIA_CONTROL_PLANE_TOKEN=<configured internal control token>
#
# Optional cross-deployment persistence check:
#   POST_DEPLOY_BASE_URL=https://<other-deployment>.vercel.app
set -euo pipefail

BASE="${BASE_URL:-${1:-}}"
STATIC_TOKEN="${UTOPIA_CONTROL_PLANE_TOKEN:-}"
OIDC_TOKEN="${VERCEL_TRUSTED_OIDC_TOKEN:-}"
EXPECTED_SHA="${EXPECTED_SHA:-}"
POST_DEPLOY_BASE="${POST_DEPLOY_BASE_URL:-}"
AUTH_TOKEN="${STATIC_TOKEN:-$OIDC_TOKEN}"

[ -n "$BASE" ] || { echo "BASE_URL is required" >&2; exit 2; }
[ -n "$AUTH_TOKEN" ] || {
  echo "UTOPIA_CONTROL_PLANE_TOKEN or VERCEL_TRUSTED_OIDC_TOKEN is required" >&2
  exit 2
}
BASE="${BASE%/}"
POST_DEPLOY_BASE="${POST_DEPLOY_BASE%/}"

JAR="$(mktemp)"
JAR2="$(mktemp)"
BODY="$(mktemp)"
DOCDIR="$(mktemp -d)"
trap 'rm -f "$JAR" "$JAR2" "$BODY"; rm -rf "$DOCDIR"' EXIT

MARKER="UTOPIA_HOSTED_SMOKE_$(date +%s)_${RANDOM}"
EMAIL="hosted-smoke-$(date +%s)-${RANDOM}@test.local"
PASSWORD="$(python3 -c 'import secrets; print(secrets.token_urlsafe(32))')"
DOC="$DOCDIR/hosted-smoke.md"

curl_base=(curl -fsS --connect-timeout 10 --max-time 60)
if [ -n "$OIDC_TOKEN" ]; then
  curl_base+=(-H "x-vercel-trusted-oidc-idp-token: $OIDC_TOKEN")
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

step "Hosted service health"
CONTROL_JSON=$(request "$BASE/control/health")
printf '%s' "$CONTROL_JSON" | grep -q '"status":"ok"' || fail "control-plane health"
request "$BASE/control/blob/health" | grep -q '"status":"ok"' || fail "blob-control health"
request "$BASE/api/v1/health" | grep -q '"status":"ok"' || fail "Rust health"
if [ -n "$EXPECTED_SHA" ]; then
  ACTUAL_SHA=$(printf '%s' "$CONTROL_JSON" | json_get 'value.get("commit", "")')
  [ "$ACTUAL_SHA" = "$EXPECTED_SHA" ] || fail "branch alias is not serving expected commit"
fi

step "Register isolated smoke account"
printf '{"email":"%s","password":"%s","display_name":"Hosted Smoke"}' "$EMAIL" "$PASSWORD" > "$BODY"
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

DIRECT_BLOB_HEAD=false
if [ -n "$STATIC_TOKEN" ]; then
  step "Verify Private Blob object at files/{sha256}"
  printf '{"pathname":"files/%s","operation":"head"}' "$SHA" > "$BODY"
  PRESIGN_JSON=$(request \
    -H "Authorization: Bearer $STATIC_TOKEN" \
    -H 'Content-Type: application/json' \
    --data-binary @"$BODY" \
    "$BASE/control/blob/presign")
  HEAD_URL=$(printf '%s' "$PRESIGN_JSON" | json_get 'value["presignedUrl"]')
  [ -n "$HEAD_URL" ] || fail "presigned HEAD URL missing"
  HEAD_CODE=$(curl -sS -I -o /dev/null -w '%{http_code}' "$HEAD_URL")
  case "$HEAD_CODE" in 2*|3*) ;; *) fail "Private Blob HEAD returned $HEAD_CODE";; esac
  DIRECT_BLOB_HEAD=true
fi

step "Start durable drain Workflow"
WORKFLOW_JSON=$(request -X POST -H "Authorization: Bearer $AUTH_TOKEN" "$BASE/control/jobs/tick")
RUN_ID=$(printf '%s' "$WORKFLOW_JSON" | json_get 'value["runId"]')
[ -n "$RUN_ID" ] || fail "Workflow run id missing"

step "Wait for document processing"
STATUS=""
for _ in $(seq 1 120); do
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
  local jar="$2"
  printf '{"q":"%s"}' "$MARKER" > "$BODY"
  request -b "$jar" -H 'Content-Type: application/json' --data-binary @"$BODY" \
    "$base/api/v1/kbs/$KB_ID/search" | grep -q "$MARKER"
}

step "Verify Postgres-authoritative hosted search"
search_marker "$BASE" "$JAR" || fail "unique marker was not returned by search"

PERSISTENCE=false
if [ -n "$POST_DEPLOY_BASE" ]; then
  step "Verify persistence through another deployment"
  request "$POST_DEPLOY_BASE/api/v1/health" | grep -q '"status":"ok"' || fail "post-deploy health"
  printf '{"email":"%s","password":"%s"}' "$EMAIL" "$PASSWORD" > "$BODY"
  request -c "$JAR2" -H 'Content-Type: application/json' --data-binary @"$BODY" \
    "$POST_DEPLOY_BASE/api/v1/auth/login" | grep -q '"user"' || fail "post-deploy login"
  search_marker "$POST_DEPLOY_BASE" "$JAR2" || fail "marker missing through other deployment"
  PERSISTENCE=true
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
direct_blob_head_checked=$DIRECT_BLOB_HEAD
persistence_deployment_checked=$PERSISTENCE
EVIDENCE
