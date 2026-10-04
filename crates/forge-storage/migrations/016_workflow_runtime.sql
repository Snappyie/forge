-- 016_workflow_runtime.sql
--
-- Makes a workflow run a first-class execution and records per-node progress.
--
-- Before this migration a workflow run could not even be created: a run has no
-- job, but `executions.job_id` and `executions.job_version_id` were NOT NULL
-- since 002, and `POST /workflows/{id}/trigger` omitted them. The DAG engine in
-- `forge-executor` was also unreachable, because nothing persisted the node
-- states it needs to resume from. Spec 02.11 requires DAG execution, so both
-- halves are fixed here.

-- A workflow run is an execution that names a workflow instead of a job.
ALTER TABLE executions ALTER COLUMN job_id DROP NOT NULL;
ALTER TABLE executions ALTER COLUMN job_version_id DROP NOT NULL;

-- The definition the run was started from. Pinned at trigger time so publishing
-- a new workflow version cannot change a run that is already in flight
-- (spec 01.22 invariant 6).
ALTER TABLE executions
    ADD COLUMN IF NOT EXISTS workflow_version_id UUID REFERENCES workflow_versions(id);

-- The engine's accumulated context: node outputs, condition results and map
-- items. Persisted so a run resumes correctly after a restart and after the
-- runtime process moves to another server.
ALTER TABLE executions
    ADD COLUMN IF NOT EXISTS workflow_context JSONB NOT NULL DEFAULT '{}'::jsonb;

-- Every execution is either job work or a workflow run, never neither.
ALTER TABLE executions DROP CONSTRAINT IF EXISTS executions_subject_check;
ALTER TABLE executions ADD CONSTRAINT executions_subject_check
    CHECK (job_id IS NOT NULL OR workflow_id IS NOT NULL);

-- Node progress for a run. One row per workflow node, keyed by the node's
-- stable `node_key` so the engine can address it across restarts.
CREATE TABLE IF NOT EXISTS workflow_node_states (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    workflow_execution_id UUID NOT NULL REFERENCES executions(id) ON DELETE CASCADE,
    workflow_version_id UUID NOT NULL REFERENCES workflow_versions(id) ON DELETE CASCADE,
    node_key TEXT NOT NULL,
    -- Nullable so a node whose row was removed from a draft version can still
    -- be recorded; the approval table needs the real id when it exists.
    node_id UUID REFERENCES workflow_nodes(id) ON DELETE SET NULL,
    node_type VARCHAR(30) NOT NULL,
    state VARCHAR(20) NOT NULL DEFAULT 'PENDING',
    suspension_reason VARCHAR(30),
    resume_at TIMESTAMPTZ,
    -- The execution a JOB node dispatched, or the parent of a MAP node's
    -- fan-out children.
    child_execution_id UUID REFERENCES executions(id) ON DELETE SET NULL,
    required_role VARCHAR(30),
    output JSONB,
    failure_reason TEXT,
    attempt_count INT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT workflow_node_states_state_check
        CHECK (state IN ('PENDING', 'RUNNING', 'SUSPENDED', 'COMPLETED', 'FAILED', 'SKIPPED')),
    CONSTRAINT workflow_node_states_unique UNIQUE (workflow_execution_id, node_key)
);

CREATE INDEX IF NOT EXISTS idx_workflow_node_states_run
    ON workflow_node_states (workflow_execution_id);

-- The runtime's drive loop scans for runs that are still going. Indexing the
-- partial set keeps that cheap as history grows.
CREATE INDEX IF NOT EXISTS idx_executions_workflow_runs
    ON executions (updated_at)
    WHERE workflow_id IS NOT NULL AND status = 'RUNNING';

-- A workflow run is driven by the server, not claimed by a worker, so the
-- dispatch query must never pick one up.
CREATE INDEX IF NOT EXISTS idx_executions_job_dispatchable
    ON executions (tenant_id, created_at)
    WHERE job_id IS NOT NULL AND status = 'QUEUED';

-- Conditional branching (spec 01.8: if/else on previous outputs, and Airflow's
-- BranchPythonOperator equivalent) is expressed as two out-edges from a
-- CONDITION node labelled `true` and `false`. The engine has always understood
-- those labels — `branch_label_matches` reads them — but 005's CHECK constraint
-- only permitted the four dependency conditions, so a branching graph could not
-- even be saved. The comparison is case-insensitive so `true`, `TRUE` and `True`
-- all round-trip without the caller having to know the storage spelling.
ALTER TABLE workflow_edges DROP CONSTRAINT IF EXISTS workflow_edges_condition_check;
ALTER TABLE workflow_edges ADD CONSTRAINT workflow_edges_condition_check
    CHECK (upper(condition) IN (
        'ALL_SUCCEEDED', 'ANY_SUCCEEDED', 'ALL_COMPLETED', 'ALWAYS', 'TRUE', 'FALSE'
    ));
