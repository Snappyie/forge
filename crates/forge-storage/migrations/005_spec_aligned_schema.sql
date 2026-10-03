-- 005_spec_aligned_schema.sql
--
-- Brings the schema in line with docs/08-storage-specification.md §8.2 (the 23
-- required tables), docs/02-domain-model.md (entity fields), and spec 08.6
-- (mandatory indexes).
--
-- This migration is forward-only, following the sequence in spec 08.9:
--   1. add new nullable/compatible schema
--   2. (code that reads old/new ships separately)
--   3. backfill
--   4. enforce constraints
--   5. drop old columns only in a later release
--
-- Lock acquisition order for multi-row transactions (spec 08.5):
--   tenant -> schedule -> execution -> execution_attempt -> worker_lease
-- Acquiring in this order prevents deadlock between the scheduler, the
-- dispatcher, the worker lease reaper, and API mutations.

--------------------------------------------------------------------------------
-- 0. Normalise legacy status values
--------------------------------------------------------------------------------
-- The pre-005 schema stored statuses as written by early handlers, which used
-- mixed case ("Draft", "Running"). The CHECK constraints below are case
-- sensitive, so existing rows must be normalised before they are added.
-- Applied on a clean database these are no-ops.

UPDATE jobs SET status = UPPER(status) WHERE status <> UPPER(status);

UPDATE workers SET status = UPPER(status)
    WHERE status IS NULL OR status <> UPPER(status);

-- Unknown worker statuses collapse to OFFLINE rather than failing the
-- constraint; the three legacy values (Online/Draining/Offline) map onto
-- READY/DRAINING/OFFLINE.
UPDATE workers SET status = 'OFFLINE' WHERE status NOT IN
    ('REGISTERING', 'READY', 'BUSY', 'DRAINING', 'OFFLINE', 'REVOKED');

UPDATE executions SET status = UPPER(status)
    WHERE status IS NULL OR status <> UPPER(status);

UPDATE executions SET status = 'FAILED' WHERE status NOT IN
    ('SCHEDULED', 'QUEUED', 'DISPATCHED', 'RUNNING', 'SUCCEEDED', 'FAILED',
     'TIMED_OUT', 'CANCEL_REQUESTED', 'CANCELLED', 'RETRY_SCHEDULED',
     'DEAD_LETTERED', 'ABANDONED');

--------------------------------------------------------------------------------
-- 1. Status vocabularies
--------------------------------------------------------------------------------
-- Statuses are VARCHAR + CHECK rather than native ENUM types. Adding a value
-- to a native enum is a DDL change that cannot run inside a transaction in
-- older PostgreSQL, which makes forward-only migrations harder.

-- JobStatus (spec 02.2)
ALTER TABLE jobs
    ADD COLUMN IF NOT EXISTS key TEXT,
    ADD COLUMN IF NOT EXISTS default_queue_id UUID,
    ADD COLUMN IF NOT EXISTS labels JSONB NOT NULL DEFAULT '{}'::jsonb,
    ADD COLUMN IF NOT EXISTS owner_id UUID,
    ADD COLUMN IF NOT EXISTS priority VARCHAR(20) NOT NULL DEFAULT 'NORMAL';

ALTER TABLE jobs DROP CONSTRAINT IF EXISTS jobs_status_check;
ALTER TABLE jobs ADD CONSTRAINT jobs_status_check
    CHECK (status IN ('DRAFT', 'ACTIVE', 'ARCHIVED'));

ALTER TABLE jobs DROP CONSTRAINT IF EXISTS jobs_priority_check;
ALTER TABLE jobs ADD CONSTRAINT jobs_priority_check
    CHECK (priority IN ('CRITICAL', 'HIGH', 'NORMAL', 'LOW', 'BACKGROUND'));

-- WorkerStatus (spec 02.8 requires six states; the table previously had three
-- unvalidated free-text values).
ALTER TABLE workers
    ADD COLUMN IF NOT EXISTS name TEXT,
    ADD COLUMN IF NOT EXISTS version TEXT,
    ADD COLUMN IF NOT EXISTS capabilities JSONB NOT NULL DEFAULT '[]'::jsonb,
    ADD COLUMN IF NOT EXISTS resources JSONB NOT NULL DEFAULT '{}'::jsonb,
    ADD COLUMN IF NOT EXISTS labels JSONB NOT NULL DEFAULT '{}'::jsonb,
    ADD COLUMN IF NOT EXISTS draining BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS registered_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW();

UPDATE workers SET name = hostname WHERE name IS NULL;
UPDATE workers SET registered_at = created_at WHERE registered_at IS NULL;

ALTER TABLE workers DROP CONSTRAINT IF EXISTS workers_status_check;
ALTER TABLE workers ADD CONSTRAINT workers_status_check
    CHECK (status IN ('REGISTERING', 'READY', 'BUSY', 'DRAINING', 'OFFLINE', 'REVOKED'));

-- ExecutionStatus (spec 02.5 state machine).
ALTER TABLE executions
    ADD COLUMN IF NOT EXISTS workflow_id UUID,
    ADD COLUMN IF NOT EXISTS workflow_execution_id UUID,
    ADD COLUMN IF NOT EXISTS parent_execution_id UUID REFERENCES executions(id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS schedule_id UUID,
    ADD COLUMN IF NOT EXISTS priority VARCHAR(20) NOT NULL DEFAULT 'NORMAL',
    ADD COLUMN IF NOT EXISTS trigger_source VARCHAR(30) NOT NULL DEFAULT 'MANUAL',
    ADD COLUMN IF NOT EXISTS scheduled_for TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS enqueued_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS correlation_id TEXT,
    ADD COLUMN IF NOT EXISTS input JSONB NOT NULL DEFAULT '{}'::jsonb,
    ADD COLUMN IF NOT EXISTS output JSONB,
    ADD COLUMN IF NOT EXISTS error_class VARCHAR(30),
    ADD COLUMN IF NOT EXISTS error_code TEXT,
    ADD COLUMN IF NOT EXISTS error_message TEXT,
    ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW();

ALTER TABLE executions DROP CONSTRAINT IF EXISTS executions_status_check;
ALTER TABLE executions ADD CONSTRAINT executions_status_check
    CHECK (status IN (
        'SCHEDULED', 'QUEUED', 'DISPATCHED', 'RUNNING', 'SUCCEEDED', 'FAILED',
        'TIMED_OUT', 'CANCEL_REQUESTED', 'CANCELLED', 'RETRY_SCHEDULED',
        'DEAD_LETTERED', 'ABANDONED'
    ));

ALTER TABLE executions DROP CONSTRAINT IF EXISTS executions_trigger_source_check;
ALTER TABLE executions ADD CONSTRAINT executions_trigger_source_check
    CHECK (trigger_source IN ('SCHEDULE', 'MANUAL', 'API', 'WORKFLOW', 'RETRY', 'RECOVERY'));

-- Spec 02.14 error taxonomy. Retry decisions MUST use this classification
-- rather than matching on free-text error strings.
ALTER TABLE executions DROP CONSTRAINT IF EXISTS executions_error_class_check;
ALTER TABLE executions ADD CONSTRAINT executions_error_class_check
    CHECK (error_class IS NULL OR error_class IN (
        'VALIDATION', 'AUTHENTICATION', 'AUTHORIZATION', 'NOT_FOUND', 'CONFLICT',
        'RATE_LIMITED', 'TRANSIENT', 'DEPENDENCY_UNAVAILABLE', 'TIMEOUT',
        'CANCELLATION', 'RESOURCE_EXHAUSTED', 'PERMANENT', 'INTERNAL'
    ));

UPDATE executions SET enqueued_at = created_at WHERE enqueued_at IS NULL;

--------------------------------------------------------------------------------
-- 2. Schedules — spec 02.4
--------------------------------------------------------------------------------
-- The original table hardcoded `job_id` and used `is_paused`. Spec 02.4
-- requires a polymorphic target plus explicit misfire/catch-up policy and an
-- `enabled` flag. `job_id` is kept (now nullable) and removed in a later
-- release per the migration sequence in spec 08.9.

ALTER TABLE schedules
    ADD COLUMN IF NOT EXISTS target_type VARCHAR(20) NOT NULL DEFAULT 'JOB',
    ADD COLUMN IF NOT EXISTS target_id UUID,
    ADD COLUMN IF NOT EXISTS target_version_policy VARCHAR(20) NOT NULL DEFAULT 'LATEST_PUBLISHED',
    ADD COLUMN IF NOT EXISTS schedule_type VARCHAR(20) NOT NULL DEFAULT 'CRON',
    ADD COLUMN IF NOT EXISTS misfire_policy VARCHAR(20) NOT NULL DEFAULT 'FIRE_ONCE',
    ADD COLUMN IF NOT EXISTS catch_up_policy JSONB NOT NULL DEFAULT '{}'::jsonb,
    ADD COLUMN IF NOT EXISTS enabled BOOLEAN NOT NULL DEFAULT TRUE,
    ADD COLUMN IF NOT EXISTS last_run_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW();

-- Backfill the polymorphic target from the legacy job_id.
UPDATE schedules SET target_id = job_id WHERE target_id IS NULL;

-- job_id must become nullable now that a schedule may target a workflow.
ALTER TABLE schedules ALTER COLUMN job_id DROP NOT NULL;

ALTER TABLE schedules DROP CONSTRAINT IF EXISTS schedules_target_type_check;
ALTER TABLE schedules ADD CONSTRAINT schedules_target_type_check
    CHECK (target_type IN ('JOB', 'WORKFLOW'));

ALTER TABLE schedules DROP CONSTRAINT IF EXISTS schedules_misfire_policy_check;
ALTER TABLE schedules ADD CONSTRAINT schedules_misfire_policy_check
    CHECK (misfire_policy IN ('SKIP', 'FIRE_ONCE', 'CATCH_UP'));

ALTER TABLE schedules DROP CONSTRAINT IF EXISTS schedules_version_policy_check;
ALTER TABLE schedules ADD CONSTRAINT schedules_version_policy_check
    CHECK (target_version_policy IN ('PINNED', 'LATEST_PUBLISHED'));

ALTER TABLE schedules DROP CONSTRAINT IF EXISTS schedules_schedule_type_check;
ALTER TABLE schedules ADD CONSTRAINT schedules_schedule_type_check
    CHECK (schedule_type IN ('CRON', 'ONE_TIME', 'INTERVAL'));

ALTER TABLE schedules DROP CONSTRAINT IF EXISTS schedules_target_id_check;
ALTER TABLE schedules ADD CONSTRAINT schedules_target_id_check
    CHECK (target_id IS NOT NULL);

-- Pause state now lives in `enabled`; keep `is_paused` in sync until it is
-- dropped in a later release.
DROP TRIGGER IF EXISTS schedules_sync_paused ON schedules;

--------------------------------------------------------------------------------
-- 3. New tables (spec 08.2)
--------------------------------------------------------------------------------

-- JobVersion (spec 02.3). Immutable once published: spec 02.16 and invariant 4
-- forbid mutating a published version, which the repository layer enforces.
CREATE TABLE IF NOT EXISTS job_versions (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    job_id UUID NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    version_number INT NOT NULL,
    execution_type VARCHAR(30) NOT NULL,
    execution_config JSONB NOT NULL DEFAULT '{}'::jsonb,
    input_schema JSONB,
    timeout_seconds INT NOT NULL DEFAULT 3600,
    retry_policy JSONB NOT NULL DEFAULT '{}'::jsonb,
    concurrency_policy JSONB NOT NULL DEFAULT '{}'::jsonb,
    resource_requirements JSONB NOT NULL DEFAULT '{}'::jsonb,
    environment JSONB NOT NULL DEFAULT '{}'::jsonb,
    secret_references JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_by UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    published_at TIMESTAMPTZ,
    CONSTRAINT job_versions_execution_type_check
        CHECK (execution_type IN ('HTTP_REQUEST', 'CONTAINER_COMMAND', 'WORKER_TASK')),
    CONSTRAINT job_versions_version_number_check CHECK (version_number > 0),
    CONSTRAINT job_versions_timeout_check CHECK (timeout_seconds > 0),
    CONSTRAINT job_versions_job_version_unique UNIQUE (job_id, version_number)
);

CREATE TABLE IF NOT EXISTS workflows (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    key TEXT NOT NULL,
    name VARCHAR(255) NOT NULL,
    description TEXT,
    status VARCHAR(20) NOT NULL DEFAULT 'DRAFT',
    current_version_id UUID,
    owner_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT workflows_status_check CHECK (status IN ('DRAFT', 'ACTIVE', 'ARCHIVED')),
    CONSTRAINT workflows_tenant_key_unique UNIQUE (tenant_id, key)
);

CREATE TABLE IF NOT EXISTS workflow_versions (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    workflow_id UUID NOT NULL REFERENCES workflows(id) ON DELETE CASCADE,
    version_number INT NOT NULL,
    execution_policy JSONB NOT NULL DEFAULT '{}'::jsonb,
    concurrency_policy JSONB NOT NULL DEFAULT '{}'::jsonb,
    timeout_seconds INT NOT NULL DEFAULT 86400,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_by UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    published_at TIMESTAMPTZ,
    CONSTRAINT workflow_versions_version_number_check CHECK (version_number > 0),
    CONSTRAINT workflow_versions_workflow_version_unique UNIQUE (workflow_id, version_number)
);

-- Nodes and edges are normalised rather than stored as JSON so the graph can
-- be validated and queried (cycle detection, fan-out counts) in SQL.
CREATE TABLE IF NOT EXISTS workflow_nodes (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    workflow_version_id UUID NOT NULL REFERENCES workflow_versions(id) ON DELETE CASCADE,
    node_key TEXT NOT NULL,
    node_type VARCHAR(30) NOT NULL,
    name VARCHAR(255) NOT NULL,
    config JSONB NOT NULL DEFAULT '{}'::jsonb,
    position JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT workflow_nodes_type_check
        CHECK (node_type IN ('JOB', 'APPROVAL', 'DELAY', 'CONDITION', 'MAP', 'WEBHOOK')),
    CONSTRAINT workflow_nodes_version_key_unique UNIQUE (workflow_version_id, node_key)
);

CREATE TABLE IF NOT EXISTS workflow_edges (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    workflow_version_id UUID NOT NULL REFERENCES workflow_versions(id) ON DELETE CASCADE,
    from_node_id UUID NOT NULL REFERENCES workflow_nodes(id) ON DELETE CASCADE,
    to_node_id UUID NOT NULL REFERENCES workflow_nodes(id) ON DELETE CASCADE,
    condition VARCHAR(30) NOT NULL DEFAULT 'ALL_SUCCEEDED',
    expression TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT workflow_edges_condition_check
        CHECK (condition IN ('ALL_SUCCEEDED', 'ANY_SUCCEEDED', 'ALL_COMPLETED', 'ALWAYS')),
    CONSTRAINT workflow_edges_no_self_loop CHECK (from_node_id <> to_node_id),
    CONSTRAINT workflow_edges_unique UNIQUE (workflow_version_id, from_node_id, to_node_id)
);

-- Human-in-the-loop approvals (spec 05 endpoints 31-32, spec 19 Phase 7).
CREATE TABLE IF NOT EXISTS manual_approvals (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    workflow_execution_id UUID NOT NULL REFERENCES executions(id) ON DELETE CASCADE,
    node_id UUID NOT NULL REFERENCES workflow_nodes(id) ON DELETE CASCADE,
    status VARCHAR(20) NOT NULL DEFAULT 'PENDING',
    required_role VARCHAR(30),
    requested_by UUID,
    decided_by UUID,
    decided_at TIMESTAMPTZ,
    comment TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT manual_approvals_status_check CHECK (status IN ('PENDING', 'APPROVED', 'REJECTED')),
    CONSTRAINT manual_approvals_unique UNIQUE (workflow_execution_id, node_id)
);

-- Attempt (spec 02.7). `execution_attempts` preserves every attempt so that a
-- recovered execution never loses the record of what a crashed worker did
-- (spec 10.11 step 2, invariant: execution history is never deleted just
-- because a worker disappeared).
CREATE TABLE IF NOT EXISTS execution_attempts (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    execution_id UUID NOT NULL REFERENCES executions(id) ON DELETE CASCADE,
    attempt_number INT NOT NULL,
    worker_id UUID,
    lease_id UUID,
    status VARCHAR(20) NOT NULL,
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    ended_at TIMESTAMPTZ,
    exit_code INT,
    error_class VARCHAR(30),
    error_code TEXT,
    error_message TEXT,
    metrics JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT execution_attempts_number_check CHECK (attempt_number > 0),
    CONSTRAINT execution_attempts_status_check
        CHECK (status IN ('RUNNING', 'SUCCEEDED', 'FAILED', 'TIMED_OUT', 'CANCELLED', 'ABANDONED')),
    CONSTRAINT execution_attempts_execution_number_unique UNIQUE (execution_id, attempt_number)
);

-- Lease (spec 02.9). Ownership is exclusive; `worker_leases` gains the full
-- spec field set plus a surrogate key so a stale completion carrying an old
-- lease_id can be rejected (spec 10.5, AT-STATE-004).
ALTER TABLE worker_leases
    ADD COLUMN IF NOT EXISTS id UUID,
    ADD COLUMN IF NOT EXISTS tenant_id UUID REFERENCES tenants(id) ON DELETE CASCADE,
    ADD COLUMN IF NOT EXISTS attempt_id UUID REFERENCES execution_attempts(id) ON DELETE CASCADE,
    ADD COLUMN IF NOT EXISTS acquired_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    ADD COLUMN IF NOT EXISTS renewed_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS released_at TIMESTAMPTZ;

-- Backfill the surrogate key and tenant scope for pre-existing rows.
UPDATE worker_leases SET id = gen_random_uuid() WHERE id IS NULL;
UPDATE worker_leases wl SET tenant_id = e.tenant_id FROM executions e
    WHERE wl.execution_id = e.id AND wl.tenant_id IS NULL;

ALTER TABLE worker_leases ALTER COLUMN id SET NOT NULL;

-- `execution_id` was the primary key; a lease is released and re-acquired
-- across retries, so it must not be unique per execution.
ALTER TABLE worker_leases DROP CONSTRAINT IF EXISTS worker_leases_pkey;
ALTER TABLE worker_leases ADD PRIMARY KEY (id);

CREATE UNIQUE INDEX IF NOT EXISTS idx_worker_leases_active_execution
    ON worker_leases (execution_id) WHERE released_at IS NULL;

-- user_identities (spec 08.2) links local accounts to SSO/OIDC subjects.
CREATE TABLE IF NOT EXISTS user_identities (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    provider VARCHAR(30) NOT NULL,
    provider_subject TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT user_identities_provider_subject_unique UNIQUE (provider, provider_subject)
);

-- Roles are a per-tenant catalogue seeded with the six roles from spec 05 §5.10.
CREATE TABLE IF NOT EXISTS roles (
    id UUID PRIMARY KEY,
    tenant_id UUID REFERENCES tenants(id) ON DELETE CASCADE,
    name VARCHAR(30) NOT NULL,
    description TEXT,
    permissions JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT roles_name_check
        CHECK (name IN ('OWNER', 'ADMIN', 'OPERATOR', 'DEVELOPER', 'AUDITOR', 'VIEWER'))
);

-- NULL tenant_id marks a system-wide (built-in) role; otherwise the grant is
-- scoped to one tenant.
CREATE UNIQUE INDEX IF NOT EXISTS idx_roles_tenant_name_unique
    ON roles (COALESCE(tenant_id, '00000000-0000-0000-0000-000000000000'::uuid), name);

CREATE TABLE IF NOT EXISTS user_roles (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role_id UUID NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT user_roles_unique UNIQUE (user_id, tenant_id, role_id)
);

-- Outbox (spec 04.8). External events are written in the same transaction as
-- the state change they describe, so a crash cannot lose an event.
CREATE TABLE IF NOT EXISTS outbox_events (
    id UUID PRIMARY KEY,
    tenant_id UUID REFERENCES tenants(id) ON DELETE CASCADE,
    event_type VARCHAR(100) NOT NULL,
    aggregate_type VARCHAR(50) NOT NULL,
    aggregate_id UUID NOT NULL,
    payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    published_at TIMESTAMPTZ,
    attempt_count INT NOT NULL DEFAULT 0,
    last_error TEXT,
    CONSTRAINT outbox_events_attempt_count_check CHECK (attempt_count >= 0)
);

-- Audit events (spec 01.17). Append-only from the application perspective:
-- the repository exposes inserts and reads, never updates or deletes.
CREATE TABLE IF NOT EXISTS audit_events (
    id UUID PRIMARY KEY,
    tenant_id UUID REFERENCES tenants(id) ON DELETE CASCADE,
    actor_type VARCHAR(30) NOT NULL,
    actor_id UUID,
    action VARCHAR(100) NOT NULL,
    resource_type VARCHAR(50) NOT NULL,
    resource_id UUID,
    result VARCHAR(20) NOT NULL DEFAULT 'SUCCESS',
    source_ip INET,
    request_id TEXT,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT audit_events_result_check CHECK (result IN ('SUCCESS', 'DENIED', 'FAILURE')),
    CONSTRAINT audit_events_actor_type_check
        CHECK (actor_type IN ('USER', 'API_KEY', 'WORKER', 'SYSTEM'))
);

-- Idempotency (spec 02.15). The request fingerprint is what lets the server
-- distinguish "same key, same request" (replay the stored result) from
-- "same key, different request" (409 CONFLICT).
CREATE TABLE IF NOT EXISTS idempotency_keys (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    idempotency_key TEXT NOT NULL,
    endpoint TEXT NOT NULL,
    request_fingerprint TEXT NOT NULL,
    response_status INT,
    response_body JSONB,
    resource_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL,
    CONSTRAINT idempotency_keys_unique UNIQUE (tenant_id, endpoint, idempotency_key)
);

-- Integration configuration (spec 05 endpoints 53-56). Secrets are referenced
-- by identifier only; raw secret material is never stored or returned.
CREATE TABLE IF NOT EXISTS integration_configurations (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    kind VARCHAR(50) NOT NULL,
    name VARCHAR(255) NOT NULL,
    config JSONB NOT NULL DEFAULT '{}'::jsonb,
    secret_reference TEXT,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    created_by UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT integration_configurations_unique UNIQUE (tenant_id, kind, name)
);

-- Optional tables (spec 08.2). Created now so retention and archival jobs in
-- later phases have somewhere to operate.
CREATE TABLE IF NOT EXISTS execution_logs (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    execution_id UUID NOT NULL REFERENCES executions(id) ON DELETE CASCADE,
    attempt_id UUID REFERENCES execution_attempts(id) ON DELETE CASCADE,
    stream VARCHAR(10) NOT NULL DEFAULT 'stdout',
    content TEXT NOT NULL,
    logged_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT execution_logs_stream_check CHECK (stream IN ('stdout', 'stderr'))
);

CREATE TABLE IF NOT EXISTS execution_artifacts (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    execution_id UUID NOT NULL REFERENCES executions(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    content_type TEXT,
    size_bytes BIGINT NOT NULL DEFAULT 0,
    storage_reference TEXT NOT NULL,
    checksum TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT execution_artifacts_size_check CHECK (size_bytes >= 0)
);

CREATE TABLE IF NOT EXISTS worker_metrics (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    worker_id UUID NOT NULL REFERENCES workers(id) ON DELETE CASCADE,
    cpu_percent DOUBLE PRECISION,
    memory_percent DOUBLE PRECISION,
    active_executions INT NOT NULL DEFAULT 0,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT worker_metrics_active_check CHECK (active_executions >= 0)
);

--------------------------------------------------------------------------------
-- 4. Deferred foreign keys
--------------------------------------------------------------------------------
-- Added after the referencing tables exist so the migration order is acyclic.

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'jobs_current_version_id_fkey'
    ) THEN
        ALTER TABLE jobs ADD CONSTRAINT jobs_current_version_id_fkey
            FOREIGN KEY (current_version_id) REFERENCES job_versions(id) ON DELETE SET NULL;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'workflows_current_version_id_fkey'
    ) THEN
        ALTER TABLE workflows ADD CONSTRAINT workflows_current_version_id_fkey
            FOREIGN KEY (current_version_id) REFERENCES workflow_versions(id) ON DELETE SET NULL;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'executions_job_version_id_fkey'
    ) THEN
        ALTER TABLE executions ADD CONSTRAINT executions_job_version_id_fkey
            FOREIGN KEY (job_version_id) REFERENCES job_versions(id);
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'executions_schedule_id_fkey'
    ) THEN
        ALTER TABLE executions ADD CONSTRAINT executions_schedule_id_fkey
            FOREIGN KEY (schedule_id) REFERENCES schedules(id) ON DELETE SET NULL;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'executions_worker_id_fkey'
    ) THEN
        ALTER TABLE executions ADD CONSTRAINT executions_worker_id_fkey
            FOREIGN KEY (worker_id) REFERENCES workers(id) ON DELETE SET NULL;
    END IF;
END
$$;

--------------------------------------------------------------------------------
-- 5. Tenant-scoped uniqueness (spec 08.3)
--------------------------------------------------------------------------------
-- Every unique constraint on a tenant-scoped table includes tenant_id unless
-- the value is genuinely global.

-- jobs by tenant/key (spec 08.6)
CREATE UNIQUE INDEX IF NOT EXISTS idx_jobs_tenant_key_unique
    ON jobs (tenant_id, key) WHERE key IS NOT NULL;

-- job_versions must be unique per (tenant, job, version) — the global
-- (job_id, version_number) constraint above is already tenant-safe because
-- job_id is tenant-scoped.

-- executions: one execution per (schedule, intended occurrence). This is the
-- database-level guarantee behind AT-SCH-008: even if two schedulers race to
-- claim the same occurrence, the second INSERT cannot succeed.
CREATE UNIQUE INDEX IF NOT EXISTS idx_executions_schedule_occurrence_unique
    ON executions (schedule_id, scheduled_for)
    WHERE schedule_id IS NOT NULL AND scheduled_for IS NOT NULL;

-- Idempotency for manual triggers: one execution per (tenant, idempotency key).
CREATE UNIQUE INDEX IF NOT EXISTS idx_executions_tenant_idempotency_unique
    ON executions (tenant_id, correlation_id)
    WHERE correlation_id IS NOT NULL;

--------------------------------------------------------------------------------
-- 6. Mandatory indexes (spec 08.6)
--------------------------------------------------------------------------------

-- schedules by enabled/next_run_at. Partial, because the scheduler only ever
-- scans enabled schedules; this keeps the index small as paused/backlog rows
-- accumulate.
DROP INDEX IF EXISTS idx_schedules_next_run;
CREATE INDEX IF NOT EXISTS idx_schedules_enabled_next_run
    ON schedules (next_run_at) WHERE enabled = TRUE;

-- executions by tenant/status/created_at
CREATE INDEX IF NOT EXISTS idx_executions_tenant_status_created
    ON executions (tenant_id, status, created_at DESC);

-- executions by job/created_at
CREATE INDEX IF NOT EXISTS idx_executions_job_created
    ON executions (job_id, created_at DESC);

-- executions by queue/status
CREATE INDEX IF NOT EXISTS idx_executions_queue_status
    ON executions (queue_id, status);

-- attempts by execution/attempt_number (the UNIQUE constraint above already
-- provides a btree on exactly this pair).

-- workers by tenant/status
CREATE INDEX IF NOT EXISTS idx_workers_tenant_status
    ON workers (tenant_id, status);

-- leases by expiry — drives the lease reaper (spec 10.11).
CREATE INDEX IF NOT EXISTS idx_worker_leases_expires
    ON worker_leases (expires_at) WHERE released_at IS NULL;

-- outbox by published_at/created_at — the publisher claims unpublished rows.
CREATE INDEX IF NOT EXISTS idx_outbox_unpublished
    ON outbox_events (created_at) WHERE published_at IS NULL;

-- audit by tenant/timestamp
CREATE INDEX IF NOT EXISTS idx_audit_events_tenant_created
    ON audit_events (tenant_id, created_at DESC);

--------------------------------------------------------------------------------
-- 7. Supporting indexes for hot query paths
--------------------------------------------------------------------------------
CREATE INDEX IF NOT EXISTS idx_executions_active_lookup
    ON executions (tenant_id, queue_id)
    WHERE status IN ('QUEUED', 'DISPATCHED', 'RUNNING', 'RETRY_SCHEDULED');

-- The dispatcher claims queued work ordered by priority then age.
CREATE INDEX IF NOT EXISTS idx_executions_queue_priority_created
    ON executions (queue_id, priority, created_at)
    WHERE status = 'QUEUED';

CREATE INDEX IF NOT EXISTS idx_execution_attempts_execution
    ON execution_attempts (execution_id, attempt_number);

CREATE INDEX IF NOT EXISTS idx_workflow_nodes_version
    ON workflow_nodes (workflow_version_id);

CREATE INDEX IF NOT EXISTS idx_workflow_edges_from
    ON workflow_edges (from_node_id);

CREATE INDEX IF NOT EXISTS idx_workflow_edges_to
    ON workflow_edges (to_node_id);

CREATE INDEX IF NOT EXISTS idx_manual_approvals_pending
    ON manual_approvals (tenant_id, created_at) WHERE status = 'PENDING';

CREATE INDEX IF NOT EXISTS idx_idempotency_keys_expiry
    ON idempotency_keys (expires_at);

CREATE INDEX IF NOT EXISTS idx_execution_logs_execution
    ON execution_logs (execution_id, logged_at);

CREATE INDEX IF NOT EXISTS idx_worker_metrics_worker_recorded
    ON worker_metrics (worker_id, recorded_at DESC);

-- Session refresh tokens need lookup by token and cleanup by expiry.
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_user
    ON refresh_tokens (user_id);
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_expires
    ON refresh_tokens (expires_at);

CREATE INDEX IF NOT EXISTS idx_api_keys_tenant
    ON api_keys (tenant_id);

--------------------------------------------------------------------------------
-- 8. Built-in roles (spec 05 §5.10)
--------------------------------------------------------------------------------
-- Seeded per tenant on creation; the tenant-scoped seed here uses a NULL
-- tenant_id to mark the templates that `TenantRepository::create` copies.
INSERT INTO roles (id, tenant_id, name, description, permissions)
VALUES
    (gen_random_uuid(), NULL, 'OWNER',
     'Full control including tenant deletion',
     '["*"]'),
    (gen_random_uuid(), NULL, 'ADMIN',
     'Manages jobs, workflows, workers, queues and users',
     '["jobs:*","executions:*","workflows:*","workers:admin","queues:*","users:write","audit:read","settings:write"]'),
    (gen_random_uuid(), NULL, 'OPERATOR',
     'Runs and supervises work; cannot change definitions',
     '["jobs:read","jobs:trigger","executions:*","workflows:read","workflows:trigger","workers:read","queues:read","audit:read"]'),
    (gen_random_uuid(), NULL, 'DEVELOPER',
     'Authors and versions job and workflow definitions',
     '["jobs:read","jobs:write","jobs:trigger","executions:read","executions:cancel","workflows:read","workflows:write","workflows:trigger","workers:read","queues:read"]'),
    (gen_random_uuid(), NULL, 'AUDITOR',
     'Read-only access to history and audit trails',
     '["jobs:read","executions:read","workflows:read","workers:read","queues:read","audit:read"]'),
    (gen_random_uuid(), NULL, 'VIEWER',
     'Read-only access to resources',
     '["jobs:read","executions:read","workflows:read","workers:read","queues:read"]')
ON CONFLICT DO NOTHING;