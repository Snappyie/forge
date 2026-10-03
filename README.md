# Forge — Distributed Job Orchestration Platform

Forge is an open-source, production-oriented distributed job scheduling and workflow orchestration platform written primarily in Rust.

Forge is designed as both:
1. A serious systems-engineering project for learning Rust, asynchronous programming, concurrency, distributed systems, and production operations.
2. A usable self-hostable scheduler for recurring jobs, one-off jobs, workflows, retries, dependencies, resource-aware execution, observability, and auditability.
3. A payment-operations reference workload, without embedding payment-provider-specific behavior into the core product.

## Documentation is the product specification

The `docs/` directory is normative. If implementation and documentation disagree, treat the documentation as the intended behavior and update the implementation or explicitly amend the specification through an ADR/RFC.

Start here:

1. `docs/01-product-requirements.md` — product scope and complete requirements.
2. `docs/02-domain-model.md` — entities, state machines, invariants, and business rules.
3. `docs/03-use-cases.md` — user and system workflows.
4. `docs/04-architecture.md` — target architecture and component boundaries.
5. `docs/05-api-specification.md` — HTTP API contract.
6. `docs/06-event-and-queue-contracts.md` — internal/event contracts.
7. `docs/07-ui-specification.md` — complete web-console requirements.
8. `docs/08-storage-specification.md` — PostgreSQL schema and persistence rules.
9. `docs/09-scheduling-engine.md` — scheduling semantics.
10. `docs/10-execution-engine.md` — workers, leases, retries, cancellation, recovery.
11. `docs/11-security.md` — threat model and security requirements.
12. `docs/12-observability.md` — logs, metrics, traces, audit events.
13. `docs/13-non-functional-requirements.md` — performance, availability, durability, compatibility.
14. `docs/14-testing-strategy.md` — unit through failure-injection testing.
15. `docs/15-deployment-operations.md` — local, Docker, Kubernetes, upgrades, backup, recovery.
16. `docs/16-cli-specification.md` — CLI behavior.
17. `docs/17-configuration.md` — configuration and environment variables.
18. `docs/18-open-source-governance.md` — licensing, contribution, releases, security disclosure.
19. `docs/19-development-plan.md` — implementation phases and acceptance gates.
20. `docs/20-acceptance-test-catalog.md` — executable product acceptance criteria.
21. `docs/21-payment-operations-reference.md` — canonical payment-domain examples.
22. `docs/22-architecture-decisions.md` — initial ADR register.
23. `docs/23-documentation-standard.md` — rules for keeping the specification complete.
24. `docs/24-terminology.md` — canonical vocabulary.
25. `docs/25-roadmap.md` — MVP through advanced capabilities.

Templates:
- `docs/templates/adr-template.md`
- `docs/templates/rfc-template.md`
- `docs/templates/incident-template.md`
- `docs/templates/job-definition-example.yaml`
- `docs/templates/workflow-definition-example.yaml`

## Normative language

- **MUST** = mandatory for conformance.
- **MUST NOT** = prohibited.
- **SHOULD** = recommended unless there is a documented reason not to.
- **SHOULD NOT** = generally avoided unless justified.
- **MAY** = optional.
- **V1** = required for the first production-capable release.
- **Future** = explicitly out of the initial implementation.

## Design principles

- Correctness before throughput.
- Explicit state transitions.
- Idempotency by design.
- At-least-once infrastructure with application-level idempotency.
- No silent job loss.
- No silent duplicate execution.
- Every execution is explainable.
- Every security-sensitive action is auditable.
- APIs are versioned and backward-compatible.
- Core domain logic remains independent from HTTP, PostgreSQL, and UI.
- Safe Rust by default; `unsafe` requires an ADR and explicit justification.
- Open-source contributors must be able to run the complete development stack locally.

## Suggested repository layout

```text
forge/
├── crates/
│   ├── forge-domain/
│   ├── forge-scheduler/
│   ├── forge-executor/
│   ├── forge-storage/
│   ├── forge-api/
│   ├── forge-events/
│   ├── forge-auth/
│   ├── forge-observability/
│   ├── forge-cli/
│   └── forge-server/
├── web/
├── migrations/
├── deploy/
├── docs/
├── examples/
├── tests/
└── .github/
```

The exact workspace split is an implementation decision, but the domain/application/infrastructure boundaries in the architecture specification are mandatory.
