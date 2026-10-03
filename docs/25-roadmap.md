# 25. Roadmap

## Release 0.x — Engineering preview

Goal:
- usable locally;
- domain and scheduler stable;
- APIs evolving.

Features:
- jobs
- schedules
- execution
- worker
- retries
- PostgreSQL
- CLI
- basic UI

## Release 0.x+1 — Workflow preview

- DAG workflows
- visual editor
- dependency conditions
- workflow retries
- workflow cancellation

## Release 1.0 — Production-capable single-region

Required:
- stable API;
- tenant isolation;
- RBAC;
- audit;
- observability;
- backup/restore;
- Kubernetes;
- migration strategy;
- security baseline;
- acceptance catalog passing;
- documented SLOs.

## 1.x

Potential:
- event triggers;
- calendar schedules;
- richer executors;
- notification integrations;
- pluggable secret backends;
- broker adapters;
- advanced queue fairness.

## 2.x candidates

- multi-region;
- global scheduling;
- workflow-as-code;
- WASM;
- advanced policy engine;
- dynamic fan-out;
- human approvals;
- distributed artifact management.

No roadmap item is considered committed until it has an RFC and acceptance criteria.
