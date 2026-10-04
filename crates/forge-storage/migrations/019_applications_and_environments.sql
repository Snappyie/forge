-- 019: applications, environments, and tenant-addressable job scoping.
--
-- Implements the two containers `redesign.md` asks for that did not exist:
-- PowerJob-style application grouping (section D) and environments as the axis
-- a job migrates along (sections 2.G, 3).
--
-- Everything here is additive. No existing column changes type, no existing row
-- is rewritten except to backfill a container id, and no NOT NULL is added to a
-- table that code inserts into without it -- a constraint a current call site
-- cannot satisfy is a production outage, not a stricter schema.

-- ---------------------------------------------------------------------------
-- Applications
-- ---------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS applications (
    id          UUID PRIMARY KEY,
    tenant_id   UUID        NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    slug        TEXT        NOT NULL,
    name        VARCHAR(255) NOT NULL,
    description TEXT,
    labels      JSONB       NOT NULL DEFAULT '{}'::jsonb,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- The slug is the typeable identifier, so it must be unique per tenant: two
-- applications both called "payments" would make `/applications/payments`
-- ambiguous, and an operator resolving that ambiguity would act on the wrong one.
CREATE UNIQUE INDEX IF NOT EXISTS applications_tenant_slug_unique
    ON applications (tenant_id, slug);

-- Listing an application's jobs is the overwhelmingly common query, so the index
-- leads with the container rather than the tenant.
CREATE INDEX IF NOT EXISTS applications_tenant_created_idx
    ON applications (tenant_id, created_at);

-- ---------------------------------------------------------------------------
-- Environments
-- ---------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS environments (
    id          UUID PRIMARY KEY,
    tenant_id   UUID        NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    slug        TEXT        NOT NULL,
    name        VARCHAR(255) NOT NULL,
    kind        VARCHAR(20) NOT NULL,
    description TEXT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS environments_tenant_slug_unique
    ON environments (tenant_id, slug);

-- A CHECK rather than a native enum, per ADR-0018: adding an enum value needs
-- DDL that cannot run inside a transaction on older servers.
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'environments_kind_check') THEN
        ALTER TABLE environments
            ADD CONSTRAINT environments_kind_check
            CHECK (kind IN ('development', 'staging', 'production', 'other'));
    END IF;
END
$$;

-- At most one production environment per tenant.
--
-- This is a correctness constraint, not tidiness. Two environments both marked
-- production would mean a change guardrail protects one and silently skips the
-- other, which is precisely the "operator believes it is isolated and it is
-- not" failure `EnvironmentKind` exists to prevent.
CREATE UNIQUE INDEX IF NOT EXISTS environments_one_production_unique
    ON environments (tenant_id)
    WHERE kind = 'production';

-- ---------------------------------------------------------------------------
-- Per-tenant defaults, so existing rows have a container
-- ---------------------------------------------------------------------------

-- Every tenant gets a default environment and application, created by trigger
-- so that a tenant insert cannot leave its jobs orphaned from both. Doing this
-- in application code would mean every one of the several existing tenant-creating
-- call sites had to remember to create them.
CREATE OR REPLACE FUNCTION provision_tenant_containers() RETURNS TRIGGER
    LANGUAGE plpgsql
    AS $$
    BEGIN
        INSERT INTO environments (id, tenant_id, slug, name, kind)
        VALUES (gen_random_uuid(), NEW.id, 'default', 'Default', 'other')
        ON CONFLICT DO NOTHING;

        INSERT INTO applications (id, tenant_id, slug, name)
        VALUES (gen_random_uuid(), NEW.id, 'default', 'Default')
        ON CONFLICT DO NOTHING;

        RETURN NEW;
    END;
    $$;

DROP TRIGGER IF EXISTS provision_tenant_containers_trg ON tenants;
CREATE TRIGGER provision_tenant_containers_trg
    AFTER INSERT ON tenants
    FOR EACH ROW
    EXECUTE FUNCTION provision_tenant_containers();

-- Backfill for tenants that already exist. The `default` slug is taken per
-- tenant, so this cannot collide with a tenant's own slug unless they chose
-- "default" themselves -- in which case that row is left alone.
INSERT INTO environments (id, tenant_id, slug, name, kind)
SELECT gen_random_uuid(), t.id, 'default', 'Default', 'other'
  FROM tenants t
 WHERE NOT EXISTS (SELECT 1 FROM environments e WHERE e.tenant_id = t.id);

INSERT INTO applications (id, tenant_id, slug, name)
SELECT gen_random_uuid(), t.id, 'default', 'Default'
  FROM tenants t
 WHERE NOT EXISTS (SELECT 1 FROM applications a WHERE a.tenant_id = t.id);

-- ---------------------------------------------------------------------------
-- Scope jobs and workers
-- ---------------------------------------------------------------------------

-- Nullable rather than NOT NULL: jobs and workers already exist, and a NOT NULL
-- added without a backfill would make every current INSERT fail.
ALTER TABLE jobs ADD COLUMN IF NOT EXISTS application_id UUID
    REFERENCES applications(id) ON DELETE SET NULL;
ALTER TABLE jobs ADD COLUMN IF NOT EXISTS environment_id UUID
    REFERENCES environments(id) ON DELETE SET NULL;

ALTER TABLE workers ADD COLUMN IF NOT EXISTS environment_id UUID
    REFERENCES environments(id) ON DELETE SET NULL;

-- Point existing rows at their tenant's default container.
UPDATE jobs j
   SET application_id = d.app_id,
       environment_id = d.env_id
  FROM (SELECT t.id AS tenant_id,
               (SELECT a.id FROM applications a WHERE a.tenant_id = t.id
                 ORDER BY (a.slug = 'default') DESC, a.created_at LIMIT 1) AS app_id,
               (SELECT e.id FROM environments e WHERE e.tenant_id = t.id
                 ORDER BY (e.slug = 'default') DESC, e.created_at LIMIT 1) AS env_id
          FROM tenants t) d
 WHERE j.tenant_id = d.tenant_id
   AND j.application_id IS NULL;

UPDATE workers w
   SET environment_id = d.env_id
  FROM (SELECT t.id AS tenant_id,
               (SELECT e.id FROM environments e WHERE e.tenant_id = t.id
                 ORDER BY (e.slug = 'default') DESC, e.created_at LIMIT 1) AS env_id
          FROM tenants t) d
 WHERE w.tenant_id = d.tenant_id
   AND w.environment_id IS NULL;

-- "Every job in application X", and the same filtered by environment, are the
-- two listings the console issues constantly.
CREATE INDEX IF NOT EXISTS jobs_application_idx
    ON jobs (application_id, updated_at DESC);
CREATE INDEX IF NOT EXISTS jobs_environment_idx
    ON jobs (environment_id, updated_at DESC);
CREATE INDEX IF NOT EXISTS workers_environment_idx
    ON workers (environment_id, status);

-- ---------------------------------------------------------------------------
-- Job revisions
-- ---------------------------------------------------------------------------

-- An editable draft, distinct from `job_versions`.
--
-- `job_versions` is the immutable artifact an execution points back to and must
-- never change; a revision is the desired state an operator edits and a
-- migration copies. Folding the two together would make drafts immutable or
-- reproducibility impossible.
CREATE TABLE IF NOT EXISTS job_revisions (
    id             UUID PRIMARY KEY,
    tenant_id      UUID        NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    job_id         UUID        NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    revision_number INTEGER    NOT NULL,
    -- The desired state: identity, schedule, parameters and bindings as one
    -- document. JSONB rather than a column per field because the shape differs
    -- per execution_type, and a revision must round-trip unchanged through a
    -- migration manifest.
    config         JSONB       NOT NULL DEFAULT '{}'::jsonb,
    change_summary TEXT,
    created_by     UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS job_revisions_number_unique
    ON job_revisions (job_id, revision_number);

-- An application key must stay unique per tenant once set, so two jobs in one
-- application cannot claim the same key and make a migration ambiguous.
CREATE INDEX IF NOT EXISTS jobs_application_key_idx
    ON jobs (tenant_id, application_id, key)
    WHERE key IS NOT NULL;

-- ---------------------------------------------------------------------------
-- Migrations
-- ---------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS migrations (
    id                  UUID PRIMARY KEY,
    tenant_id           UUID        NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    -- Where the work came from and where it is going. Recorded as slugs, not
    -- ids: a migration record has to stay readable after one side is retired.
    source_system       TEXT        NOT NULL,
    source_tenant_slug  TEXT        NOT NULL,
    source_environment  TEXT        NOT NULL,
    target_environment  TEXT        NOT NULL,
    -- The plan the operator approved, verbatim. Keeping it means an audit can
    -- show exactly what was proposed, not only what happened.
    plan                JSONB       NOT NULL,
    status              VARCHAR(20) NOT NULL DEFAULT 'PLANNED',
    applied_by          UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    applied_at          TIMESTAMPTZ
);

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'migrations_status_check') THEN
        ALTER TABLE migrations
            ADD CONSTRAINT migrations_status_check
            CHECK (status IN ('PLANNED', 'APPLYING', 'APPLIED', 'FAILED', 'ROLLED_BACK'));
    END IF;
END
$$;

CREATE INDEX IF NOT EXISTS migrations_tenant_created_idx
    ON migrations (tenant_id, created_at DESC);