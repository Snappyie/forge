-- 002_execution_schema.sql

CREATE TABLE queues (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id),
    name VARCHAR(255) NOT NULL,
    max_concurrency INT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE executions (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id),
    job_id UUID NOT NULL REFERENCES jobs(id),
    job_version_id UUID NOT NULL,
    status VARCHAR(50) NOT NULL,
    queue_id UUID REFERENCES queues(id),
    worker_id UUID,
    attempt_count INT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    started_at TIMESTAMPTZ,
    ended_at TIMESTAMPTZ
);

CREATE TABLE workers (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id),
    hostname VARCHAR(255) NOT NULL,
    status VARCHAR(50) NOT NULL,
    last_heartbeat_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE worker_leases (
    execution_id UUID PRIMARY KEY REFERENCES executions(id),
    worker_id UUID NOT NULL REFERENCES workers(id),
    expires_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX idx_executions_status ON executions(status);
CREATE INDEX idx_worker_leases_expires ON worker_leases(expires_at);
