# 3. Use Cases

## UC-001 Create a job

Actor: Developer.

Preconditions:
- Authenticated.
- Authorized for the tenant.

Flow:
1. Create job metadata.
2. Validate execution configuration.
3. Validate retry/timeout/concurrency/resource configuration.
4. Persist draft.
5. Return job ID and version.

Acceptance:
- Invalid configuration is rejected.
- No partially created resource remains.
- Audit event is emitted.

## UC-002 Publish a job

1. Validate complete configuration.
2. Validate referenced queue/secret/capability.
3. Freeze version.
4. Mark published.
5. Optionally activate schedule.
6. Emit audit event.

## UC-003 Schedule a job

1. Validate schedule expression.
2. Resolve timezone.
3. Calculate next occurrence.
4. Persist schedule.
5. Audit.

## UC-004 Trigger manually

1. Authorize.
2. Resolve target version.
3. Validate input.
4. Check concurrency/idempotency.
5. Create execution.
6. Enqueue.
7. Return execution ID.

## UC-005 Execute a job

1. Worker polls/receives work.
2. Scheduler assigns lease.
3. Worker acknowledges dispatch.
4. Worker starts execution.
5. Worker sends heartbeat.
6. Worker reports completion.
7. Server validates lease.
8. Server persists terminal state.
9. Queue capacity is released.
10. Audit/metrics/events emitted.

## UC-006 Worker crash

1. Worker stops heartbeats.
2. Lease expires.
3. Scheduler marks attempt abandoned.
4. Recovery policy determines whether to retry.
5. If recoverable, execution returns to queue.
6. If not, execution becomes dead-lettered/failed.
7. Recovery event is emitted.

## UC-007 Retry

1. Attempt fails.
2. Error classified.
3. Retry policy evaluated.
4. If retryable and attempts remain, compute delay.
5. Create retry schedule.
6. Requeue.
7. Otherwise finalize failure/dead-letter.

## UC-008 Cancel execution

1. User requests cancellation.
2. Authorization checked.
3. Execution enters CANCEL_REQUESTED.
4. Worker receives cancellation.
5. Worker stops work.
6. Server records CANCELLED.
7. If worker cannot confirm, server applies configured cancellation timeout/recovery policy.

## UC-009 Workflow execution

1. Workflow execution created.
2. Root nodes become runnable.
3. Scheduler creates task executions.
4. Independent nodes run concurrently subject to policies.
5. Completed nodes unlock downstream nodes.
6. Workflow finishes when terminal condition is satisfied.

## UC-010 Reconciliation workflow

Reference payment workload:
1. Fetch provider file.
2. Validate file.
3. Parse records.
4. Match internal transactions.
5. Produce discrepancies.
6. Generate report.
7. Notify operator.

No provider credentials or real financial transactions are required by Forge itself.

## UC-011 Audit investigation

Operator filters audit events by:
- actor
- resource
- action
- time
- result
- tenant

The system MUST preserve event ordering by event timestamp plus stable event ID.

## UC-012 Upgrade

1. Backup metadata.
2. Validate migration.
3. Run migration.
4. Start compatible application.
5. Verify health.
6. Verify scheduler.
7. Resume scheduling.

Backward compatibility requirements are defined in deployment specification.
