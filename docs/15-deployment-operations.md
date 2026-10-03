# 15. Deployment and Operations

## 15.1 Deployment modes

V1:
- local development;
- Docker Compose;
- single binary;
- Kubernetes.

## 15.2 Local development

One command SHOULD start:
- PostgreSQL;
- Forge API/scheduler;
- worker;
- web UI.

Provide seeded development data.

## 15.3 Containers

Images MUST:
- run as non-root where possible;
- use minimal base;
- expose only required ports;
- define health checks;
- pin base image digest for release builds where practical.

## 15.4 Kubernetes

Provide:
- Deployment for API.
- Deployment for scheduler.
- Deployment for workers.
- Service.
- ConfigMap.
- Secret references.
- PodDisruptionBudget where appropriate.
- readiness/liveness probes.
- resource requests/limits.
- NetworkPolicy examples.

## 15.5 Configuration

Configuration precedence:
1. explicit startup arguments;
2. environment variables;
3. config file;
4. documented defaults.

Do not silently merge conflicting configuration values.

## 15.6 Database migrations

Before upgrade:
- verify backup;
- verify migration compatibility;
- run migration;
- verify health.

Never automatically downgrade schema.

## 15.7 Backup

Back up:
- database;
- configuration;
- required secret-management metadata.

Logs/artifacts follow their own retention/backup policy.

## 15.8 Restore

Document:
1. provision PostgreSQL;
2. restore backup;
3. run verification;
4. start API in read-only/maintenance mode if needed;
5. validate scheduler state;
6. resume execution.

Restore drills are mandatory before production declaration.

## 15.9 Disaster recovery

Document:
- primary failure;
- database failure;
- worker fleet failure;
- API failure;
- region failure if multi-region is deployed.

V1 is single-region unless deployment documentation explicitly adds multi-region.

## 15.10 Graceful shutdown

API:
- stop accepting new requests;
- finish in-flight requests where possible.

Scheduler:
- stop claiming new schedules;
- finish current state transition.

Worker:
- stop accepting new jobs;
- finish current jobs within grace period;
- renew lease until termination or relinquish safely.

## 15.11 Operational runbooks

Must exist for:
- database unavailable;
- queue stuck;
- scheduler stuck;
- worker storm;
- high failure rate;
- high retry rate;
- outbox backlog;
- disk full;
- certificate expiry;
- credential compromise;
- bad deployment;
- migration failure.
