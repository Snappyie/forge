-- 013_registration_bootstrap.sql
--
-- Anonymous registration granting tenant OWNER was reachable on any running
-- instance. Two things make it safe:
--
--   1. `tenant_bootstrap` records that the first tenant has been claimed. Once
--      a row exists, no anonymous caller can create another tenant owner.
--   2. `invites` becomes live: an invited address may register, and only into
--      the tenant the invite names, with the role the invite grants.
--
-- Existing deployments already have a tenant, so the bootstrap flag is set by
-- the first registration that succeeds rather than by migration.

CREATE TABLE IF NOT EXISTS tenant_bootstrap (
    id BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (id),
    -- The tenant the very first owner created, for the audit trail.
    tenant_id UUID REFERENCES tenants(id) ON DELETE SET NULL,
    claimed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    claimed_by UUID
);

-- `invites` existed from the initial auth migration but was never read. Add the
-- columns the registration path needs and an index for token lookup.
ALTER TABLE invites
    ADD COLUMN IF NOT EXISTS used_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS invited_by UUID,
    ADD COLUMN IF NOT EXISTS accepted_user_id UUID;

-- An invite is claimed atomically by setting `used_at`, so two concurrent
-- registrations cannot both consume the same token.
CREATE UNIQUE INDEX IF NOT EXISTS idx_invites_token
    ON invites (token);

CREATE INDEX IF NOT EXISTS idx_invites_email
    ON invites (email)
    WHERE used_at IS NULL;

-- Record that open self-service signup is no longer available for this
-- deployment: the first tenant already exists.
INSERT INTO tenant_bootstrap (id, tenant_id)
SELECT TRUE, (SELECT id FROM tenants ORDER BY created_at ASC LIMIT 1)
WHERE EXISTS (SELECT 1 FROM tenants)
ON CONFLICT (id) DO NOTHING;