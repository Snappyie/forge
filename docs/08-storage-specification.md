# 8. Storage Specification

## 8.1 Database

PostgreSQL is the V1 authoritative datastore.

All schema changes use migrations.

Production migrations MUST be forward-only.

## 8.2 Core tables

Required:
- tenants
- users
- user_identities (for SSO/OIDC linking)
- roles
- user_roles
- api_keys
- jobs
- job_versions
- schedules
- workflows
- workflow_versions
- workflow_nodes
- workflow_edges
- manual_approvals
- queues
- executions
- execution_attempts
- workers
- worker_leases
- integration_configurations
- outbox_events
- audit_events
- idempotency_keys

Optional:
- execution_logs
- execution_artifacts
- notifications
- notification_deliveries
- worker_metrics

## 8.3 Tenant isolation

Every tenant-scoped table MUST include `tenant_id`.

Every unique index MUST consider tenant scope unless globally unique.

Application repositories MUST require tenant context.

Tests MUST include cross-tenant access attempts.

## 8.4 Transaction rules

Use transactions for:
- creating an execution and associated queue record;
- terminal execution state changes and concurrency slot release;
- workflow node completion and downstream activation;
- audit/outbox creation where atomicity is required;
- idempotency record creation and operation reservation.

## 8.5 Locking

Use row-level locks for:
- claiming due schedules;
- claiming queue items;
- updating leases;
- concurrency counters where necessary.

Use `SKIP LOCKED` where appropriate to allow horizontal scheduler/worker scaling.

Lock acquisition order MUST be documented to avoid deadlocks.

## 8.6 Index requirements

At minimum:
- jobs by tenant/key.
- schedules by enabled/next_run_at.
- executions by tenant/status/created_at.
- executions by job/created_at.
- executions by queue/status.
- attempts by execution/attempt_number.
- workers by tenant/status.
- leases by expiry.
- outbox by published_at/created_at.
- audit by tenant/timestamp.

Indexes MUST be validated against actual query plans.

## 8.7 Retention

Cleanup MUST be batched.

Cleanup MUST NOT lock the system for long periods.

Execution retention MUST not delete active execution records.

Audit retention SHOULD be longer than execution retention by default.

## 8.8 Encryption

Database encryption at rest is deployment responsibility.

Sensitive fields SHOULD be encrypted at application layer where appropriate.

Passwords/API tokens MUST be stored using secure one-way hashing or secure token hashing—not reversible plaintext storage.

## 8.9 Schema migration compatibility

For zero/minimal downtime:
1. Add new nullable/compatible schema.
2. Deploy code that can read old/new.
3. Backfill.
4. Enforce new constraint.
5. Remove old schema only in a later release.

## 8.10 Time

Use UTC `timestamptz`.

Never store local wall-clock time as the only scheduling representation.
