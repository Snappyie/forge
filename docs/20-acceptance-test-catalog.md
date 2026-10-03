# 20. Acceptance Test Catalog

## Scheduling

AT-SCH-001: one-time job executes once.
AT-SCH-002: recurring schedule calculates next run.
AT-SCH-003: paused schedule creates no new execution.
AT-SCH-004: resumed schedule continues correctly.
AT-SCH-005: timezone schedule executes at intended local time.
AT-SCH-006: DST nonexistent time follows documented policy.
AT-SCH-007: DST ambiguous time follows documented policy.
AT-SCH-008: duplicate schedulers do not create duplicate occurrence executions.
AT-SCH-009: catch-up obeys maximum limit.
AT-SCH-010: manual trigger does not alter future calendar schedule.

## State machine

AT-STATE-001: valid transitions succeed.
AT-STATE-002: invalid transitions fail.
AT-STATE-003: terminal execution cannot return to running.
AT-STATE-004: stale worker completion cannot overwrite recovered execution.

## Retry

AT-RETRY-001: fixed delay.
AT-RETRY-002: exponential delay.
AT-RETRY-003: max delay.
AT-RETRY-004: jitter bounded.
AT-RETRY-005: non-retryable error is not retried.
AT-RETRY-006: max attempts enforced.
AT-RETRY-007: cancellation does not create retry.

## Workers

AT-WKR-001: worker registers.
AT-WKR-002: worker heartbeat updates lease.
AT-WKR-003: expired worker lease is recovered.
AT-WKR-004: drained worker receives no new work.
AT-WKR-005: revoked worker cannot receive work.

## Concurrency

AT-CON-001: job concurrency limit enforced.
AT-CON-002: tenant concurrency enforced.
AT-CON-003: queue concurrency enforced.
AT-CON-004: slot released after terminal completion.
AT-CON-005: slot recovered after abandoned attempt.

## Workflows

AT-WF-001: DAG validates.
AT-WF-002: cycle rejected.
AT-WF-003: sequential dependency.
AT-WF-004: parallel branches.
AT-WF-005: fan-in waits for required branches.
AT-WF-006: upstream failure follows dependency policy.
AT-WF-007: workflow cancellation propagates according to policy.
AT-WF-008: workflow timeout works.

## Multi-tenancy

AT-TEN-001: tenant A cannot read tenant B job.
AT-TEN-002: tenant A cannot trigger tenant B job.
AT-TEN-003: tenant A cannot infer tenant B IDs through error behavior.
AT-TEN-004: tenant-scoped uniqueness works.

## API

AT-API-001: authentication required.
AT-API-002: authorization enforced.
AT-API-003: pagination works.
AT-API-004: invalid input returns standard error.
AT-API-005: idempotent mutation returns same semantic result.
AT-API-006: conflicting idempotency key returns conflict.
AT-API-007: stale version update returns conflict.

## Security

AT-SEC-001: secrets absent from logs.
AT-SEC-002: SSRF protection blocks private targets.
AT-SEC-003: revoked API key rejected.
AT-SEC-004: rate limits apply.
AT-SEC-005: command execution is sandboxed according to executor policy.
AT-SEC-006: audit events are emitted for privileged actions.

## Recovery

AT-REC-001: API restart preserves state.
AT-REC-002: scheduler restart preserves schedules.
AT-REC-003: worker restart recovers leased work.
AT-REC-004: database backup restores usable state.
AT-REC-005: outbox publisher restart does not lose events.

## Observability

AT-OBS-001: execution has correlation ID.
AT-OBS-002: worker metrics emitted.
AT-OBS-003: queue depth visible.
AT-OBS-004: failure includes safe error classification.
AT-OBS-005: dispatch explanation is available.

A release cannot be called production-capable while mandatory acceptance tests are failing.
