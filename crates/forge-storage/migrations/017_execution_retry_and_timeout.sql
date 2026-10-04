-- 017_execution_retry_and_timeout.sql
--
-- Makes retries and timeouts durable facts about an execution.
--
-- `RetryPolicy` and `ExecutionStatus::TIMED_OUT` existed in the domain (with
-- unit tests) but nothing in the runtime consulted them: a failure went straight
-- to FAILED, `RETRY_SCHEDULED` was requeued on a fixed five-second timer, and no
-- code ever produced TIMED_OUT even though every job version carries a
-- `timeout_seconds`. Both need somewhere to record *when* something is due, so
-- the sweeper can find it with an index instead of scanning history.

-- When a RETRY_SCHEDULED execution becomes eligible to run again.
ALTER TABLE executions ADD COLUMN IF NOT EXISTS retry_at TIMESTAMPTZ;

-- When a running attempt must be declared TIMED_OUT. Stamped at dispatch from
-- the version's `timeout_seconds`, so the ceiling is the one agreed when the
-- work was handed out (spec 10.7: the server-side timeout is authoritative).
ALTER TABLE executions ADD COLUMN IF NOT EXISTS deadline_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS idx_executions_retry_due
    ON executions (retry_at)
    WHERE status = 'RETRY_SCHEDULED';

CREATE INDEX IF NOT EXISTS idx_executions_overdue
    ON executions (deadline_at)
    WHERE status IN ('DISPATCHED', 'RUNNING');

-- Existing in-flight rows have no deadline; give them one so the sweeper has a
-- defined answer rather than skipping them forever.
UPDATE executions e
SET deadline_at = COALESCE(e.started_at, e.enqueued_at, e.created_at)
                  + make_interval(secs => COALESCE(v.timeout_seconds, 3600))
FROM job_versions v
WHERE v.id = e.job_version_id
  AND e.deadline_at IS NULL
  AND e.status IN ('DISPATCHED', 'RUNNING');
