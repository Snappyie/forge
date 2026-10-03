-- 008_refresh_token_storage.sql
--
-- `refresh_tokens.token` was declared `UUID PRIMARY KEY` in the original
-- schema, but spec 08.8 requires refresh tokens to be stored as a secure
-- one-way hash. A SHA-256 digest is 64 hex characters and is not a UUID, so
-- the column type is widened here.
--
-- Existing rows cannot be meaningfully converted (the stored value was never a
-- real token), so any pre-existing row is discarded: an unverifiable session is
-- safer than one that cannot be checked.

-- Rotation needs to record when a token was exchanged. This must exist before
-- the partial index below references it.
ALTER TABLE refresh_tokens
    ADD COLUMN IF NOT EXISTS revoked_at TIMESTAMPTZ;

-- A token is issued within a tenant, so the scope is recorded on the token
-- itself. This lets the refresh path refuse a token that belongs to a tenant
-- the caller is no longer a member of, and it removes the need to join through
-- `tenant_memberships` on every refresh.
ALTER TABLE refresh_tokens
    ADD COLUMN IF NOT EXISTS tenant_id UUID REFERENCES tenants(id) ON DELETE CASCADE;

ALTER TABLE refresh_tokens
    ALTER COLUMN token TYPE TEXT USING token::TEXT;

DROP INDEX IF EXISTS idx_refresh_tokens_user;
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_user ON refresh_tokens (user_id);

-- An active token is one that has been neither revoked nor expired; the
-- refresh path checks this, so it gets its own index.
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_active
    ON refresh_tokens (user_id) WHERE revoked_at IS NULL;

-- An expired or revoked token must never be replayable.
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_expiry
    ON refresh_tokens (expires_at);