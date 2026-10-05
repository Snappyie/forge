# Acceptance matrix

`Redesign.md` §6 defines nine areas and, crucially, says what "done" means:

> Before declaring the platform production-ready, test behavior under failure —
> **not just whether API endpoints return success.**

This document maps each area to the tests that hold it up, and says honestly
where coverage is thin.

## How to run

```bash
# Everything, against a live PostgreSQL.
DATABASE_URL=postgres://forge:forgepassword@localhost:5432/forgedb cargo test --workspace

# Just the matrix.
cargo test -p forge-api --test acceptance_matrix

# SDK contract and live-execution coverage.
./scripts/verify-sdks.sh

# Backup and restore round trip. Needs a `pg_dump`/`pg_restore` that is not older
# than the server; set FORGE_PG_DUMP / FORGE_PG_RESTORE if yours is.
./scripts/verify-backup-restore.sh

# Load measurement. Slow, so ignored by default.
cargo test -p forge-storage --test load -- --ignored --nocapture
```

The matrix needs PostgreSQL. Without it the integration tests skip rather than
fail, so a checkout with no database still compiles and runs the unit tests.

## The matrix

| Area | Criterion from §6 | Covered by |
| --- | --- | --- |
| Scheduling | No unintended duplicate occurrence creation under concurrent activity | `capacity::concurrent_claims_produce_exactly_one_winner`, `capacity::an_execution_has_at_most_one_active_lease` |
| Recovery | Expired worker leases are reconciled and eligible jobs recover safely | `recovery::an_expired_lease_leaves_the_execution_claimable_again`, `recovery::a_live_lease_is_not_recovered` |
| Retries | Attempt history and backoff persist across restarts | `retries::a_transient_failure_schedules_a_retry`, `retries::a_permanent_failure_is_not_retried`, `retries::attempt_history_survives_a_new_connection` |
| Security | Workers cannot access or complete executions outside their authorization | `security::a_worker_token_cannot_create_a_job`, `security::a_worker_cannot_complete_an_unclaimed_execution`, `security::a_token_cannot_read_another_tenants_execution` |
| Capacity | Concurrency limits and queue policies are enforced under load | `capacity::*` in this matrix, plus `forge-storage/tests/concurrency_acceptance.rs` (9 tests, `AT_CON-001`..`AT_CON-005`, covering job-, tenant- and queue-level limits, slot release on every terminal state, and recovery of an abandoned slot) |
| Observability | Every execution can be traced through dispatch, attempts, logs and final outcome | `observability::logs_survive_and_can_be_read_back`, `observability::a_completed_execution_records_its_outcome` |
| Deployment | Migrations, restart, backup and recovery procedures are tested | `deployment::migrations_are_idempotent`, `deployment::a_fresh_database_has_the_core_tables`, `deployment::tenant_tables_have_row_level_security_enabled`, `scripts/verify-sdks.sh` (starts and stops a real server), `scripts/verify-backup-restore.sh` (round trip) |
| Compatibility | SDKs and API versions have automated contract tests | `sdk/{python,go,node}/tests`, `scripts/verify-sdks.sh`, the OpenAPI drift guards in `forge-api` |
| Workflows | Dependency, branch, fan-out, cancellation and recovery tests pass | `forge-executor/tests/workflow_runtime.rs`, `forge-api/tests/partial_rerun.rs` |

## What the matrix found

Writing these tests surfaced four real defects, none of which the existing
suites caught. Each is a case of a property being asserted in a way that could
pass while the system was broken.

**`worker_leases` had no row-level security, and its queries were unscoped.**
`LeaseRepository::active_for_execution` was scoped only by an execution id the
caller supplied, so a caller could ask about another tenant's lease.
`claim_expired` — the query the reaper runs on every tick — carried no
`tenant_id` predicate at all. Migration 027 adds the policy and both queries now
say which tenant they mean.

**The worker detail page asked for the lease on `Uuid::nil()`.** An execution id
that cannot exist, so the query always returned `None` and the page reported no
load however busy the worker was. Replaced with a query by worker.

**A validation schema change was needed for `POST /jobs`.** It could not set
`default_queue_id`, and `POST /jobs/{id}/trigger` hardcoded `queue_id: None`,
discarding the job's queue binding — so every triggered execution sat `QUEUED`
with no queue and no worker could claim it. Found while driving the SDKs, not by
reading code.

**`backup.sh` failed on a directory that did not exist.** It never created its
target directory, so the first run on a fresh checkout died with `pg_dump: could
not open output file`, which reads like a permissions problem. It also wrote no
dump and reported nothing when the dump was empty - a file that looks like a
backup and is discovered to be worthless during a restore. Both fixed, and
`verify-backup-restore.sh` now proves the round trip: write known data, back up,
**destroy the database**, restore, read the data back, and start the server
against it. It was itself checked by sabotaging the dump, which it caught.

**All four SDKs omitted `lease_id` and `error_class`.** Retries were unreachable
and a completion after lease expiry was silently discarded, so work was
re-dispatched and its side effect applied twice. `scripts/verify-sdks.sh` exists
because "the SDK compiles" never caught it.

## Coverage that is thinner than the table implies

Stated plainly, because a matrix that reads as complete when it is not is worse
than no matrix:

- **Throughput has a measurement but no target.** `crates/forge-storage/tests/load.rs`
  measures the claim path under contention and prints throughput and p99
  latency, but deliberately asserts no minimum: §6 says to choose targets after
  estimating the expected workload, and inventing one here would be a number
  with no deployment behind it. On the development machine it reports ~450
  claims/s at p99 53 ms with 32 concurrent claimers against a 10-connection pool;
  a real target needs a real workload estimate.
- **Cross-tenant isolation** is asserted for executions and at the RLS layer.
  It is not asserted for every table; the RLS test checks that the policy
  *exists*, not that each query respects it.
- **Retry backoff** timing is not asserted, only that a retry is scheduled and
  that attempt history survives.
- **Workflows** are covered by their own suites rather than by this matrix, and
  the matrix does not re-assert them.