# Requirements Traceability Matrix

This matrix is intentionally high-level. Every detailed requirement in the
numbered documents should map to implementation issues and acceptance tests.

Acceptance-test IDs (AT-*) refer to `20-acceptance-test-catalog.md`. Tests are
named after the ID they satisfy, so a search for e.g. `AT-SCH-008` finds both
the specification and its implementation.

| Domain | Primary spec | Main tests | Implementation |
|---|---|---|---|
| Jobs | 01, 02 | AT-STATE, AT-API | `forge-domain::job`, `forge-storage` (Phase 3) |
| Scheduling | 01, 09 | AT-SCH | `forge-domain::schedule`, `forge-scheduler` (Phase 4) |
| Execution | 01, 10 | AT-STATE, AT-REC | `forge-domain::execution`, `forge-executor` (Phase 5) |
| Workers | 01, 10 | AT-WKR | `forge-executor` (Phase 5) |
| Retries | 02, 10 | AT-RETRY | `forge-domain::policy` — AT-RETRY-001..007 implemented |
| Concurrency | 01, 02, 10 | AT-CON | `forge-domain::policy` (limits), `forge-executor` (slots, Phase 5) |
| Workflows | 01, 02, 10 | AT-WF | `forge-domain::workflow` — AT-WF-001, AT-WF-002 implemented |
| Multi-tenancy | 01, 08, 11 | AT-TEN | `forge-storage` repositories (Phase 3), `forge-api` (Phase 8) |
| API | 05 | AT-API | `forge-api` (Phase 8) |
| CLI | 16 | CLI integration tests | `forge-cli` (Phase 9) |
| UI | 07 | UI E2E | `forge-web` (Phase 12) |
| Security | 11 | AT-SEC | `forge-auth`, `forge-api` (Phase 7) |
| Observability | 12 | AT-OBS | `forge-observability` (Phase 6) |
| Storage | 08 | repository tests | `forge-storage` migrations (Phase 1), repositories (Phase 3) |
| Deployment | 15 | upgrade/restore tests | Dockerfile/compose/k8s (Phase 13) |
| Open source | 18 | repository governance checks | CI workflow (Phase 13) |

## Conformance status by acceptance family

| Family | Total | Implemented | Phase |
|---|---|---|---|
| AT-STATE | 4 | 3 | 2 |
| AT-RETRY | 7 | 7 | 2 |
| AT-WF (validation) | 2 | 2 | 2 |
| AT-SCH (catch-up) | 1 | 1 | 2 |
| AT-SCH (cron/timezone/DST) | 9 | 0 | 4 |
| AT-SCH (scheduler concurrency) | 1 | 0 | 4 |
| AT-CON | 5 | 0 | 5 |
| AT-WKR | 5 | 0 | 5 |
| AT-TEN | 4 | 0 | 3 |
| AT-API | 7 | 0 | 8 |
| AT-SEC | 6 | 0 | 7 |
| AT-REC | 5 | 0 | 10, 11 |
| AT-OBS | 5 | 0 | 6 |

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