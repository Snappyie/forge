# 12. Observability

## 12.1 Logs

Use structured JSON logs in production.

Every log should include where applicable:
- timestamp
- level
- service
- version
- tenant_id
- request_id
- correlation_id
- execution_id
- attempt_id
- worker_id
- job_id
- event
- duration

Never include secrets.

## 12.2 Metrics

### Scheduler
- schedules_evaluated_total
- executions_created_total
- schedule_lag_seconds
- scheduler_loop_duration_seconds
- duplicate_schedule_prevented_total

### Queue
- queue_depth
- queue_oldest_age_seconds
- queue_enqueue_total
- queue_dequeue_total
- queue_wait_seconds

### Execution
- executions_started_total
- executions_succeeded_total
- executions_failed_total
- executions_cancelled_total
- executions_timed_out_total
- executions_retried_total
- execution_duration_seconds
- execution_attempts

### Worker
- workers_registered
- workers_ready
- workers_busy
- worker_heartbeat_age_seconds
- worker_lease_expired_total
- worker_execution_capacity

### API
- http_requests_total
- http_request_duration_seconds
- http_errors_total
- rate_limited_total

### Database
- db_pool_size
- db_pool_wait_seconds
- db_query_duration_seconds
- db_errors_total

## 12.3 Tracing

Trace:
API request → execution creation → queue → worker → executor → completion.

Propagate:
- trace ID
- span ID
- correlation ID

## 12.4 SLO candidates

Initial target examples:
- API availability: 99.9%.
- Scheduler availability: 99.9%.
- No acknowledged durable execution lost.
- 99% of healthy queued jobs dispatched within configured scheduling SLA under normal load.

Exact production SLOs are deployment-specific.

## 12.5 Alerts

Recommended alerts:
- scheduler not progressing;
- queue age excessive;
- worker fleet unavailable;
- lease expiry spike;
- execution failure spike;
- database connection exhaustion;
- outbox backlog;
- API error rate;
- disk usage;
- certificate expiry.

## 12.6 Explainability

For each dispatch decision, retain enough metadata to answer:
- why this worker?
- why not other workers?
- why now?
- why this priority?
- why retry?
- why not retry?
- why was the job delayed?
