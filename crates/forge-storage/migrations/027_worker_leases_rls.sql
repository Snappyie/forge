-- Row-level security for `worker_leases`.
--
-- `worker_leases` is tenant-scoped by construction - it carries a `tenant_id`
-- and every path to it starts from an execution, which is tenant-scoped - but it
-- was absent from the table list in migration 020. That left it relying on every
-- query remembering a predicate, which is the convention these policies exist to
-- stop depending on.
--
-- It is not hypothetical. `LeaseRepository::claim_expired` - the query the reaper
-- runs on every tick - carried no `tenant_id` predicate, and
-- `active_for_execution` was scoped only by an execution id the caller supplied,
-- so a caller could ask about another tenant's lease. Both now say which tenant
-- they mean. This migration is the backstop, so a future query that forgets fails
-- closed instead of leaking.
--
-- `users` stays deliberately unprotected: a user row is reachable through
-- `tenant_memberships`, so scoping `users` itself would stop an operator seeing a
-- colleague who has not yet joined their tenant.

-- Wrapped in a loop so `CONTINUE` is legal. `CONTINUE` is a PL/pgSQL loop
-- construct, not a general "skip the rest"; using it in a bare block raises
-- `CONTINUE cannot be used outside a loop` at migration time. One element, one
-- iteration.
DO $$
DECLARE
    target TEXT;
BEGIN
    FOREACH target IN ARRAY ARRAY['worker_leases']::TEXT[]
    LOOP
        -- Skip a table that does not exist in this deployment, for the same
        -- reason migration 020 does: migrations run in order, and a policy for a
        -- table a given build never created should not break the ones it did.
        IF to_regclass(target) IS NULL THEN
            CONTINUE;
        END IF;

        EXECUTE format('ALTER TABLE %I ENABLE ROW LEVEL SECURITY', target);

        -- Deliberately not `FORCE`, for the reason given in migration 020: the
        -- table owner bypasses RLS by default, and forcing it would break
        -- migrations and the test harness, which connect as the owner. Application
        -- connections are expected to be a non-owner role (ADR-0021).
        --
        -- Re-run: the policy is dropped first, because `CREATE POLICY` has no
        -- `IF NOT EXISTS`.
        EXECUTE format('DROP POLICY IF EXISTS %I ON %I',
                       target || '_tenant_isolation', target);

        EXECUTE format($policy$
            CREATE POLICY %I ON %I
                USING (tenant_id = current_tenant_id())
                WITH CHECK (tenant_id = current_tenant_id())
        $policy$, target || '_tenant_isolation', target);
    END LOOP;
END
$$;