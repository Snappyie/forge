# 2. Domain Model

## 2.1 Identifiers

Use opaque IDs rather than exposing database sequential IDs.

Recommended representation:
- UUIDv7 for sortable identifiers.
- String serialization in API.
- Database native UUID if supported.

Every resource has:
- `id`
- `tenant_id` where tenant-scoped
- `created_at`
- `updated_at` where mutable

## 2.2 Job

```text
Job
- id
- tenant_id
- key
- name
- description
- status
- current_version_id
- default_queue_id
- labels
- owner_id
- created_at
- updated_at
```

`key` is a stable human-readable unique key within a tenant.

## 2.3 Job version

A version contains immutable executable configuration.

```text
JobVersion
- id
- job_id
- version_number
- execution_type
- execution_config
- input_schema
- timeout
- retry_policy
- concurrency_policy
- resource_requirements
- environment
- created_by
- created_at
- published_at
```

Once published, the version MUST NOT be mutated.

## 2.4 Schedule

```text
Schedule
- id
- tenant_id
- target_type
- target_id
- target_version_policy
- schedule_type
- expression
- timezone
- misfire_policy
- catch_up_policy
- enabled
- next_run_at
- last_run_at
```

## 2.5 Execution state machine

```text
SCHEDULED
    |
    v
QUEUED
    |
    v
DISPATCHED
    |
    v
RUNNING
  / | \
 /  |  \
v   v   v
SUCCEEDED FAILED TIMED_OUT
       |
       v
 RETRY_SCHEDULED
       |
       v
    QUEUED

RUNNING -> CANCEL_REQUESTED -> CANCELLED
DISPATCHED/RUNNING -> ABANDONED -> RECOVERED -> QUEUED
FAILED -> DEAD_LETTERED
```

Terminal:
- SUCCEEDED
- FAILED
- CANCELLED
- TIMED_OUT
- DEAD_LETTERED

`ABANDONED` is an intermediate recovery state unless product policy explicitly makes it terminal.

## 2.6 Transition rules

- SCHEDULED → QUEUED: scheduler creates durable dispatch record.
- QUEUED → DISPATCHED: worker lease acquired.
- DISPATCHED → RUNNING: worker acknowledges start.
- RUNNING → SUCCEEDED: valid completion.
- RUNNING → FAILED: non-retryable or final failed attempt.
- RUNNING → TIMED_OUT: server timeout reached.
- RUNNING → CANCEL_REQUESTED: user/system requests cancellation.
- CANCEL_REQUESTED → CANCELLED: cancellation confirmed or policy marks execution cancelled.
- FAILED → RETRY_SCHEDULED: retry policy permits another attempt.
- RETRY_SCHEDULED → QUEUED: retry delay elapsed.
- DISPATCHED/RUNNING → ABANDONED: lease expired and recovery started.
- ABANDONED → QUEUED: recovery policy permits re-execution.
- FAILED → DEAD_LETTERED: no retry remains or operator explicitly dead-letters.

Invalid transitions MUST return a domain error.

## 2.7 Attempt

An execution may have multiple attempts.

```text
Attempt
- id
- execution_id
- attempt_number
- worker_id
- lease_id
- started_at
- ended_at
- status
- exit_code
- error_code
- error_message
- metrics
```

Attempt numbers start at 1.

## 2.8 Worker

```text
Worker
- id
- tenant_id or global scope
- name
- version
- status
- capabilities
- resources
- labels
- last_heartbeat_at
- registered_at
- draining
```

Worker statuses:
- REGISTERING
- READY
- BUSY
- DRAINING
- OFFLINE
- REVOKED

## 2.9 Lease

```text
Lease
- id
- execution_id
- worker_id
- acquired_at
- expires_at
- renewed_at
- released_at
```

Lease ownership is exclusive.

## 2.10 Workflow

A workflow is a named versioned DAG.

```text
Workflow
- id
- tenant_id
- key
- name
- description
- status
- current_version_id
```

Workflow version:
- nodes
- edges
- execution policy
- concurrency
- timeout
- metadata

## 2.11 Node dependency semantics

Default edge condition:
`UPSTREAM_SUCCEEDED`.

Supported V1 conditions:
- ALL_SUCCEEDED
- ANY_SUCCEEDED
- ALL_COMPLETED
- ALWAYS

The graph MUST be acyclic.

## 2.12 Retry policy

```text
RetryPolicy
- max_attempts
- strategy
- initial_delay
- multiplier
- max_delay
- jitter
- retryable_errors
- non_retryable_errors
```

The scheduler computes the next retry time deterministically from attempt number and policy, except random jitter.

## 2.13 Priority

Priority is an ordered integer or named level mapped to an integer.

Recommended:
- CRITICAL = 1000
- HIGH = 750
- NORMAL = 500
- LOW = 250
- BACKGROUND = 100

The actual queue ordering MUST be documented and tested. Priority MUST NOT create unbounded starvation; an aging mechanism SHOULD be available.

## 2.14 Error taxonomy

Errors MUST be classified:

- VALIDATION
- AUTHENTICATION
- AUTHORIZATION
- NOT_FOUND
- CONFLICT
- RATE_LIMITED
- TRANSIENT
- DEPENDENCY_UNAVAILABLE
- TIMEOUT
- CANCELLATION
- RESOURCE_EXHAUSTED
- PERMANENT
- INTERNAL

Retry behavior MUST use classification, not arbitrary string matching.

## 2.15 Idempotency

An idempotency key identifies an intended mutation.

Rules:
- Same tenant + endpoint + key + same semantic request → same operation/result.
- Same tenant + endpoint + key + different semantic request → conflict.
- Idempotency records have configurable retention.
- The original request fingerprint MUST be stored.
- Sensitive request bodies MUST NOT be stored unnecessarily.

## 2.16 Versioning

Job/workflow definitions are immutable once published.

Editing creates a new draft/version.

Executions always record the exact effective version.

## 2.17 Domain invariants

The implementation MUST have automated tests for every invariant in this document.
