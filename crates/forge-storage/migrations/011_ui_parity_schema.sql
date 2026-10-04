-- 011_ui_parity_schema.sql
--
-- Backs the UI.md sections that had no data source: execution metrics (§17),
-- webhooks (§75), integrations (§74), saved views (§48), job dependencies
-- (§70), undo history (§38), and system health tracking (§73).

-- Per-execution resource samples. The executor writes these; the console reads
-- them. Absent samples stay absent rather than being interpolated to zero.
CREATE TABLE IF NOT EXISTS execution_metrics (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    execution_id UUID NOT NULL REFERENCES executions(id) ON DELETE CASCADE,
    -- Milliseconds since the execution started, so the series plots on one axis.
    offset_ms INT NOT NULL,
    cpu_percent DOUBLE PRECISION,
    memory_bytes BIGINT,
    network_rx_bytes BIGINT,
    network_tx_bytes BIGINT,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Sampling is per execution, so re-sampling at the same offset is a correction
-- rather than a duplicate.
CREATE UNIQUE INDEX IF NOT EXISTS idx_execution_metrics_sample
    ON execution_metrics (execution_id, offset_ms);

CREATE INDEX IF NOT EXISTS idx_execution_metrics_exec
    ON execution_metrics (tenant_id, execution_id, offset_ms);

-- Named integrations with their connection state (UI.md §74).
CREATE TABLE IF NOT EXISTS integrations (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    -- `POSTGRESQL`, `KAFKA`, `REDIS`, `KUBERNETES`, `SLACK`, `TEAMS`, `EMAIL`,
    -- `PAGERDUTY`, `CLOUD_PROVIDER`, `SECRET_MANAGER`.
    kind VARCHAR(40) NOT NULL,
    name VARCHAR(255) NOT NULL,
    -- Non-secret configuration only. Secrets live in the secret manager.
    config JSONB NOT NULL DEFAULT '{}'::jsonb,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    last_success_at TIMESTAMPTZ,
    last_failure_at TIMESTAMPTZ,
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT integrations_kind_check CHECK (kind IN (
        'POSTGRESQL', 'KAFKA', 'REDIS', 'KUBERNETES', 'SLACK', 'TEAMS',
        'EMAIL', 'PAGERDUTY', 'CLOUD_PROVIDER', 'SECRET_MANAGER'
    )),
    CONSTRAINT integrations_tenant_name_unique UNIQUE (tenant_id, name)
);

CREATE INDEX IF NOT EXISTS idx_integrations_enabled
    ON integrations (tenant_id) WHERE enabled = TRUE;

-- Outbound webhooks (UI.md §75). Headers may carry a secret, so the value is
-- stored hashed alongside and only the presence is ever displayed.
CREATE TABLE IF NOT EXISTS webhooks (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    url TEXT NOT NULL,
    -- `NONE`, `BEARER`, `HMAC`, `BASIC`.
    auth_kind VARCHAR(20) NOT NULL DEFAULT 'NONE',
    -- SHA-256 of the secret; the plaintext is never stored or returned.
    secret_hash TEXT,
    -- Extra non-secret headers as a flat object.
    headers JSONB NOT NULL DEFAULT '{}'::jsonb,
    -- Which event types this webhook subscribes to; empty means all.
    events TEXT[] NOT NULL DEFAULT '{}',
    max_retries INT NOT NULL DEFAULT 5,
    timeout_seconds INT NOT NULL DEFAULT 10,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    last_success_at TIMESTAMPTZ,
    last_failure_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT webhooks_auth_check CHECK (auth_kind IN ('NONE','BEARER','HMAC','BASIC')),
    CONSTRAINT webhooks_retries_check CHECK (max_retries >= 0),
    CONSTRAINT webhooks_timeout_check CHECK (timeout_seconds > 0)
);

CREATE INDEX IF NOT EXISTS idx_webhooks_enabled
    ON webhooks (tenant_id) WHERE enabled = TRUE;

-- A webhook delivery attempt, so the console can show request and response
-- rather than only the final outcome (UI.md §75).
CREATE TABLE IF NOT EXISTS webhook_deliveries (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    webhook_id UUID NOT NULL REFERENCES webhooks(id) ON DELETE CASCADE,
    event_type VARCHAR(100) NOT NULL,
    -- `PENDING`, `DELIVERED`, `FAILED`.
    status VARCHAR(20) NOT NULL DEFAULT 'PENDING',
    attempt INT NOT NULL DEFAULT 1,
    request_body TEXT,
    response_status INT,
    response_body TEXT,
    error_message TEXT,
    -- Seconds to wait before the next retry, per the spec's retry setting.
    next_retry_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMPTZ,
    CONSTRAINT webhook_deliveries_status_check
        CHECK (status IN ('PENDING', 'DELIVERED', 'FAILED'))
);

CREATE INDEX IF NOT EXISTS idx_webhook_deliveries_webhook
    ON webhook_deliveries (webhook_id, created_at DESC);

-- Saved filter views (UI.md §48).
CREATE TABLE IF NOT EXISTS saved_views (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    -- `JOBS`, `EXECUTIONS`, `QUEUES`, `ALERTS`.
    resource VARCHAR(40) NOT NULL,
    name VARCHAR(255) NOT NULL,
    -- The filter state to restore, as the console serialises it.
    filters JSONB NOT NULL DEFAULT '{}'::jsonb,
    -- Personal views are per user; team views are visible to the tenant.
    shared BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT saved_views_resource_check
        CHECK (resource IN ('JOBS', 'EXECUTIONS', 'QUEUES', 'ALERTS')),
    CONSTRAINT saved_views_unique UNIQUE (tenant_id, user_id, resource, name)
);

CREATE INDEX IF NOT EXISTS idx_saved_views_user
    ON saved_views (tenant_id, user_id, resource);

-- Job-to-job dependencies (UI.md §70). The dependency map renders from this.
CREATE TABLE IF NOT EXISTS job_dependencies (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    upstream_job_id UUID NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    downstream_job_id UUID NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    -- What is being waited on.
    condition VARCHAR(40) NOT NULL DEFAULT 'SUCCEEDED',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT job_dependencies_condition_check
        CHECK (condition IN ('SUCCEEDED', 'COMPLETED')),
    -- A job cannot depend on itself, and each edge is declared once.
    CONSTRAINT job_dependencies_no_self CHECK (upstream_job_id <> downstream_job_id),
    CONSTRAINT job_dependencies_unique
        UNIQUE (tenant_id, upstream_job_id, downstream_job_id)
);

CREATE INDEX IF NOT EXISTS idx_job_dependencies_downstream
    ON job_dependencies (tenant_id, downstream_job_id);

-- Reversible actions, so an operator can undo one within a window
-- (UI.md §38). The prior state is captured when the action is taken.
CREATE TABLE IF NOT EXISTS undo_log (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    -- What was changed: `JOB_STATUS`, `QUEUE_PAUSE`, `JOB_DELETE`.
    action VARCHAR(40) NOT NULL,
    resource_type VARCHAR(40) NOT NULL,
    resource_id UUID NOT NULL,
    -- The value to restore, as JSON.
    previous_state JSONB NOT NULL,
    -- `AVAILABLE`, `UNDONE`, `EXPIRED`.
    status VARCHAR(20) NOT NULL DEFAULT 'AVAILABLE',
    expires_at TIMESTAMPTZ NOT NULL,
    undone_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT undo_log_status_check
        CHECK (status IN ('AVAILABLE', 'UNDONE', 'EXPIRED'))
);

-- An entry is undoable only while it is still available and unexpired.
CREATE UNIQUE INDEX IF NOT EXISTS idx_undo_log_available
    ON undo_log (tenant_id, resource_type, resource_id)
    WHERE status = 'AVAILABLE';

CREATE INDEX IF NOT EXISTS idx_undo_log_user
    ON undo_log (tenant_id, user_id, created_at DESC);

-- Scheduler and queue latency samples (UI.md §73 reports scheduler lag and
-- queue latency). One row per observation.
CREATE TABLE IF NOT EXISTS scheduler_heartbeats (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    -- How long the scheduler took to complete its last evaluation pass.
    lag_ms INT NOT NULL,
    executions_dispatched INT NOT NULL DEFAULT 0,
    observed_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_scheduler_heartbeats_time
    ON scheduler_heartbeats (tenant_id, observed_at DESC);