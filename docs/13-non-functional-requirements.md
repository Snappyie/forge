# 13. Non-Functional Requirements

## 13.1 Correctness

Correct state transitions take priority over performance.

All state machine transitions MUST have tests.

## 13.2 Performance targets

V1 reference target on representative hardware:
- API p95 read latency < 200ms excluding database overload.
- API p95 simple mutation < 300ms.
- Scheduler decision latency < 1s under normal reference load.
- Queue dispatch should support at least 1,000 executions/minute in the reference environment.
- The architecture MUST permit horizontal workers.

These are engineering targets, not universal guarantees.

Benchmarks MUST publish hardware, dataset, configuration, and methodology.

## 13.3 Scalability

The system SHOULD scale independently:
- API instances;
- scheduler instances;
- workers.

Database is the primary scaling constraint in V1.

## 13.4 Availability

Stateless API instances SHOULD be horizontally scalable.

Scheduler leaderless coordination SHOULD permit multiple instances.

Worker loss MUST not cause permanent job loss when recovery policy permits retry.

## 13.5 Durability

Persisted execution state MUST survive process restart.

Backups MUST be supported.

Recovery procedures MUST be documented and tested.

## 13.6 Compatibility

Supported:
- Linux server.
- Docker/OCI.
- Kubernetes.
- PostgreSQL versions declared by each release.

Browser support should cover current major versions of Chrome, Firefox, Safari, and Edge.

## 13.7 Accessibility

Target WCAG 2.2 AA for the web UI.

Keyboard operation is mandatory for primary workflows.

## 13.8 Internationalization

V1:
- English UI.
- UTC/system timestamps with explicit timezone display.
- Unicode-safe names.

Future:
- translated UI.
- locale-aware formatting.

## 13.9 Maintainability

Public Rust APIs MUST be documented.

Rustdoc guidance recommends documenting public items and including copyable examples where useful. urlRustdoc documentation guidancehttps://doc.rust-lang.org/stable/rustdoc/how-to-write-documentation.html

## 13.10 Code quality

CI MUST run:
- cargo fmt --check
- cargo clippy
- cargo test
- documentation build
- dependency audit
- license checks
- integration tests

Cargo's official documentation defines workspaces, dependency management, tests, CI, publishing, and related project workflows. urlCargo Bookhttps://doc.rust-lang.org/cargo/

## 13.11 Recovery point/objectives

Deployment documentation MUST define:
- RPO.
- RTO.
- backup frequency.
- restore verification frequency.

Recommended initial reference:
- RPO <= 15 minutes.
- RTO <= 60 minutes.

## 13.12 Resource safety

No unbounded:
- queue fetch;
- log buffering;
- API response;
- retry schedule;
- workflow fan-out;
- database query result.

All must have configured or hard safety caps.
