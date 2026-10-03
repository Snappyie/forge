# 10. Execution Engine

## 10.1 Worker protocol

Worker lifecycle:

```text
REGISTER
  ↓
READY
  ↓
CLAIM/RECEIVE
  ↓
RUNNING
  ↓
HEARTBEAT
  ↓
COMPLETE
  ↓
READY
```

Drain:

```text
READY/BUSY → DRAINING → OFFLINE
```

## 10.2 Lease

A lease grants temporary execution ownership.

The server records:
- worker
- execution
- expiry
- last renewal

Renewals must verify ownership.

## 10.3 Lease duration

Lease duration MUST exceed heartbeat interval by a documented safety factor.

Example:
- heartbeat every 5s
- lease 20s

Actual values MUST be configurable.

## 10.4 Completion

Completion must include:
- execution ID
- attempt ID
- lease ID
- result
- duration
- exit/error information
- output reference where applicable

Server MUST reject stale lease completion.

## 10.5 Stale completion

If a worker completes after its lease expired and another attempt was legitimately recovered, the stale result MUST NOT overwrite the authoritative execution state.

It MUST be logged and metrically counted.

## 10.6 Cancellation

Cancellation is cooperative.

The server sends a cancellation request.

Worker:
1. receives cancellation;
2. stops work;
3. cleans up;
4. acknowledges.

A hard kill policy MAY exist for container/process execution, but must be explicit.

## 10.7 Timeout

Timeouts are enforced server-side and SHOULD also be enforced by the worker.

Server-side timeout is authoritative.

## 10.8 Retry

Retry decision uses:
- error class
- attempt number
- retry policy
- cancellation status
- workflow policy

Cancellation MUST NOT accidentally become a retry.

## 10.9 Backoff

For exponential:

`delay = min(max_delay, initial_delay * multiplier^(attempt-1))`

Then apply bounded jitter.

Overflow MUST be prevented.

## 10.10 Resource scheduling

Before dispatch:
- worker is healthy;
- worker not draining;
- capabilities match;
- resources are available;
- queue is active;
- tenant/job concurrency permits execution.

## 10.11 Worker crash recovery

If lease expires:
1. mark attempt abandoned;
2. preserve prior attempt data;
3. decide retry/recovery;
4. create new attempt if requeued;
5. increment recovery metrics.

## 10.12 Exactly-once disclaimer

Forge provides durable state and idempotency mechanisms but does not guarantee exactly-once external side effects.

Documentation and UI MUST avoid claiming exactly-once execution.

## 10.13 Worker capability matching

Capabilities may include:
- OS
- architecture
- installed executor
- labels
- region
- custom key/value properties

A job may specify required capabilities.

## 10.14 Execution types

### HTTP
Perform HTTP request with timeout, retry classification, TLS validation, and secret references.

### Process/container
Run configured command/image with limits.

### Worker task
Invoke a registered task type.

Every executor type MUST define:
- input
- output
- timeout
- cancellation
- error classification
- security boundary
