-- Hosted Supabase deployments can keep the credential sealing key in Vault
-- when UTOPIA_SECRET_KEY is not supplied by the hosting environment.
-- Ordinary PostgreSQL installs do not have Supabase Vault; they no-op here and
-- continue to require the normal environment/local-file key path.
DO $$
DECLARE
    matching_count integer;
    created_id uuid;
BEGIN
    IF to_regclass('vault.decrypted_secrets') IS NULL THEN
        RAISE NOTICE 'Supabase Vault unavailable; UTOPIA_SECRET_KEY env remains required';
        RETURN;
    END IF;

    EXECUTE 'SELECT count(*) FROM vault.decrypted_secrets WHERE name = $1'
      INTO matching_count
      USING 'utopia_hosted_seal_key';

    IF matching_count = 0 THEN
        EXECUTE 'SELECT vault.create_secret(encode(extensions.gen_random_bytes(32), ''hex''), $1, $2, NULL)'
          INTO created_id
          USING 'utopia_hosted_seal_key',
                'Utopia hosted credential sealing key';
    END IF;
END
$$;
