-- 006_schedule_claiming.sql
--
-- Adds the claim markers the scheduler uses to coordinate concurrent
-- schedulers (spec 09.3).
--
-- A `SELECT ... FOR UPDATE SKIP LOCKED` alone is not a claim: the row lock is
-- released as soon as the implicit transaction ends, so two schedulers both
-- receive the same schedule. Writing a lease marker inside the claiming
-- transaction makes the claim durable, and `lease_expires_at` bounds how long a
-- crashed scheduler can hold a schedule before another may take it over.

ALTER TABLE schedules
    ADD COLUMN IF NOT EXISTS last_claimed_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS claimed_by UUID,
    ADD COLUMN IF NOT EXISTS lease_expires_at TIMESTAMPTZ;

-- The claiming query filters on `enabled`, `next_run_at` and
-- `lease_expires_at` together; this index supports it directly and replaces
-- the earlier partial index, which did not account for the lease.
DROP INDEX IF EXISTS idx_schedules_enabled_next_run;

CREATE INDEX IF NOT EXISTS idx_schedules_claimable
    ON schedules (next_run_at)
    WHERE enabled = TRUE AND lease_expires_at IS NULL;

-- Observability: which scheduler currently holds a claim.
CREATE INDEX IF NOT EXISTS idx_schedules_claimed_by
    ON schedules (claimed_by)
    WHERE claimed_by IS NOT NULL;