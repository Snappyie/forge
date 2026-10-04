-- 018: constraints the code already depends on.
--
-- `ExecutionRepository::create` calls `default_queue_id`, which provisions a
-- tenant's `default` queue with `INSERT ... ON CONFLICT (tenant_id, name) DO
-- NOTHING`. Postgres requires a unique index to arbitrate an ON CONFLICT
-- target, and no migration ever created one — so every path that created an
-- execution for a tenant without a pre-existing `default` queue failed with
--
--     42P10: there is no unique or exclusion constraint matching
--            the ON CONFLICT specification
--
-- The SELECT-before-INSERT ahead of it hid the bug whenever the queue already
-- existed, which is why it passed on a seeded database and failed on a fresh
-- one (35 integration tests across four suites).
--
-- The constraint is also the guarantee the surrounding comment claims: two
-- concurrent inserts of the same tenant's default queue cannot both win, so
-- `fetch_one` afterwards always returns exactly one row. Without it the code
-- relies on that SELECT winning the race, which is exactly what it does not.

CREATE UNIQUE INDEX IF NOT EXISTS queues_tenant_name_unique
    ON queues (tenant_id, name);

-- Tenant slugs become the HTTP-facing identifier (a tenant is addressed as
-- /tenants/{slug}), so they must be unique and stable. The value is derived
-- from the tenant's id, which is unique by construction, so no existing call
-- site has to change: several code paths insert a tenant row without naming a
-- slug, and a slug that must be supplied in Rust is a slug some caller will
-- forget. An operator can still set an explicit, human-readable slug.
ALTER TABLE tenants ADD COLUMN IF NOT EXISTS slug TEXT;

-- Populated by a BEFORE INSERT trigger rather than a DEFAULT.
--
-- Postgres rejects a column reference in a DEFAULT expression outright
-- (`cannot use column reference in DEFAULT expression`), so a DEFAULT cannot
-- derive a slug from the row's own id — not even through a function call,
-- because the column is still named in the DEFAULT. A trigger has access to
-- the whole row and is the mechanism Postgres actually provides for this.
CREATE OR REPLACE FUNCTION tenants_assign_slug() RETURNS TRIGGER
    LANGUAGE plpgsql
    AS $$
    BEGIN
        -- An operator-supplied slug always wins; this only fills the blank.
        IF NEW.slug IS NULL OR NEW.slug = '' THEN
            NEW.slug := 't-' || replace(NEW.id::TEXT, '-', '');
        END IF;
        RETURN NEW;
    END;
    $$;

DROP TRIGGER IF EXISTS tenants_assign_slug_trg ON tenants;
CREATE TRIGGER tenants_assign_slug_trg
    BEFORE INSERT ON tenants
    FOR EACH ROW
    EXECUTE FUNCTION tenants_assign_slug();

-- Backfill before the column is made mandatory, otherwise the NOT NULL below
-- fails on any tenant created before this migration.
UPDATE tenants
   SET slug = 't-' || replace(id::text, '-', '')
 WHERE slug IS NULL OR slug = '';

ALTER TABLE tenants ALTER COLUMN slug SET NOT NULL;

-- Names collide ("Payments", "Payments 2"); slugs cannot, because an operator
-- chooses them and gets a clear error rather than a silent suffix.
CREATE UNIQUE INDEX IF NOT EXISTS tenants_slug_unique ON tenants (slug);

-- A tenant must be able to be suspended without losing its history, so status
-- is explicit rather than inferred from the absence of rows.
ALTER TABLE tenants ADD COLUMN IF NOT EXISTS status TEXT NOT NULL DEFAULT 'ACTIVE';

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'tenants_status_check'
    ) THEN
        ALTER TABLE tenants
            ADD CONSTRAINT tenants_status_check
            CHECK (status IN ('ACTIVE', 'SUSPENDED', 'ARCHIVED'));
    END IF;
END
$$;