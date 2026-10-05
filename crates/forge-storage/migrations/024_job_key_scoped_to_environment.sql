-- 024: a job key is unique per environment, not per tenant.
--
-- `jobs.key` was unique across `(tenant_id, key)`, which makes
-- cross-environment migration impossible: the dev copy and the prod copy of one
-- job share a key by definition, because the key is what a migration matches on.
-- Creating the second one fails with a unique violation, so the same job could
-- never exist in both environments of one tenant.
--
-- The constraint becomes `(tenant_id, environment_id, key)`. Two consequences,
-- both intended:
--
--   * the same job can now exist in `dev` and `prod` under one key, which is what
--     a migration creates;
--   * within a single environment the key is still unique, so a deep link
--     `?environment=prod&key=nightly` still resolves to exactly one job.
--
-- Rows whose `environment_id` is null are excluded from the constraint. A job
-- created before environments existed has no environment, and forcing every
-- legacy row to a single sentinel environment would collide them all together.
-- Those rows keep the old behaviour via the partial index below, which falls
-- back to per-tenant uniqueness for exactly the ungrouped case.

-- Drop the tenant-wide constraint first: adding the narrower one while it still
-- exists would fail, because dev/prod copies of a key violate it.
DROP INDEX IF EXISTS idx_jobs_tenant_key_unique;

-- The per-environment uniqueness that replaces it.
CREATE UNIQUE INDEX IF NOT EXISTS idx_jobs_env_key_unique
    ON jobs (tenant_id, environment_id, key)
    WHERE key IS NOT NULL AND environment_id IS NOT NULL;

-- Ungrouped jobs keep one-per-tenant semantics, so an operator who has not yet
-- assigned an environment cannot accidentally create two jobs with one key.
CREATE UNIQUE INDEX IF NOT EXISTS idx_jobs_tenant_key_ungrouped_unique
    ON jobs (tenant_id, key)
    WHERE key IS NOT NULL AND environment_id IS NULL;
