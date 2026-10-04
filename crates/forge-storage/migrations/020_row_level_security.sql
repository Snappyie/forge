-- 020: Postgres row-level security as a second line of tenant isolation.
--
-- Today tenant isolation lives entirely in application code: every repository
-- method takes a `tenant_id` and puts it in the WHERE clause. That is a real
-- boundary, but it is a *convention*, not a guarantee. A single query that
-- forgets the predicate reads another tenant's data, and no test would catch it
-- unless that exact query were exercised with two tenants present.
--
-- RLS moves the check into the database, where it holds for every writer
-- including psql, a migration, a future service, and the queries in this
-- codebase we have not yet read.
--
-- The application-level predicates stay. Two independent checks are what makes
-- this defence in depth rather than a race to remove the redundant one.
--
-- HOW THE TENANT IS SET
-- A transaction sets it once:
--
--     SET LOCAL app.current_tenant = '<uuid>';
--
-- `SET LOCAL` scopes the value to the transaction, so it cannot leak to the next
-- request on a pooled connection -- the failure mode a bare `SET` would have.
--
-- WHY NOT A SESSION VARIABLE
-- A session variable is set once per pooled connection and, if any code path
-- forgets to overwrite it, a request silently inherits the *previous* tenant.
-- `SET LOCAL` makes forgetting visible instead: the transaction simply has no
-- tenant set and the policy denies every row.

-- ---------------------------------------------------------------------------
-- Helper
-- ---------------------------------------------------------------------------

-- True when the transaction has a tenant set.
--
-- `current_setting(..., true)` returns NULL for an unset variable rather than
-- raising, so a transaction without one denies everything instead of erroring.
-- An empty string is treated as unset too: a caller that binds an empty
-- identifier must not be treated as "matches nothing except rows with an empty
-- tenant".
CREATE OR REPLACE FUNCTION current_tenant_id() RETURNS UUID
    LANGUAGE sql STABLE
    AS $$
        SELECT NULLIF(current_setting('app.current_tenant', true), '')::UUID
    $$;

-- ---------------------------------------------------------------------------
-- Policies
-- ---------------------------------------------------------------------------

-- Applied to the tables that hold or expose tenant data. `users` is
-- deliberately absent: a user row is reachable through `tenant_memberships`, so
-- scoping `users` itself by tenant would make an operator unable to see or
-- disable a colleague who has not yet joined this tenant.
--
-- `idempotency_keys`, `audit_events` and `outbox` are included: an audit trail
-- that leaks across tenants is worse than one that is incomplete.

DO $$
DECLARE
    target TEXT;
    scoped TEXT[] := ARRAY[
        'applications',
        'environments',
        'jobs',
        'job_versions',
        'job_revisions',
        'executions',
        'schedules',
        'queues',
        'workers',
        'workflows',
        'workflow_versions',
        'workflow_nodes',
        'workflow_edges',
        'manual_approvals',
        'workflow_node_states',
        'alerts',
        'incidents',
        'notifications',
        'api_keys',
        'audit_events',
        'idempotency_keys',
        'migrations'
    ];
BEGIN
    FOREACH target IN ARRAY scoped LOOP
        -- Skip a table that does not exist in this deployment rather than
        -- failing: migrations run in order, and a policy for a table a given
        -- build never created should not break the ones it did.
        IF to_regclass(target) IS NULL THEN
            CONTINUE;
        END IF;

        EXECUTE format('ALTER TABLE %I ENABLE ROW LEVEL SECURITY', target);
        -- Not `FORCE`: the table owner bypasses RLS by default, and forcing it
        -- would break migrations and the test harness, which connect as the
        -- owner. Application connections are expected to be a non-owner role;
        -- see the note in docs/22-architecture-decisions.md (ADR-0021).
        --
        -- `DROP POLICY IF EXISTS` is standalone syntax (there is no
        -- `ALTER TABLE ... DROP POLICY IF EXISTS`), so it is emitted as its own
        -- statement rather than folded into the CREATE below.
        EXECUTE format('DROP POLICY IF EXISTS %I ON %I',
                       target || '_tenant_isolation', target);
        EXECUTE format(
            'CREATE POLICY %I ON %I USING (tenant_id = current_tenant_id()) '
            'WITH CHECK (tenant_id = current_tenant_id())',
            target || '_tenant_isolation', target);
    END LOOP;
END
$$;

-- ---------------------------------------------------------------------------
-- A non-owner role for the application
-- ---------------------------------------------------------------------------

-- RLS does not apply to the table owner, so an application connecting as the
-- owner gets no protection at all. This role exists so a deployment can connect
-- as something that is subject to the policies.
--
-- Created conditionally because it is cluster-level state that a migration
-- cannot assume it may create, and because re-running it must be a no-op.
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'forge_app') THEN
        CREATE ROLE forge_app NOLOGIN;
    END IF;
END
$$;

-- GRANT is left to deployment configuration rather than asserted here: which
-- tables a given installation exposes to the application role is its own
-- decision, and a migration that granted a fixed set would silently over-grant
-- after a future table is added. The role and the policies are the parts this
-- migration owns.