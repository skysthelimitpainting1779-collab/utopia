#!/bin/sh
set -eu

# Prefer Utopia's explicit runtime URL, but consume the standard connection
# variables injected by Vercel Marketplace storage integrations when present.
# Supabase on Vercel provides POSTGRES_URL for the pooled runtime connection.
if [ "${UTOPIA_HOSTED:-}" = "true" ] && [ -z "${UTOPIA_DATABASE_URL:-}" ]; then
  UTOPIA_DATABASE_URL="${POSTGRES_URL:-${POSTGRES_URL_NON_POOLING:-${DATABASE_URL:-}}}"
  export UTOPIA_DATABASE_URL
fi

# Hosted Vercel deployments must never generate the credential sealing key on
# ephemeral disk. Prefer an explicit Vercel environment variable; when it is
# absent, load the persistent 32-byte key from Supabase Vault using the existing
# server-side Postgres connection. The secret is captured into the environment
# and never printed.
if [ "${UTOPIA_HOSTED:-}" = "true" ] && [ -z "${UTOPIA_SECRET_KEY:-}" ]; then
  if [ -z "${UTOPIA_DATABASE_URL:-}" ]; then
    echo "No hosted Postgres connection is configured (UTOPIA_DATABASE_URL/POSTGRES_URL)" >&2
    exit 1
  fi

  key=$(psql "$UTOPIA_DATABASE_URL" -X -A -t -q -v ON_ERROR_STOP=1 \
    -c "select decrypted_secret from vault.decrypted_secrets where name = 'utopia_hosted_seal_key' order by created_at desc limit 1")

  if [ -z "$key" ]; then
    echo "Hosted sealing key not found in Supabase Vault" >&2
    exit 1
  fi

  UTOPIA_SECRET_KEY="$key"
  export UTOPIA_SECRET_KEY
  unset key
fi

exec /usr/local/bin/utopia-server
