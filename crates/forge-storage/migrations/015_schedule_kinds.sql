-- One-time and fixed-interval schedules (spec 01.5, 09.2).
--
-- The `schedules` table could record *that* a schedule was ONE_TIME or
-- INTERVAL but not *what made it recurring*: there was no column for the
-- period or the one-time instant, so a non-cron row could only be stored with
-- a NULL cron expression — which `cron_expression`'s NOT NULL constraint
-- rejected outright — and the engine had nothing to evaluate. These columns
-- make the stored configuration self-describing, which is what lets the engine
-- persist, preview and advance all three kinds.
--
-- `sqlx` validates the checksum of every applied migration, so this is a new
-- additive migration rather than an edit to 001/005.

ALTER TABLE schedules
    -- Period in seconds between interval occurrences. NULL for other kinds.
    ADD COLUMN IF NOT EXISTS interval_seconds BIGINT,
    -- The single UTC instant a one-time schedule fires at. NULL otherwise.
    ADD COLUMN IF NOT EXISTS one_time_at TIMESTAMPTZ,
    -- The exact job version a PINNED schedule executes (spec 02.4). Without
    -- this the policy was stored and then ignored: resolution always preferred
    -- the job's current version.
    ADD COLUMN IF NOT EXISTS pinned_version_id UUID
        REFERENCES job_versions(id) ON DELETE SET NULL,
    -- Why the scheduler disabled a schedule on its own. NULL means the
    -- schedule is enabled or an operator paused it (`is_paused` records the
    -- latter); a completed one-shot is disabled, not paused.
    ADD COLUMN IF NOT EXISTS disabled_reason TEXT;

-- A one-time or interval schedule has no cron expression, so the column must
-- be nullable. Applying 005's backfill never depended on it.
ALTER TABLE schedules ALTER COLUMN cron_expression DROP NOT NULL;

COMMENT ON COLUMN schedules.interval_seconds IS
    'Period in seconds for an INTERVAL schedule; NULL for CRON and ONE_TIME.';
COMMENT ON COLUMN schedules.one_time_at IS
    'The single UTC instant a ONE_TIME schedule fires at; NULL for CRON and INTERVAL.';
COMMENT ON COLUMN schedules.pinned_version_id IS
    'The job version a PINNED schedule executes (spec 02.4); NULL for LATEST_PUBLISHED.';
COMMENT ON COLUMN schedules.disabled_reason IS
    'Set when the scheduler disabled the schedule itself (COMPLETED_ONE_TIME, INVALID_CONFIGURATION, NO_FUTURE_OCCURRENCE). NULL when enabled or operator-paused.';
