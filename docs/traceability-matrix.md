# Requirements Traceability Matrix

This matrix is intentionally high-level. Every detailed requirement in the
numbered documents should map to implementation issues and acceptance tests.

| Domain | Primary spec | Main tests |
|---|---|---|
| Jobs | 01, 02 | AT-STATE, AT-API |
| Scheduling | 01, 09 | AT-SCH |
| Execution | 01, 10 | AT-STATE, AT-REC |
| Workers | 01, 10 | AT-WKR |
| Retries | 02, 10 | AT-RETRY |
| Concurrency | 01, 02, 10 | AT-CON |
| Workflows | 01, 02, 10 | AT-WF |
| Multi-tenancy | 01, 08, 11 | AT-TEN |
| API | 05 | AT-API |
| CLI | 16 | CLI integration tests |
| UI | 07 | UI E2E |
| Security | 11 | AT-SEC |
| Observability | 12 | AT-OBS |
| Storage | 08 | repository tests |
| Deployment | 15 | upgrade/restore tests |
| Open source | 18 | repository governance checks |
