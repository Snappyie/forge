-- Seed data for local development and demos.
--
-- Idempotent: every insert checks for an existing row first, so running it
-- twice does not duplicate anything.
--
-- Apply with:
--   psql "$FORGE_DATABASE_URL" -f scripts/seed.sql

\set ON_ERROR_STOP on

BEGIN;

-- A tenant with a known id, so the CLI and console have something to sign in
-- to.
INSERT INTO tenants (id, name)
VALUES ('11111111-1111-1111-1111-111111111111', 'Demo Tenant')
ON CONFLICT (id) DO NOTHING;

-- The built-in roles are seeded by migration 005; this only needs the tenant's
-- copies, which `forge auth register` creates on first sign-up. Nothing to do
-- here.

-- A default queue, referenced by jobs that want an explicit bound.
INSERT INTO queues (id, tenant_id, name, max_concurrency)
VALUES ('22222222-2222-2222-2222-222222222222',
        '11111111-1111-1111-1111-111111111111',
        'default', 4)
ON CONFLICT (id) DO NOTHING;

-- Three jobs with a published version each, so the console has rows to show
-- and an execution can actually be triggered.
DO $$
DECLARE
    -- Prefixed names so they cannot be confused with the table columns of the
    -- same name, which PL/pgSQL resolves ambiguously.
    v_job_id UUID;
    v_version_id UUID;
    v_spec RECORD;
BEGIN
    FOR v_spec IN
        SELECT * FROM (VALUES
            ('33333333-3333-3333-3333-333333333333'::uuid, 'nightly-settlement', 'Nightly settlement', 'HIGH'),
            ('44444444-4444-4444-4444-444444444444'::uuid, 'reconciliation', 'Reconciliation', 'NORMAL'),
            ('55555555-5555-5555-5555-555555555555'::uuid, 'fraud-scoring', 'Fraud scoring', 'CRITICAL')
        ) AS t(id, key, name, priority)
    LOOP
        v_job_id := v_spec.id;

        INSERT INTO jobs (id, tenant_id, key, name, description, status, priority)
        VALUES (v_job_id, '11111111-1111-1111-1111-111111111111',
                v_spec.key, v_spec.name,
                'Seeded job for local development.', 'ACTIVE', v_spec.priority)
        ON CONFLICT (id) DO NOTHING;

        -- Version 1, published, so the job can be triggered immediately. The id
        -- is derived from the job so a re-run collides predictably rather than
        -- adding a second version.
        v_version_id := gen_random_uuid();
        INSERT INTO job_versions
            (id, tenant_id, job_id, version_number, execution_type,
             execution_config, timeout_seconds, published_at)
        VALUES (v_version_id, '11111111-1111-1111-1111-111111111111', v_job_id, 1,
                'WORKER_TASK', '{"kind":"demo"}'::jsonb, 3600, NOW())
        ON CONFLICT (job_id, version_number) DO NOTHING;

        UPDATE jobs j
        SET current_version_id = v_version_id
        WHERE j.id = v_job_id AND j.current_version_id IS NULL;
    END LOOP;
END
$$;

-- A recurring schedule on the first job, one minute past midnight UTC so it is
-- easy to observe on a short demo.
INSERT INTO schedules
    (id, tenant_id, job_id, target_id, target_type, target_version_policy,
     schedule_type, cron_expression, timezone, misfire_policy, catch_up_policy,
     next_run_at, enabled)
VALUES ('66666666-6666-6666-6666-666666666666',
        '11111111-1111-1111-1111-111111111111',
        '33333333-3333-3333-3333-333333333333',
        '33333333-3333-3333-3333-333333333333',
        'JOB', 'LATEST_PUBLISHED',
        'CRON', '1 0 * * *', 'UTC',
        'FIRE_ONCE', '{"max_occurrences": 100}'::jsonb,
        date_trunc('day', NOW()) + INTERVAL '1 day 1 minute', TRUE)
ON CONFLICT (id) DO NOTHING;

COMMIT;

\echo 'Seed complete.'
\echo 'Seed complete.'
\echo 'Then register an owner:'
\echo '  curl -X POST http://localhost:3000/api/v1/auth/register'
\echo '    -H "content-type: application/json"'
\echo '    -d @scripts/register.json'
