-- 010_sla_job_health_maintenance.sql
--
-- UI.md sections 31 (SLA UI), 32/79 (job health), 71 (maintenance mode), and
-- 73 (system health) all need stored facts to display. Without these the console
-- would have to invent numbers, which is what this migration exists to prevent.

-- An SLA target for a job: the window it is expected to run in, and how late a
-- completion may be before it counts as a violation (UI.md section 31).
CREATE TABLE IF NOT EXISTS sla_targets (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    job_id UUID NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    -- How late the job is permitted to finish, e.g. 'PT30M'.
    target_duration_seconds INT NOT NULL,
    -- Whether a run that finishes inside the target counts as compliant.
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT sla_targets_duration_check CHECK (target_duration_seconds > 0),
    -- One target per job; the spec treats SLA as a property of the job.
    CONSTRAINT sla_targets_job_unique UNIQUE (tenant_id, job_id)
);

CREATE INDEX IF NOT EXISTS idx_sla_targets_enabled
    ON sla_targets (tenant_id) WHERE enabled = TRUE;

-- Per-execution SLA outcome, computed once when the execution reaches a
-- terminal state. Storing it keeps the dashboard a cheap read instead of a
-- scan over every execution (UI.md section 31).
CREATE TABLE IF NOT EXISTS sla_evaluations (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    job_id UUID NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    execution_id UUID NOT NULL REFERENCES executions(id) ON DELETE CASCADE,
    -- Measured wall-clock duration, excluding queue wait.
    duration_seconds INT NOT NULL,
    met BOOLEAN NOT NULL,
    evaluated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- One evaluation per execution: re-running the evaluator must not duplicate.
    CONSTRAINT sla_evaluations_execution_unique UNIQUE (execution_id)
);

CREATE INDEX IF NOT EXISTS idx_sla_evaluations_job_time
    ON sla_evaluations (tenant_id, job_id, evaluated_at DESC);

CREATE INDEX IF NOT EXISTS idx_sla_evaluations_missed
    ON sla_evaluations (tenant_id, evaluated_at DESC) WHERE met = FALSE;

-- Tenant-wide maintenance mode (UI.md section 71). While set, scheduling is
-- held back; the flag is deliberately a single row per tenant.
CREATE TABLE IF NOT EXISTS maintenance_windows (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    reason TEXT NOT NULL,
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    ended_at TIMESTAMPTZ,
    -- Who put the tenant into maintenance, for the audit trail.
    started_by UUID
);

-- At most one open window per tenant.
CREATE UNIQUE INDEX IF NOT EXISTS idx_maintenance_windows_open
    ON maintenance_windows (tenant_id) WHERE ended_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_maintenance_windows_tenant
    ON maintenance_windows (tenant_id, started_at DESC);

-- Duration is derived, not stored: `executions` records `started_at` and
-- `ended_at`, and a column that can drift from them is a liability. This view
-- is the single definition every health and percentile query reads, so the
-- numbers cannot disagree between screens.
CREATE VIEW execution_durations AS
SELECT
    id,
    tenant_id,
    job_id,
    status,
    attempt_count,
    -- Whole seconds, measured from start to end. NULL until the execution ends.
    CASE
        WHEN started_at IS NOT NULL AND ended_at IS NOT NULL
            THEN FLOOR(EXTRACT(EPOCH FROM (ended_at - started_at)))::int
        ELSE NULL
    END AS duration_seconds,
    -- Time spent waiting for a worker, which the spec reports separately from
    -- execution time (UI.md section 17).
    CASE
        WHEN enqueued_at IS NOT NULL AND started_at IS NOT NULL
            THEN FLOOR(EXTRACT(EPOCH FROM (started_at - enqueued_at)))::int
        ELSE NULL
    END AS queue_wait_seconds,
    created_at,
    started_at,
    ended_at
FROM executions;