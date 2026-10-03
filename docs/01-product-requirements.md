# 1. Product Requirements

## 1.1 Product definition

Forge is a self-hostable distributed job orchestration platform. It schedules and executes jobs, manages workflow dependencies, coordinates workers, records execution history, and exposes APIs, CLI commands, and a web console.

Forge is not initially intended to be:
- A general-purpose ETL platform.
- A Kubernetes replacement.
- A payment processor.
- A message broker.
- A serverless compute platform.
- A durable workflow language with arbitrary code replay semantics.

Forge can integrate with those systems.

## 1.2 Personas

### Platform administrator
Manages installation, users, tenants, workers, credentials, retention, backups, and system configuration.

### Scheduler operator
Monitors queues, workers, executions, failures, schedules, and incidents.

### Developer
Creates jobs/workflows, tests them, inspects logs, and integrates Forge through API/CLI.

### Application service
Calls the API to create, trigger, cancel, or inspect executions.

### Worker operator
Deploys and monitors worker processes.

### Auditor
Reads immutable audit events and execution history.

## 1.3 Core objects

Forge MUST support:
- Organization/tenant.
- User.
- Role.
- API key.
- Job.
- Job version.
- Schedule.
- Workflow.
- Workflow version.
- Workflow node.
- Workflow edge/dependency.
- Execution.
- Task execution.
- Attempt.
- Worker.
- Worker lease.
- Queue.
- Retry policy.
- Concurrency policy.
- Resource requirement.
- Secret reference.
- Artifact/log reference.
- Notification destination.
- Audit event.
- Event/outbox record.

## 1.4 Job requirements

A job MUST have:
- Stable ID.
- Human-readable name.
- Description.
- Enabled/disabled state.
- Version.
- Execution type.
- Input schema or documented input contract.
- Timeout.
- Retry policy.
- Concurrency policy.
- Priority.
- Queue.
- Resource requirements.
- Ownership metadata.
- Created/updated timestamps.

Execution types for V1:
1. HTTP request.
2. Container command.
3. Worker-defined task handler.

The architecture MUST permit future types without changing the core scheduler.

## 1.5 Scheduling requirements

Forge MUST support:
- One-time execution.
- Fixed interval.
- Cron expressions.
- Time zones.
- Manual trigger.
- Pause/resume.
- Next-run calculation.
- Schedule history.
- Misfire policy.
- Catch-up policy.
- Schedule enable/disable.
- Explicit timezone.
- DST-safe behavior.

Future:
- Calendar schedules.
- Business-day schedules.
- Holiday calendars.
- Dependency-triggered schedules.
- Event-triggered schedules.

## 1.6 Execution requirements

Each execution MUST have:
- Unique execution ID.
- Job ID/version.
- Trigger source.
- Scheduled time.
- Actual enqueue time.
- Start time.
- End time.
- Status.
- Worker ID.
- Attempt count.
- Input reference.
- Output reference where configured.
- Error classification.
- Correlation ID.
- Trace ID where tracing is enabled.

Forge MUST distinguish:
- Queued.
- Dispatched.
- Running.
- Succeeded.
- Failed.
- Timed out.
- Cancel requested.
- Cancelled.
- Retry scheduled.
- Dead-lettered.
- Abandoned/recovered.

## 1.7 Reliability requirements

Forge MUST:
- Persist state before acknowledging durable operations.
- Avoid acknowledging a job as completed until completion is persisted.
- Use leases for worker ownership.
- Detect expired leases.
- Requeue recoverable work.
- Preserve execution history.
- Make duplicate delivery safe through execution identity and idempotency controls.
- Never delete execution history merely because a worker disappears.
- Make recovery behavior deterministic.

## 1.8 Workflow requirements

A workflow MUST support:
- Directed acyclic graph execution.
- Sequential dependencies.
- Parallel branches.
- Fan-out.
- Fan-in.
- Dependency success/failure policies.
- Per-node retry policy.
- Per-node timeout.
- Workflow-level timeout.
- Manual workflow execution.
- Workflow pause/cancel.
- Execution graph visualization.

Advanced capabilities MUST include:
- Human-in-the-loop (manual approval) nodes, halting execution until an authorized user approves/rejects.
- Delayed execution nodes, pausing execution for a duration or until a specified timestamp.
- Conditional nodes (if/else branching based on previous outputs).
- Dynamic fan-out (map) processing over lists.
- External webhook nodes.

V1 workflow nodes are jobs and the advanced capabilities mentioned above.

## 1.9 Queue requirements

Queues MUST support:
- Named queues.
- Priority.
- Maximum concurrency.
- Worker capability matching.
- Queue pause.
- Queue depth.
- Queue age.
- Queue metrics.
- Fairness policy.

The scheduler MUST avoid starvation under normal operation.

## 1.10 Worker requirements

Workers MUST:
- Register.
- Authenticate.
- Advertise capabilities.
- Send heartbeats.
- Receive work.
- Renew leases.
- Report state.
- Gracefully stop accepting new work.
- Complete or relinquish current work.
- Expose health status.
- Expose version.

Workers MUST NOT be trusted to declare successful completion without server-side persistence/validation.

## 1.11 Retry requirements

Retry policies MUST support:
- Maximum attempts.
- Fixed delay.
- Linear backoff.
- Exponential backoff.
- Maximum delay.
- Jitter.
- Retryable error classes.
- Non-retryable error classes.
- Retry-after hints.
- Dead-letter behavior.

## 1.12 Concurrency requirements

Support:
- Per-job max concurrent executions.
- Per-workflow max concurrent executions.
- Per-queue concurrency.
- Global concurrency.
- Per-tenant concurrency.

A concurrency slot MUST be released when execution reaches a terminal state or is explicitly recovered according to documented lease rules.

## 1.13 Resource requirements

Jobs MAY specify:
- CPU units.
- Memory.
- Worker capabilities.
- Architecture.
- OS.
- Labels.
- Region.
- Custom scheduling constraints.

Resource accounting MUST be conservative: Forge MUST NOT intentionally oversubscribe resources based on stale worker information.

## 1.14 Observability requirements

Provide:
- Structured logs.
- Metrics.
- Distributed traces.
- Execution timelines.
- Queue metrics.
- Worker metrics.
- Scheduler metrics.
- Audit events.
- Correlation IDs.
- Searchable failure information.

## 1.15 Security requirements

MUST support:
- Authentication (Local Email/Password, OAuth2/OIDC providers like Google, GitHub, Okta, SAML 2.0 for enterprise).
- Session Management (JWT tokens with rotating refresh tokens, session revocation).
- Multi-Factor Authentication (MFA/2FA via TOTP or WebAuthn).
- Authorization.
- Tenant isolation.
- RBAC.
- API key management.
- Secret references rather than plaintext secret duplication.
- TLS for production.
- Audit logging.
- Sensitive-field redaction.
- Rate limiting.
- Input validation.
- Security headers for the web UI.
- Dependency vulnerability scanning.

## 1.16 Multi-tenancy

Every tenant-scoped resource MUST contain an immutable tenant identifier.

Authorization MUST be evaluated before returning tenant-scoped resources.

Database queries MUST include tenant scope.

Cross-tenant resource references MUST be rejected.

## 1.17 Auditability

The following MUST generate audit events:
- Authentication changes.
- User/role changes.
- API key creation/revocation.
- Job creation/update/delete.
- Workflow changes.
- Schedule changes.
- Manual trigger.
- Cancellation.
- Retry policy changes.
- Worker registration/revocation.
- Secret reference changes.
- Configuration changes.
- Tenant changes.

Audit events MUST contain:
- Event ID.
- Timestamp.
- Actor.
- Tenant.
- Action.
- Resource.
- Resource ID.
- Result.
- Source IP where available.
- Request/correlation ID.
- Metadata without secrets.

## 1.18 Data retention

Retention MUST be configurable separately for:
- Execution records.
- Task attempts.
- Logs.
- Audit events.
- Metrics.
- Traces.

Deletion MUST be explicit, observable, and auditable.

## 1.19 API requirements

The API MUST:
- Be versioned.
- Return consistent errors.
- Use stable resource identifiers.
- Support pagination.
- Support filtering.
- Support sorting.
- Support idempotency for mutation endpoints where duplicate requests are dangerous.
- Provide OpenAPI.
- Include examples.

## 1.20 CLI requirements

The CLI MUST cover:
- Login/authentication.
- Context management.
- Jobs.
- Workflows.
- Executions.
- Workers.
- Queues.
- Secrets references.
- Configuration inspection.
- Logs.
- Health.
- Version.

## 1.21 Web console requirements

The UI MUST provide:
- Dashboard.
- Job list/detail/editor.
- Workflow list/detail/editor.
- Execution list/detail.
- Worker list/detail.
- Queue list/detail.
- Audit log.
- User/role management.
- API keys.
- System health.
- Search.
- Filters.
- Bulk actions where safe.
- Empty/loading/error states.
- Confirmation dialogs for destructive actions.
- Accessible keyboard navigation.
- Approval queues for human-in-the-loop workflows.

## 1.22 Third-Party Integrations

Forge MUST provide out-of-the-box support for ecosystem integrations:
- **Compute platforms**: Kubernetes (native Job runner), AWS ECS/Fargate, GCP Cloud Run.
- **Observability**: Datadog, Prometheus/Grafana, OpenTelemetry, AWS CloudWatch.
- **Alerting**: Slack, PagerDuty, Microsoft Teams, Email.
- **Auth**: Okta, Auth0, Microsoft Entra ID.

## 1.22 Product-wide invariants

1. No execution may belong to more than one tenant.
2. An execution has exactly one current status.
3. Terminal executions cannot transition back to active states.
4. A job version is immutable after publication.
5. A workflow version is immutable after publication.
6. A schedule references a specific published job/workflow version or an explicit "latest" policy; the effective version MUST be recorded on execution.
7. A worker lease has an owner and expiry.
8. Only a valid lease holder may renew its lease.
9. Expired leases may be recovered according to execution recovery policy.
10. Audit records are append-only from the application perspective.
11. Secret values MUST NOT appear in logs.
12. API error responses MUST NOT expose internal stack traces in production.
