# 6. Event and Queue Contracts

## 6.1 Event envelope

```json
{
  "event_id": "uuid",
  "event_type": "execution.completed",
  "event_version": 1,
  "occurred_at": "UTC timestamp",
  "tenant_id": "uuid",
  "aggregate_type": "execution",
  "aggregate_id": "uuid",
  "correlation_id": "uuid",
  "causation_id": "uuid",
  "payload": {}
}
```

## 6.2 Required event types

- job.created
- job.updated
- job.published
- schedule.created
- schedule.paused
- schedule.resumed
- execution.created
- execution.queued
- execution.dispatched
- execution.started
- execution.retry_scheduled
- execution.succeeded
- execution.failed
- execution.timed_out
- execution.cancel_requested
- execution.cancelled
- execution.recovered
- execution.dead_lettered
- worker.registered
- worker.heartbeat_missed
- worker.draining
- worker.revoked
- workflow.started
- workflow.completed
- workflow.failed
- audit.recorded

## 6.3 Delivery semantics

Default:
- at-least-once.

Consumers MUST be idempotent.

Events MAY be delivered more than once.

Consumers MUST use `event_id` or aggregate/version semantics to deduplicate.

## 6.4 Queue message

A queue item references durable execution state.

It MUST include:
- execution ID
- tenant ID
- queue ID
- priority
- enqueue time
- eligible time
- attempt number
- required capabilities
- resource requirements

The queue message MUST NOT be treated as the authoritative execution record.

## 6.5 Visibility/lease

A worker receiving a queue item receives a lease.

The lease has:
- acquisition timestamp
- expiry timestamp
- renewal interval
- maximum renewal policy

A worker MUST renew before expiry.

## 6.6 Duplicate execution

Duplicate execution is possible after:
- network partition
- worker crash after side effect
- acknowledgement loss
- lease ambiguity.

Forge MUST provide execution identity and idempotency metadata so external jobs can implement safe semantics.

Forge MUST NOT claim universal exactly-once execution.

## 6.7 Ordering

Ordering is guaranteed only within explicitly configured scopes.

Default:
- no global ordering guarantee.

Optional:
- per queue FIFO among equal-priority eligible jobs.
- per key serialization.

Any ordering guarantee MUST be documented per queue.
