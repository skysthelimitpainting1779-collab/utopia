#!/bin/sh
set -eu

# Hosted Vercel deployments must never generate the credential sealing key on
# ephemeral disk. Prefer an explicit Vercel environment variable; when it is
# absent, load the persistent 32-byte key from Supabase Vault using the existing
# server-side Postgres connection. The secret is captured into the environment
# and never printed.
if [ "${UTOPIA_HOSTED:-}" = "true" ] && [ -z "${UTOPIA_SECRET_KEY:-}" ]; then
  if [ -z "${UTOPIA_DATABASE_URL:-}" ]; then
    echo "UTOPIA_DATABASE_URL is required to load the hosted sealing key" >&2
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
