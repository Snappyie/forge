-- 009_alerts_incidents_notifications.sql
--
-- UI.md specifies alerts (§28), incidents (§29), alert configuration (§30),
-- notifications (§50) and notification preferences (§51). Those screens need
-- real records behind them; without a schema the console would have to invent
-- them, which is exactly what it must not do.

-- An alert rule: what to watch and when to fire.
CREATE TABLE IF NOT EXISTS alert_rules (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    -- `EXECUTION_FAILED`, `SLA_VIOLATION`, `QUEUE_BACKLOG`, `WORKER_OFFLINE`,
    -- `LATENCY_SPIKE`, `SCHEDULE_MISSED`.
    kind VARCHAR(40) NOT NULL,
    name VARCHAR(255) NOT NULL,
    -- Optional scoping: which job, queue, or worker the rule watches.
    target_type VARCHAR(30),
    target_id UUID,
    -- Rule-specific thresholds, e.g. {"failures": 3, "window_seconds": 900}.
    config JSONB NOT NULL DEFAULT '{}'::jsonb,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    -- Notifications fire no more often than this, to avoid a storm.
    cooldown_seconds INT NOT NULL DEFAULT 900,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT alert_rules_kind_check CHECK (kind IN (
        'EXECUTION_FAILED', 'SLA_VIOLATION', 'QUEUE_BACKLOG',
        'WORKER_OFFLINE', 'LATENCY_SPIKE', 'SCHEDULE_MISSED'
    )),
    CONSTRAINT alert_rules_cooldown_check CHECK (cooldown_seconds >= 0),
    CONSTRAINT alert_rules_tenant_name_unique UNIQUE (tenant_id, name)
);

CREATE INDEX IF NOT EXISTS idx_alert_rules_enabled
    ON alert_rules (tenant_id) WHERE enabled = TRUE;

-- A firing alert. Append-only from the application's point of view: an alert is
-- acknowledged, never edited.
CREATE TABLE IF NOT EXISTS alerts (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    rule_id UUID REFERENCES alert_rules(id) ON DELETE SET NULL,
    kind VARCHAR(40) NOT NULL,
    severity VARCHAR(10) NOT NULL,
    title VARCHAR(255) NOT NULL,
    detail TEXT,
    -- What the alert points at, so the console can link straight to it.
    resource_type VARCHAR(30),
    resource_id UUID,
    status VARCHAR(20) NOT NULL DEFAULT 'OPEN',
    acknowledged_by UUID,
    acknowledged_at TIMESTAMPTZ,
    -- Set when the condition cleared on its own.
    resolved_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT alerts_severity_check CHECK (severity IN ('INFO', 'WARNING', 'CRITICAL')),
    CONSTRAINT alerts_status_check CHECK (status IN ('OPEN', 'ACKNOWLEDGED', 'RESOLVED'))
);

-- The console's alerts view sorts newest-first by status.
CREATE INDEX IF NOT EXISTS idx_alerts_tenant_status_created
    ON alerts (tenant_id, status, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_alerts_open
    ON alerts (tenant_id, created_at DESC) WHERE status = 'OPEN';

-- An incident groups the alerts and executions behind one operational problem
-- (UI.md §29).
CREATE TABLE IF NOT EXISTS incidents (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    title VARCHAR(255) NOT NULL,
    summary TEXT,
    severity VARCHAR(10) NOT NULL,
    status VARCHAR(20) NOT NULL DEFAULT 'OPEN',
    -- Several alerts roll up into one incident.
    alert_ids UUID[] NOT NULL DEFAULT '{}',
    impact TEXT,
    root_cause TEXT,
    resolution TEXT,
    detected_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    acknowledged_by UUID,
    acknowledged_at TIMESTAMPTZ,
    resolved_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT incidents_severity_check CHECK (severity IN ('SEV1', 'SEV2', 'SEV3')),
    CONSTRAINT incidents_status_check CHECK (status IN ('OPEN', 'ACKNOWLEDGED', 'RESOLVED'))
);

CREATE INDEX IF NOT EXISTS idx_incidents_tenant_status
    ON incidents (tenant_id, status, detected_at DESC);

-- Incident timeline entries (UI.md §29 shows "related alerts" and a timeline).
CREATE TABLE IF NOT EXISTS incident_events (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    incident_id UUID NOT NULL REFERENCES incidents(id) ON DELETE CASCADE,
    at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    kind VARCHAR(30) NOT NULL,
    message TEXT NOT NULL,
    actor_id UUID,
    CONSTRAINT incident_events_kind_check CHECK (kind IN (
        'DETECTED', 'ALERT', 'ACKNOWLEDGED', 'NOTE', 'MITIGATED', 'RESOLVED'
    ))
);

CREATE INDEX IF NOT EXISTS idx_incident_events_incident
    ON incident_events (incident_id, at);

-- Per-user notification preferences (UI.md §51).
CREATE TABLE IF NOT EXISTS notification_preferences (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    -- Which alert kinds reach this user at all.
    enabled_kinds VARCHAR(40)[] NOT NULL DEFAULT '{}',
    -- Channel choices: {"in_app": true, "email": false, "webhook": false}.
    channels JSONB NOT NULL DEFAULT '{"in_app": true}'::jsonb,
    -- Local time window during which non-critical alerts are held.
    quiet_hours_start TIME,
    quiet_hours_end TIME,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT notification_preferences_unique UNIQUE (tenant_id, user_id)
);

-- Delivered notifications (UI.md §50 bell icon).
CREATE TABLE IF NOT EXISTS notifications (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    alert_id UUID REFERENCES alerts(id) ON DELETE CASCADE,
    title VARCHAR(255) NOT NULL,
    body TEXT,
    resource_type VARCHAR(30),
    resource_id UUID,
    read_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_notifications_user_unread
    ON notifications (user_id, created_at DESC) WHERE read_at IS NULL;