# 9. Scheduling Engine

## 9.1 Responsibilities

The scheduler:
- Evaluates schedules.
- Calculates due work.
- Creates executions.
- Enforces concurrency.
- Places work on queues.
- Handles retry timers.
- Recovers expired leases.
- Maintains next-run state.

It MUST NOT execute business jobs itself.

## 9.2 Schedule evaluation loop

Conceptually:

```text
load due schedules
  -> claim
  -> calculate intended occurrence
  -> create execution
  -> enqueue
  -> calculate next occurrence
  -> persist
```

The operation MUST be idempotent.

## 9.3 Duplicate scheduler instances

Multiple schedulers MAY run simultaneously.

They MUST coordinate using database locking/leases.

Exactly one scheduler should win a specific schedule occurrence.

Duplicate execution creation must be prevented by a uniqueness constraint or equivalent idempotency mechanism.

## 9.4 Cron semantics

Cron parser behavior MUST be documented.

V1 supports standard 5-field or explicitly documented 6/7-field syntax. Pick one and expose the choice in API documentation; do not silently support ambiguous dialects.

## 9.5 Time zones

Every recurring schedule MUST have an IANA timezone.

If omitted, the API MUST reject the schedule or explicitly apply a documented tenant/system default. Silent machine-local timezone behavior is prohibited.

## 9.6 DST

For nonexistent local times:
- apply the documented misfire policy.

For ambiguous times:
- execute according to an explicitly documented first/second occurrence policy.

Tests MUST cover DST transitions.

## 9.7 Misfire policy

Supported:
- SKIP
- FIRE_ONCE
- CATCH_UP

Definitions:

SKIP:
Missed occurrences are discarded.

FIRE_ONCE:
One execution is created for the missed period.

CATCH_UP:
Each missed occurrence is created, subject to configured maximum catch-up count.

## 9.8 Catch-up limits

A schedule MUST have a safety limit for missed occurrences.

Default: 100.

The limit MUST be configurable.

## 9.9 Clock drift

The scheduler MUST use server time.

Large clock drift between nodes SHOULD be detected operationally.

## 9.10 Manual trigger

Manual triggers bypass the calendar but MUST still respect:
- authorization
- concurrency
- resource limits
- queue pause
- tenant limits
- idempotency

## 9.11 Schedule pause

Pausing prevents new scheduled executions.

It does not cancel already-running executions.

## 9.12 Schedule deletion

Deletion/archive prevents future execution.

Historical executions remain queryable according to retention.

## 9.13 Next-run preview

The same schedule calculation engine MUST power:
- UI preview
- API preview
- actual scheduling

There must not be separate implementations.
