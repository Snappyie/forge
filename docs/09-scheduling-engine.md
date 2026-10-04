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

All three schedule kinds are evaluated by one calculator, so the preview of a
one-time or interval schedule is the same series the loop materialises.

## 9.14 Schedule kinds

Spec 01.5 requires one-time and fixed-interval schedules alongside cron.

- `CRON` is unchanged: five-field dialect (ADR-0013), IANA timezone, the
  first-occurrence DST policy (ADR-0014), and the SKIP/FIRE_ONCE/CATCH_UP
  misfire policies.
- `ONE_TIME` fires exactly once, at the UTC instant stored in
  `schedules.one_time_at`. After it fires, `next_run_at` is cleared and the
  schedule is disabled with `disabled_reason = COMPLETED_ONE_TIME`. That is a
  completion, not an error and not an operator pause, so `is_paused` stays
  false. The claim lease and the `(schedule_id, scheduled_for)` unique index
  keep it at-most-once across restarts and concurrent schedulers.
- `INTERVAL` fires every `schedules.interval_seconds` seconds, counted from the
  schedule's stored `next_run_at`. That stored occurrence is the series anchor,
  so a restart cannot re-anchor the series, and catch-up honours the same
  `catch_up_limit` it does for cron.

A stored configuration that cannot be evaluated — a malformed cron, a
non-positive interval, a one-time schedule with no instant, or a one-time
schedule whose `next_run_at` disagrees with `one_time_at` — is disabled with
`disabled_reason = INVALID_CONFIGURATION` and logged. It is not retried on
every tick.

`disabled_reason` is NULL while a schedule is enabled or was paused by an
operator; `is_paused` distinguishes the latter.

## 9.15 Pinned target versions

`target_version_policy = PINNED` runs the version recorded in
`schedules.pinned_version_id`, resolved within the schedule's tenant and target
job. A pin that no longer resolves to a version of that job causes the
occurrence to be skipped with a warning rather than running the wrong version.
`LATEST_PUBLISHED` keeps its existing behaviour: the job's current version,
else the latest published version.

