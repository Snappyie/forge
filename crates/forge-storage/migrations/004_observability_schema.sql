CREATE TABLE execution_audit_trails (
    id UUID PRIMARY KEY,
    execution_id UUID NOT NULL, -- Logical reference to execution
    job_id UUID NOT NULL,       -- Logical reference to job
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    actor_id UUID,              -- Who triggered it (User, API Key, System)
    action VARCHAR(255) NOT NULL, -- e.g. "Triggered", "Failed", "Completed", "Canceled"
    details JSONB,              -- Contextual state or failure reasons
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index for querying audit trails rapidly by execution or tenant
CREATE INDEX idx_execution_audit_trails_execution_id ON execution_audit_trails(execution_id);
CREATE INDEX idx_execution_audit_trails_tenant_id ON execution_audit_trails(tenant_id);
