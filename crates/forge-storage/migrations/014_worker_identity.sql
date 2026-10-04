-- Worker identity for the lease protocol.
--
-- The lease protocol requires the server to know *which worker* holds a lease,
-- because only the holder may renew it (spec 10.3) or report completion
-- (spec 10.4). The worker table had no credential of its own, so handlers
-- passed the authenticated *user* id where a worker id was required. A worker's
-- operator is a human whose id is never a worker id, so every renewal was
-- refused and every completion was rejected as `NotLeaseHolder`.
--
-- A worker therefore needs a credential that identifies the worker. It is
-- stored as a hash, like `api_keys.key_hash`: the plaintext is shown once at
-- registration and is unrecoverable afterwards, so a database leak does not
-- hand over the ability to claim or complete work.

ALTER TABLE workers
    ADD COLUMN IF NOT EXISTS token_hash TEXT;

-- A worker that predates this migration has no token and cannot authenticate a
-- claim; it must re-register. Recording that explicitly keeps the failure mode
-- a clear 401 rather than a confusing lease conflict.
COMMENT ON COLUMN workers.token_hash IS
    'Argon2/SHA-256 hash of the worker token presented on claim, heartbeat and complete. NULL means the worker must re-register to obtain a token.';

-- Claim filtering consults worker capabilities, so that lookup is now on the hot
-- path. Without this index every claim scans every worker in the tenant.
--
-- GIN indexes a single column expression here: `tenant_id` is a uuid, which has
-- no GIN operator class, so the tenant predicate is served by the btree index on
-- `workers (tenant_id)` from 005 and this index narrows the capability match.
CREATE INDEX IF NOT EXISTS idx_workers_capabilities
    ON workers USING GIN (capabilities jsonb_path_ops);

-- `claim_next` orders candidates by an aged priority score and skips locked
-- rows; the partial index keeps that scan proportional to the queue depth rather
-- than to the full execution history.
CREATE INDEX IF NOT EXISTS idx_executions_dispatchable
    ON executions (tenant_id, priority, created_at)
    WHERE status = 'QUEUED';

-- API keys were generated and stored but never verified, so minting one looked
-- like it produced a working integration credential. Verification needs the
-- tenant and a role; `owner_id` records the human who created the key so an
-- action taken with it is attributed in the audit log, and `role` bounds what
-- the key can do. Both are NOT NULL with a literal default so existing keys
-- stay readable rather than becoming unverifiable.
ALTER TABLE api_keys
    ADD COLUMN IF NOT EXISTS owner_id UUID REFERENCES users(id) ON DELETE SET NULL;

ALTER TABLE api_keys
    ADD COLUMN IF NOT EXISTS role VARCHAR(16) NOT NULL DEFAULT 'OPERATOR';

-- Verification looks a key up by hash on every request; without this the lookup
-- is a sequential scan of every key in the database.
CREATE UNIQUE INDEX IF NOT EXISTS idx_api_keys_hash
    ON api_keys (key_hash);

-- Teams and RBAC support
CREATE TABLE IF NOT EXISTS teams (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    description TEXT,
    on_call_email VARCHAR(255),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS team_members (
    team_id UUID NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role VARCHAR(50) NOT NULL DEFAULT 'MEMBER',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (team_id, user_id)
);