-- Pin function search paths so helper functions cannot resolve through a
-- caller-controlled mutable path. The fact helper qualifies its table because
-- an empty search_path is intentional.
CREATE OR REPLACE FUNCTION public.audit_events_immutable()
RETURNS trigger
LANGUAGE plpgsql
SET search_path = ''
AS $function$
BEGIN
    RAISE EXCEPTION 'audit_events is append-only (attempted %)', TG_OP
        USING HINT = 'Audit records cannot be modified or deleted.';
END;
$function$;

CREATE OR REPLACE FUNCTION public.fact_surface_predicate(fact uuid)
RETURNS text
LANGUAGE sql
STABLE
SET search_path = ''
AS $function$
    SELECT e.proposed_predicate
      FROM public.fact_evidence e
     WHERE e.fact_id = fact
     ORDER BY e.created_at DESC
     LIMIT 1
$function$;
