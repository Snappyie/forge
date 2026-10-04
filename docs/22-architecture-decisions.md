# 22. Architecture Decision Register

## ADR-0001 Rust as primary implementation language

Status: Accepted.

Decision:
Use Rust for server/domain/scheduler/worker components.

Reason:
The project is intended to exercise memory safety, concurrency, performance, and reliable systems programming.

## ADR-0002 PostgreSQL as V1 source of truth

Status: Accepted.

Decision:
PostgreSQL is authoritative for execution state.

Reason:
Strong transactions, locking, indexing, operational maturity.

## ADR-0003 Safe Rust by default

Status: Accepted.

Decision:
Forbid unsafe unless explicitly approved.

## ADR-0004 At-least-once execution semantics

Status: Accepted.

Decision:
The platform provides at-least-once infrastructure semantics and idempotency primitives.

Reason:
Exactly-once external side effects cannot be universally guaranteed.

## ADR-0005 Versioned immutable job definitions

Status: Accepted.

Decision:
Published versions cannot mutate.

Reason:
Reproducibility and auditability.

## ADR-0006 Domain/infrastructure separation

Status: Accepted.

Decision:
Core domain must not depend on HTTP/database frameworks.

Reason:
Testability and long-term maintainability.

## ADR-0007 Open-source licensing

Status: Accepted.

Decision:
MIT OR Apache-2.0 for code unless a later legal review selects another compatible license.

## ADR-0008 API versioning

Status: Accepted.

Decision:
Public API is versioned from its first release.

## ADR-0009 Workflow DAG model

Status: Accepted.

Decision:
V1 workflows are directed acyclic graphs.

Reason:
Deterministic dependency semantics and manageable execution model.

## ADR-0010 Durable outbox

Status: Accepted.

Decision:
Use an outbox for reliable event publication.

## ADR-0011 Execution `ABANDONED` is an intermediate state, not terminal

Status: Accepted.

Amends: 02-domain-model.md §2.5/§2.6.

Decision:
`ABANDONED` is not a terminal state. The permitted transitions out of it are
`ABANDONED -> QUEUED` (recovery re-queues the work) and
`ABANDONED -> DEAD_LETTERED` (recovery gives up).

Reason:
The specification's own transition table lists `ABANDONED -> QUEUED`, and §2.5
states that `ABANDONED` is an intermediate recovery state unless product policy
makes it terminal. Treating it as terminal makes that required transition
unreachable, so a lost worker would strand its execution permanently — in
direct conflict with the "no silent job loss" design principle in README.md.

`RECOVERED`, which appears in the §2.5 diagram, is not a state: it has no entry
in the §2.6 transition table and no terminal classification. It is an artifact
of the diagram and is not implemented.

## ADR-0012 `FAILED` and `TIMED_OUT` permit retry and dead-letter exits

Status: Accepted.

Amends: 01-product-requirements.md invariant 3; 02-domain-model.md §2.6.

Decision:
A terminal execution may not return to an active state, with two documented
exceptions: `FAILED -> RETRY_SCHEDULED` and `FAILED -> DEAD_LETTERED`, plus the
equivalent pair for `TIMED_OUT`.

Reason:
Invariant 3 forbids terminal executions from becoming active again, while §2.6
requires retry and dead-letter transitions out of `FAILED`. Reading the
invariant as absolute would make the retry table unreachable. The exception is
narrow: neither transition re-enters `QUEUED`, `DISPATCHED` or `RUNNING`, so
the invariant's intent is preserved.

## ADR-0013 Five-field cron dialect

Status: Accepted.

Amends: 09-scheduling-engine.md §9.4.

Decision:
V1 recurring schedules use standard five-field cron syntax
(`minute hour day-of-month month day-of-week`). Six- and seven-field
extensions are rejected rather than inferred. Days of the week are numbered
from **Sunday**, where `0` is Sunday and `1` is Monday.

Reason:
§9.4 explicitly defers the dialect choice but forbids silently supporting
ambiguous dialects. Pinning one dialect and documenting it in the OpenAPI
specification satisfies both halves of that requirement.

Day-of-week numbering needs stating separately because the two conventions in
common use disagree: the Unix convention treats `1` as Monday, whereas the
underlying parser numbers from Sunday, so `1-5` means Sunday through Thursday
rather than "weekdays". A schedule written under the Unix reading would fire on
the wrong days. The OpenAPI description MUST carry this explicitly, and
`forge_scheduler::schedule` tests pin the behaviour so it cannot drift.

Implementation note: the parser takes a *six*-field expression beginning with
seconds. Forge's dialect omits seconds, so a fixed `0 ` is prepended during
parsing. Passing the raw string through would read `0 2 * * *` as
second=0/minute=2, shifting every occurrence by one field.

## ADR-0014 Ambiguous local times fire once, at the first occurrence

Status: Accepted.

Amends: 09-scheduling-engine.md §09.6.

Decision:
Where a local time occurs twice during a DST fall-back, the schedule fires
once, at the first (earlier) occurrence. Nonexistent local times during a
spring-forward shift forward past the gap, and the schedule's misfire policy
then decides whether the shifted instant still fires.

Reason:
§09.6 requires an explicitly documented first/second policy. Firing once
avoids the duplicate execution that a "both" policy would produce, and the
database constraint on `(schedule_id, scheduled_for)` would reject the second
write anyway.

Implementation note: the underlying cron iterator walks local wall-clock time
and steps *over* an ambiguous hour rather than emitting both candidates. That
is compatible with this ADR — one wall-clock time yields at most one execution —
but the practical consequence is that a schedule targeting the repeated hour
does not run at all on the fall-back day. This is documented operator-facing
behaviour, not a silent surprise.

## ADR-0015 A schedule's timezone is required

Status: Accepted.

Amends: 09-scheduling-engine.md §9.5.

Decision:
A recurring schedule without an IANA timezone is rejected, unless
`FORGE_DEFAULT_TIMEZONE` is set explicitly, in which case that value is applied
and recorded on the schedule. The built-in default for the setting is `UTC`.

Reason:
§09.5 prohibits silent machine-local timezone behaviour. Making the deployment
default explicit and persisting it keeps schedules reproducible across hosts.

## ADR-0016 Cancellation is cooperative only

Status: Accepted.

Amends: 10-execution-engine.md §10.6.

Decision:
V1 provides no hard-kill policy for cancellation. The server records the request,
the worker observes it, stops, cleans up and acknowledges, and the server-side
timeout remains authoritative.

Reason:
§10.6 states a hard-kill policy MAY exist but must be explicit. Omitting it
entirely is explicit, and avoids implying a containment guarantee that a
container executor cannot make across arbitrary runtimes.

## ADR-0017 Runtime SQL queries instead of compile-time macros

Status: Accepted.

Decision:
All SQL uses `sqlx::query_as::<_, T>` with `#[derive(FromRow)]`. The
compile-time `query!`/`query_as!` macros are not used, and no `.sqlx` offline
cache is committed.

Reason:
A committed query cache must be regenerated whenever a query or schema changes.
Without it, a clean checkout or a CI clone cannot compile at all, which
contradicts the README requirement that contributors can run the complete stack
locally. Row-shape structs give the same type safety at the boundary.

## ADR-0018 Status vocabularies use CHECK constraints, not native enums

Status: Accepted.

Decision:
Status columns are `VARCHAR` plus a named `CHECK` constraint.

Reason:
Adding a value to a PostgreSQL enum requires DDL that cannot run inside a
transaction in older server versions, which conflicts with the forward-only
migration requirement in 08-storage-specification.md §8.1. CHECK constraints are
dropped and recreated in place.

## ADR-0019 Tenant slugs are addressable identifiers, derived by trigger

Status: Accepted.

Amends: 05-api-specification.md (no tenant addressing existed); 08-storage-specification.md §8.

Decision:
A tenant gains a `slug` that is unique, non-null, and stable for the life of the
tenant. It is the HTTP-facing identifier (`/tenants/{slug}`). A `BEFORE INSERT`
trigger fills it from the tenant id when the caller does not supply one.

Reason:
A tenant was previously reachable only through a bearer claim, with no route that
addresses one. `redesign.md` §G requires per-tenant isolation to be an operable
model, and an operator cannot administer, switch between, or audit a tenant that
has no name they can type.

The slug is derived rather than required at every call site because several code
paths create a tenant without naming one; making the value mandatory in
application code is a slug some caller will eventually forget, which would fail at
runtime rather than at compile time.

Implementation note:
The value is set by a trigger, not a `DEFAULT`. Postgres rejects a column
reference in a `DEFAULT` expression (`cannot use column reference in DEFAULT
expression`) — including indirectly through a function call — so a slug derived
from the row's own id is not expressible as a default. This was found by applying
the migration to an empty database; it passes on a populated one only because no
row is inserted while the column still allows null.

## ADR-0020 An `ON CONFLICT` target must be backed by a unique index

Status: Accepted.

Amends: 06-event-and-queue-contracts.md; 10-execution-engine.md §10.2.

Decision:
Every `INSERT ... ON CONFLICT` in the storage layer names a column set that a
unique index or constraint actually backs. Adding a conflict target and its
index is one change, and a migration that adds an `ON CONFLICT`-dependent column
set adds the index with it.

Reason:
`ExecutionRepository::create` provisions a tenant's `default` queue with
`ON CONFLICT (tenant_id, name) DO NOTHING`, but no migration ever created that
unique index. Postgres requires one to arbitrate the conflict, so every
execution created for a tenant without a pre-existing `default` queue failed with
`42P10`. The SELECT that runs first hid the defect on a seeded database and only
surfaced on a fresh one — 35 integration tests across four suites failed while
the same tests passed against a database that already had a `default` queue.

Beyond the outage, the missing index also removed the guarantee the surrounding
comment claimed: without it, two concurrent inserts can both succeed and the
follow-up `fetch_one` becomes a race.

## Future ADR candidates

- Queue implementation.
- Authentication mechanism.
- Secret backend abstraction.
- Executor sandbox.
- Broker adapter.
- Multi-region model.
- Workflow language.
- WASM executor.
- Event trigger model.
