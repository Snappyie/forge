-- 012_integration_connection_state.sql
--
-- UI.md section 74 asks, per integration, for a status, the last successful
-- connection, and the last failure. The existing table records only
-- configuration, so those observations are added here.
--
-- Only observations are stored; the connection state is written by whatever
-- component actually talks to the integration.

ALTER TABLE integration_configurations
    ADD COLUMN IF NOT EXISTS last_success_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS last_failure_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS last_error TEXT,
    ADD COLUMN IF NOT EXISTS last_checked_at TIMESTAMPTZ;

-- The console sorts and filters by health, so the open-with-failure case needs
-- an index to stay cheap as the table grows.
CREATE INDEX IF NOT EXISTS idx_integration_configurations_health
    ON integration_configurations (tenant_id, kind, last_checked_at DESC);