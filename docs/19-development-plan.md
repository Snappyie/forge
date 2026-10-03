# 19. Development Plan

## Phase 0 — Repository foundation

Deliver:
- workspace;
- CI;
- licenses;
- code quality;
- documentation structure;
- local PostgreSQL;
- development scripts.

Exit:
- clean build;
- CI passes;
- contributor can run tests from clean checkout.

## Phase 1 — Domain core

Implement:
- IDs;
- jobs;
- versions;
- statuses;
- state machines;
- retry policy;
- concurrency policy;
- resource requirements.

Exit:
- domain tests complete;
- no infrastructure dependency in domain.

## Phase 2 — Persistent scheduler

Implement:
- PostgreSQL;
- migrations;
- schedules;
- cron;
- one-time schedules;
- timezone;
- next-run;
- due schedule claiming.

Exit:
- restart-safe scheduler;
- duplicate scheduler test passes.

## Phase 3 — Execution engine

Implement:
- queue;
- worker registration;
- lease;
- heartbeat;
- completion;
- timeout;
- cancellation.

Exit:
- worker crash recovery passes.

## Phase 4 — HTTP API

Implement all V1 endpoints.

Exit:
- OpenAPI complete;
- API integration tests complete.

## Phase 5 — CLI

Implement core commands.

Exit:
- automation workflows can be performed without UI.

## Phase 6 — Web UI

Implement:
- dashboard;
- jobs;
- schedules;
- executions;
- workers;
- queues;
- audit;
- administration.

Exit:
- primary user journeys work end-to-end.

## Phase 7 — Workflows

Implement:
- DAGs;
- dependency conditions;
- fan-out/fan-in;
- manual human-in-the-loop approvals;
- delayed execution nodes;
- conditional (if/else) nodes;
- dynamic fan-out (map) over lists.

Exit:
- workflow acceptance catalog passes.

## Phase 8 — Security hardening

Implement:
- RBAC;
- tenant isolation;
- local email/password authentication;
- SSO/OIDC integration (SAML 2.0, GitHub, Google);
- multi-factor authentication (MFA);
- JWT rotating refresh tokens for session management;
- API key lifecycle;
- secrets references;
- rate limits;
- SSRF protections;
- security scanning.

Exit:
- security test suite passes.

## Phase 9 — Observability

Implement:
- metrics;
- logs;
- tracing;
- audit;
- dispatch explanations.

Exit:
- operators can diagnose common failures without database access.

## Phase 10 — Production hardening

Implement:
- backup/restore;
- migrations;
- Kubernetes;
- upgrade tests;
- failure injection;
- load tests.

Exit:
- production-readiness checklist passes.

## Phase 11 — Payment reference workload

Implement examples:
- settlement workflow;
- reconciliation workflow;
- retryable gateway request;
- report generation.

No real money or provider credentials.

Exit:
- examples are deterministic and safe.

## Phase 12 — Integrations

Implement out-of-the-box ecosystem connectors:
- Kubernetes (native Job runner);
- AWS ECS/Fargate & GCP Cloud Run executors;
- Datadog & OpenTelemetry observability sinks;
- Slack, PagerDuty, and Email alerting hooks.

Exit:
- Integration acceptance tests pass for all supported platforms.

## Phase 13 — Advanced roadmap

Potential:
- event triggers;
- calendar schedules;
- pluggable brokers;
- multi-region;
- policy engine;
- workflow-as-code SDK;
- WASM executor;
- advanced scheduling strategies.

Each requires its own RFC.
