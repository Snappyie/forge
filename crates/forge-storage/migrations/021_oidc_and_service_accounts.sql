-- 021: OIDC identities, tenant selection, and service accounts.
--
-- Three gaps `redesign.md` §G names that the schema did not support:
--
--  1. **OIDC/OAuth2 SSO.** `user_identities` existed since migration 005 but no
--     code ever read or wrote it, and it required a `tenant_id`, which is wrong:
--     one identity at an identity provider spans every tenant a user belongs to,
--     so the same `(provider, subject)` pair could not be recorded twice.
--  2. **A user who belongs to several tenants.** `login` picked one membership
--     with `LEFT JOIN ... LIMIT 1` and no ordering, so which tenant you landed in
--     depended on row order rather than on anything the operator chose.
--  3. **Service accounts.** Distinct from both users and workers: a non-human
--     principal with explicit scopes, for CI and for automation.

-- ---------------------------------------------------------------------------
-- Identities
-- ---------------------------------------------------------------------------

-- One row per external identity, not per tenant.
--
-- Dropping the tenant scoping is the point: `(provider, subject)` is the
-- provider's own unique identifier for a person, and one person may be a member
-- of several tenants. Keying on `(tenant_id, provider, subject)` would let the
-- same provider identity be recorded once per tenant, and nothing would tie the
-- duplicates to one `user_id`.
ALTER TABLE user_identities DROP COLUMN IF EXISTS tenant_id;

-- The provider's issuer, so two providers that both issue a `sub` of "42" are
-- not conflated. Without it, `UNIQUE (provider, provider_subject)` collides
-- across issuers for any deployment configured with more than one.
ALTER TABLE user_identities
    ADD COLUMN IF NOT EXISTS issuer VARCHAR(255) NOT NULL DEFAULT '';

-- Backfill an issuer for rows created before this migration so the new unique
-- index below can be built.
UPDATE user_identities SET issuer = 'legacy' WHERE issuer = '';

-- Migration 005 declared this as a table *constraint* rather than a bare index,
-- so it must be dropped as a constraint: `DROP INDEX` fails with "requires it"
-- and the whole migration aborts.
ALTER TABLE user_identities
    DROP CONSTRAINT IF EXISTS user_identities_provider_subject_unique;

CREATE UNIQUE INDEX IF NOT EXISTS user_identities_issuer_subject_unique
    ON user_identities (issuer, provider, provider_subject);

-- Lookup by the provider's own key, which is how a callback resolves a user.
CREATE INDEX IF NOT EXISTS user_identities_user_idx
    ON user_identities (user_id);

-- ---------------------------------------------------------------------------
-- Tenant selection
-- ---------------------------------------------------------------------------

-- The tenant a user last used, so a returning user lands where they left off.
--
-- Nullable: a user with several memberships and no prior session has no
-- preference yet, and defaulting the column would make "no preference" and
-- "prefers the default tenant" indistinguishable.
ALTER TABLE users ADD COLUMN IF NOT EXISTS last_tenant_id UUID
    REFERENCES tenants(id) ON DELETE SET NULL;

-- ---------------------------------------------------------------------------
-- Identity provider configuration
-- ---------------------------------------------------------------------------

-- Registered providers. Kept in the database rather than in configuration so a
-- deployment can add one without a restart, and so an operator can see which
-- providers are trusted without reading a config file.
CREATE TABLE IF NOT EXISTS identity_providers (
    id          UUID PRIMARY KEY,
    -- Globally unique so two tenants cannot register the same issuer and have
    -- one tenant's login silently satisfy the other's.
    name        VARCHAR(64) NOT NULL UNIQUE,
    issuer      VARCHAR(255) NOT NULL,
    client_id   VARCHAR(255) NOT NULL,
    -- The client secret is never returned by any endpoint; see ADR-0023.
    client_secret_encrypted BYTEA NOT NULL,
    scopes      TEXT[] NOT NULL DEFAULT ARRAY['openid', 'profile', 'email'],
    -- A disabled provider stops accepting logins without deleting its
    -- identities, so re-enabling does not orphan every linked account.
    enabled     BOOLEAN NOT NULL DEFAULT TRUE,
    -- Emails from this provider that may create a tenant. NULL means any
    -- authenticated user may; the alternative is a verified domain list, which
    -- is stricter and what an enterprise deployment should configure.
    allowed_email_domains TEXT[],
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ---------------------------------------------------------------------------
-- Service accounts
-- ---------------------------------------------------------------------------

-- Non-human principals. Distinct from users (which have a password or an SSO
-- identity) and from workers (which may only claim and complete their own
-- work). A service account holds explicit scopes rather than a role, because
-- "deploy the prod jobs from CI" and "read the audit log" are different grants
-- and neither should imply the other.
CREATE TABLE IF NOT EXISTS service_accounts (
    id            UUID PRIMARY KEY,
    tenant_id     UUID        NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name          VARCHAR(255) NOT NULL,
    description   TEXT,
    -- Hashed with the API-key pepper, exactly like `api_keys`, so the raw
    -- secret is shown once at creation and never again.
    token_hash    TEXT        NOT NULL,
    token_prefix  VARCHAR(32) NOT NULL,
    -- An empty array means "no access", not "everything": a service account
    -- created without scopes is inert until granted something.
    scopes        TEXT[]      NOT NULL DEFAULT '{}',
    expires_at    TIMESTAMPTZ,
    revoked_at    TIMESTAMPTZ,
    last_used_at  TIMESTAMPTZ,
    created_by    UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS service_accounts_tenant_name_unique
    ON service_accounts (tenant_id, name);

-- Lookup by token hash on every authenticated request.
CREATE UNIQUE INDEX IF NOT EXISTS service_accounts_token_hash_unique
    ON service_accounts (token_hash);

-- The active accounts of a tenant, for the admin list. Partial so revoked
-- accounts do not crowd it.
CREATE INDEX IF NOT EXISTS service_accounts_tenant_active_idx
    ON service_accounts (tenant_id, created_at DESC)
    WHERE revoked_at IS NULL;

-- ---------------------------------------------------------------------------
-- Foreign keys the new tables need
-- ---------------------------------------------------------------------------

-- `users.last_tenant_id` must not point at a tenant the user is not a member
-- of, or a revoked membership would leave a user defaulting into a tenant they
-- can no longer enter. A trigger enforces it because a composite foreign key
-- cannot reference `tenant_memberships` without repeating its columns.
CREATE OR REPLACE FUNCTION users_last_tenant_is_member() RETURNS TRIGGER
    LANGUAGE plpgsql
    AS $$
    BEGIN
        IF NEW.last_tenant_id IS NULL THEN
            RETURN NEW;
        END IF;
        IF NOT EXISTS (
            SELECT 1 FROM tenant_memberships m
             WHERE m.user_id = NEW.id AND m.tenant_id = NEW.last_tenant_id
        ) THEN
            RAISE EXCEPTION
                'last_tenant_id % is not a tenant the user belongs to',
                NEW.last_tenant_id
                USING ERRCODE = '23514';
        END IF;
        RETURN NEW;
    END;
    $$;

DROP TRIGGER IF EXISTS users_last_tenant_is_member_trg ON users;
CREATE TRIGGER users_last_tenant_is_member_trg
    BEFORE INSERT OR UPDATE OF last_tenant_id ON users
    FOR EACH ROW
    WHEN (NEW.last_tenant_id IS NOT NULL)
    EXECUTE FUNCTION users_last_tenant_is_member();