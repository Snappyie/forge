# Requirements Traceability Matrix

This matrix is intentionally high-level. Every detailed requirement in the
numbered documents should map to implementation issues and acceptance tests.

Acceptance-test IDs (AT-*) refer to `20-acceptance-test-catalog.md`. Tests are
named after the ID they satisfy, so a search for e.g. `AT-SCH-008` finds both
the specification and its implementation.

| Domain | Primary spec | Main tests | Implementation |
|---|---|---|---|
| Jobs | 01, 02 | AT-STATE, AT-API | `forge-domain::job`, `forge-storage::jobs` |
| Scheduling | 01, 09 | AT-SCH | `forge-scheduler` (engine, cron, misfire, clock) |
| Execution | 01, 10 | AT-STATE, AT-REC | `forge-domain::execution`, `forge-storage::jobs`; worker runtime Phase 5 |
| Workers | 01, 10 | AT-WKR | `forge-storage::scheduling` (register/heartbeat/drain/revoke) |
| Retries | 02, 10 | AT-RETRY | `forge-domain::policy` — AT-RETRY-001..007 implemented |
| Concurrency | 01, 02, 10 | AT-CON | `forge-domain::policy` (limits), `forge-storage` (admission + counting) |
| Workflows | 01, 02, 10 | AT-WF | `forge-domain::workflow` — AT-WF-001, AT-WF-002 implemented |
| Multi-tenancy | 01, 08, 11 | AT-TEN | `forge-storage` — AT-TEN-001, AT-TEN-004 implemented |
| API | 05 | AT-API | `forge-api` (Phase 8); AT-API-007 at the storage layer |
| CLI | 16 | CLI integration tests | `forge-cli` (Phase 9) |
| UI | 07 | UI E2E | `forge-web` (Phase 12) |
| Security | 11 | AT-SEC | `forge-auth` (Phase 7); revoked-worker rejection in storage |
| Observability | 12 | AT-OBS | `forge-observability` (Phase 6) |
| Storage | 08 | repository tests | `forge-storage` migrations + repositories |
| Deployment | 15 | upgrade/restore tests | Dockerfile/compose/k8s (Phase 13) |
| Open source | 18 | repository governance checks | CI workflow (Phase 13) |

## Conformance status by acceptance family

Every acceptance-test ID in `20-acceptance-test-catalog.md` has at least one
test that names it, so searching for an ID finds both the specification and its
implementation.

| Family | Total | Implemented | Where |
|---|---|---|---|
| AT-SCH | 10 | 10 | `forge-scheduler` unit + integration |
| AT-STATE | 4 | 4 | `forge-domain::execution` |
| AT-RETRY | 7 | 7 | `forge-domain::policy` |
| AT-WKR | 5 | 5 | `forge-storage`, `forge-executor` |
| AT-CON | 5 | 5 | `forge-storage/tests/concurrency_acceptance.rs` |
| AT-WF | 8 | 8 | `forge-domain::workflow`, `forge-executor::workflow_engine` |
| AT-TEN | 4 | 4 | `forge-storage`, `forge-api/tests/api_integration.rs` |
| AT-API | 7 | 7 | `forge-api` unit + integration |
| AT-SEC | 6 | 6 | `forge-auth` SSRF/RBAC, rate and sandbox policy |
| AT-REC | 5 | 5 | `forge-events/tests/recovery_acceptance.rs` |
| AT-OBS | 5 | 5 | `forge-observability`, `forge-executor` |

A release cannot be called production-capable while mandatory acceptance tests
are failing (20-acceptance-test-catalog.md).

## Specification amendments

Decisions taken where the specification deliberately defers are recorded as ADRs
in `22-architecture-decisions.md`:

- ADR-0011 — `ABANDONED` is intermediate, not terminal.
- ADR-0012 — `FAILED`/`TIMED_OUT` permit retry and dead-letter exits.
- ADR-0013 — five-field cron dialect.
- ADR-0014 — ambiguous DST times fire once, at the first occurrence.
- ADR-0015 — a recurring schedule's timezone is required.
- ADR-0016 — cancellation is cooperative only.
- ADR-0017 — runtime SQL queries instead of compile-time macros.
- ADR-0018 — status vocabularies use CHECK constraints, not native enums.

## Parity evaluation

A detailed comparative assessment against Apache Airflow and PowerJob is maintained in
[PowerJob + Airflow Parity Matrix](file:///Users/neel/Downloads/forge-specification/docs/powerjob-airflow-parity.md).

Complete system architecture and sequence diagrams for all core flows are detailed in
[Architecture & Complete Execution Flows](file:///Users/neel/Downloads/forge-specification/docs/architecture-and-flows.md).