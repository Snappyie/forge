-- 007_queue_pause_and_user_admin.sql
--
-- Adds the columns the HTTP API requires: a queue must be pausable (spec 05
-- endpoints 43/44, and spec 10.10 lists "queue is active" as a dispatch
-- precondition), and user administration (spec 05 endpoints 45-48) needs a
-- disabled flag and a display name.

ALTER TABLE queues
    ADD COLUMN IF NOT EXISTS paused BOOLEAN NOT NULL DEFAULT FALSE;

ALTER TABLE users
    ADD COLUMN IF NOT EXISTS display_name TEXT,
    ADD COLUMN IF NOT EXISTS disabled BOOLEAN NOT NULL DEFAULT FALSE;

-- `tenant_memberships.role` predates the six-role vocabulary in spec 05 §5.10;
-- widen the constraint so the new roles can be stored.
ALTER TABLE tenant_memberships DROP CONSTRAINT IF EXISTS tenant_memberships_role_check;
ALTER TABLE tenant_memberships ADD CONSTRAINT tenant_memberships_role_check
    CHECK (role IN ('OWNER', 'ADMIN', 'OPERATOR', 'DEVELOPER', 'AUDITOR', 'VIEWER'));

ALTER TABLE invites DROP CONSTRAINT IF EXISTS invites_role_check;
ALTER TABLE invites ADD CONSTRAINT invites_role_check
    CHECK (role IN ('OWNER', 'ADMIN', 'OPERATOR', 'DEVELOPER', 'AUDITOR', 'VIEWER'));

-- API keys must be revocable and identifiable by prefix, so a listing can show
-- which key is which without exposing the value.
ALTER TABLE api_keys
    ADD COLUMN IF NOT EXISTS revoked_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS prefix TEXT,
    ADD COLUMN IF NOT EXISTS last_used_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS idx_api_keys_active
    ON api_keys (tenant_id) WHERE revoked_at IS NULL;

-- A paused queue must not be scanned by the dispatcher.
CREATE INDEX IF NOT EXISTS idx_queues_active
    ON queues (tenant_id) WHERE paused = FALSE;