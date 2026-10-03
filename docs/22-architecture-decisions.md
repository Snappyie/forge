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
