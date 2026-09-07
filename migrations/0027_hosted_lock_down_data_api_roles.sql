-- Supabase exposes tables in `public` through its Data API roles by default.
-- Utopia's hosted deployment uses direct Postgres through the Rust service and
-- does not use Supabase REST/GraphQL. Revoke browser-role access when those
-- Supabase roles exist, while remaining portable to ordinary Postgres installs.
DO $$
DECLARE
    role_name text;
    has_postgres_role boolean;
BEGIN
    SELECT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'postgres')
      INTO has_postgres_role;

    FOR role_name IN
        SELECT rolname FROM pg_roles WHERE rolname IN ('anon', 'authenticated')
    LOOP
        EXECUTE format(
            'REVOKE ALL PRIVILEGES ON ALL TABLES IN SCHEMA public FROM %I',
            role_name
        );
        EXECUTE format(
            'REVOKE ALL PRIVILEGES ON ALL SEQUENCES IN SCHEMA public FROM %I',
            role_name
        );

        IF has_postgres_role THEN
            EXECUTE format(
                'ALTER DEFAULT PRIVILEGES FOR ROLE postgres IN SCHEMA public REVOKE ALL ON TABLES FROM %I',
                role_name
            );
            EXECUTE format(
                'ALTER DEFAULT PRIVILEGES FOR ROLE postgres IN SCHEMA public REVOKE ALL ON SEQUENCES FROM %I',
                role_name
            );
        END IF;
    END LOOP;
END
$$;
