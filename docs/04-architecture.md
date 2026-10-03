# 4. Architecture

## 4.1 Architectural goals

- Domain logic independent of infrastructure.
- Replaceable storage.
- Replaceable transport.
- Testable scheduler.
- Explicit concurrency boundaries.
- Horizontal worker scaling.
- API backward compatibility.
- Operational transparency.

## 4.2 Target topology

```text
                     Web UI
                       |
                       v
                 API / Auth Layer
                       |
          +------------+-------------+
          |                          |
          v                          v
     Application Services       Query Services
          |                          |
          +------------+-------------+
                       |
                       v
                  Domain Core
                       |
             +---------+---------+
             |                   |
             v                   v
        Scheduler Core      Workflow Engine
             |                   |
             +---------+---------+
                       |
                       v
                 Queue/Dispatch
                       |
            +----------+----------+
            |          |          |
            v          v          v
         Worker      Worker      Worker
            |          |          |
            +----------+----------+
                       |
                       v
                   PostgreSQL
```

## 4.3 Rust workspace

Recommended crates:

### forge-domain
Pure domain types, state machines, validation, policies.

### forge-scheduler
Scheduling decisions, due schedules, queue selection.

### forge-executor
Worker protocol, leases, execution lifecycle.

### forge-workflow
DAG validation and workflow state progression.

### forge-storage
PostgreSQL repositories, migrations, transactions.

### forge-api
HTTP handlers, DTOs, OpenAPI, authentication middleware.

### forge-events
Domain events, outbox, event serialization.

### forge-auth
Authentication, RBAC, API key validation.

### forge-observability
Tracing, metrics, correlation.

### forge-cli
CLI client.

### forge-server
Binary composing all server components.

## 4.4 Dependency rule

Domain MUST NOT depend on:
- HTTP framework.
- PostgreSQL client.
- UI.
- concrete message broker.
- environment variables.

Infrastructure depends inward toward domain/application contracts.

## 4.5 Execution architecture

The scheduler is logically responsible for:
- Finding due schedules.
- Creating executions.
- Queueing work.
- Enforcing policy.

Workers are responsible for:
- Acquiring/receiving work.
- Running it.
- Heartbeats.
- Completion reporting.

The server is authoritative for:
- State.
- Lease ownership.
- Tenant isolation.
- Execution identity.

## 4.6 Persistence model

PostgreSQL is the source of truth for V1.

A future broker MAY be introduced, but a broker MUST NOT become the only source of truth for execution state.

## 4.7 Queue model

V1 may implement durable queues using PostgreSQL row locking and `SKIP LOCKED`.

The abstraction MUST allow future Redis/NATS/Kafka/etc. adapters.

The semantics—not the implementation—are normative:
- no acknowledged work disappears;
- leases expire;
- duplicate delivery is possible;
- handlers must be idempotent.

## 4.8 Outbox

External events MUST use an outbox pattern where atomicity between database state and event publication matters.

Outbox records:
- ID
- event type
- aggregate type
- aggregate ID
- payload
- created_at
- published_at
- attempt count
- last error

## 4.9 Clock

All persisted timestamps are UTC.

Schedule evaluation uses an explicit IANA timezone.

The scheduler MUST use an injectable clock in tests.

## 4.10 Randomness

Jitter MUST use a cryptographically appropriate or high-quality random source as appropriate for the use case. Randomness MUST be injectable for deterministic tests.

## 4.11 Unsafe Rust

`unsafe` is forbidden by default.

Any `unsafe` requires:
- written justification,
- safety invariant,
- review by a maintainer,
- test coverage,
- ADR.

## 4.12 Recommended technology baseline

- Rust stable toolchain.
- Tokio runtime.
- Axum HTTP API.
- Serde serialization.
- PostgreSQL.
- SQLx or an equivalent compile-time-conscious SQL layer.
- tracing ecosystem.
- OpenTelemetry-compatible tracing where enabled.
- OpenAPI.
- Containerized deployment.
- **Web UI**: Next.js (React Framework).
- **Web UI Styling/Components**: Tailwind CSS and shadcn/ui.

Exact dependency versions MUST be pinned through Cargo.lock and dependency policy rather than hard-coded in this specification.
