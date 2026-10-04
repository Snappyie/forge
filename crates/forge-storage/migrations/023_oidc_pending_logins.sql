-- 023: a store for in-flight OIDC logins, and a recoverable client secret.
--
-- Two defects in the OIDC flow as first written, both of which make a correct
-- flow impossible:
--
--  1. **The client secret was hashed.** Migration 021 named the column
--     `client_secret_encrypted` and the handler wrote a one-way hash into it. A
--     hash cannot be sent to the token endpoint, so the exchange could never
--     have completed even with everything else correct. The secret must be
--     *encrypted* — recoverable by the server, never readable by anyone holding
--     only the database.
--
--  2. **`state` and the PKCE verifier had nowhere to live.** They were returned
--     to the browser, which defeats both: `state` is only a CSRF defence when
--     the server remembers it, and the PKCE verifier is the secret PKCE exists
--     to protect. They are stored here, keyed by the state token, and deleted on
--     use so a token is single-use.
--
-- The encryption key is the existing `FORGE_API_KEY_HASHING_SECRET`, reusing a
-- key the deployment already sets rather than introducing a second secret an
-- operator has to remember to configure.

-- A pending authorization, between the redirect to the provider and the
-- callback. Both `code_verifier` and `state_token` are secrets; neither is ever
-- returned to the browser.
CREATE TABLE IF NOT EXISTS oidc_pending_logins (
    -- The `state` parameter. Primary key because it is the lookup key and
    -- because a token must be usable at most once.
    state_token    TEXT PRIMARY KEY,
    provider_name  VARCHAR(64)  NOT NULL REFERENCES identity_providers(name) ON DELETE CASCADE,
    issuer         VARCHAR(255) NOT NULL,
    client_id      VARCHAR(255) NOT NULL,
    code_verifier  TEXT         NOT NULL,
    redirect_uri   TEXT         NOT NULL,
    expires_at     TIMESTAMPTZ  NOT NULL,
    created_at     TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

-- The sweep index. Without it the cleanup pass scans every abandoned login.
CREATE INDEX IF NOT EXISTS oidc_pending_logins_expires_idx
    ON oidc_pending_logins (expires_at);

-- ---------------------------------------------------------------------------
-- Recoverable client secrets
-- ---------------------------------------------------------------------------

-- Replaces the hashed column. The plaintext secret is never stored and never
-- returned by any endpoint; the API decrypts it in order to talk to the token
-- endpoint, which is the only reason it is needed at all.
ALTER TABLE identity_providers
    ADD COLUMN IF NOT EXISTS client_secret_ciphertext BYTEA;

-- The hashed column is dropped once the ciphertext column exists.
--
-- It cannot simply be left NOT NULL and unused: every insert that omits it
-- fails with "null value in column client_secret_encrypted violates not-null
-- constraint", which is exactly what happened when the handler started writing
-- ciphertext instead.
--
-- Any rows still holding a one-way hash are unrecoverable by design — a hash
-- cannot be sent to the token endpoint — so they are reported before the drop
-- rather than silently left as providers that can never complete a login.
DO $$
DECLARE
    orphans INTEGER;
BEGIN
    SELECT count(*) INTO orphans
      FROM identity_providers
     WHERE client_secret_ciphertext IS NULL;

    IF orphans > 0 THEN
        RAISE NOTICE
            'dropping % identity_providers row(s) whose client secret was \
             stored as a one-way hash and cannot be recovered; re-register \
             those providers', orphans;
    END IF;

    ALTER TABLE identity_providers
        DROP COLUMN IF EXISTS client_secret_encrypted;
END
$$;

-- From here the secret is mandatory: a provider that cannot authenticate to its
-- issuer is not a working provider.
ALTER TABLE identity_providers
    ALTER COLUMN client_secret_ciphertext SET NOT NULL;

-- ---------------------------------------------------------------------------
-- Row-level security
-- ---------------------------------------------------------------------------

-- `service_accounts` was created in migration 021 *after* migration 020 had
-- already enabled and populated the policy set, so it shipped unprotected
-- while being fully tenant-scoped. Enabling it here closes that gap; the
-- isolation guard test in `tenant_isolation.rs` is extended to match.
DO $$
BEGIN
    IF to_regclass('service_accounts') IS NOT NULL THEN
        ALTER TABLE service_accounts ENABLE ROW LEVEL SECURITY;
        DROP POLICY IF EXISTS service_accounts_tenant_isolation ON service_accounts;
        CREATE POLICY service_accounts_tenant_isolation ON service_accounts
            USING (tenant_id = current_tenant_id())
            WITH CHECK (tenant_id = current_tenant_id());
    END IF;
END
$$;

-- `oidc_pending_logins` has no `tenant_id`: a login is started before any tenant
-- is known. It is keyed by an unguessable state token and deleted on use, so it
-- carries no cross-tenant data and is deliberately not tenant-scoped.
