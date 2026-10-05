-- 026: jobs carry their environment through creation.
--
-- `JobRepository::create` inserted a row without `environment_id`, so a job
-- could only be placed in an environment by a follow-up UPDATE. Cross-environment
-- migration needs to create a job *in* its destination environment as part of
-- one transaction, and a create-then-update pair leaves a window in which the
-- job exists in no environment at all.

-- Existing jobs keep the tenant's default environment rather than a null, so
-- "which environment does this job run in" has an answer for every row. A null
-- here would mean an invisible, unmanaged environment on legacy data.
UPDATE jobs j
   SET environment_id = d.environment_id
  FROM (
        SELECT t.id AS tenant_id,
               (SELECT e.id FROM environments e
                 WHERE e.tenant_id = t.id
                 ORDER BY (e.slug = 'default') DESC, e.created_at
                 LIMIT 1) AS environment_id
          FROM tenants t
       ) d
 WHERE j.tenant_id = d.tenant_id
   AND j.environment_id IS NULL;

-- Not NULL: every job runs somewhere, and `application_id` stays nullable so an
-- ungrouped job is representable without inventing a container.
ALTER TABLE jobs
    ALTER COLUMN environment_id SET NOT NULL;
