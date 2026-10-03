# 5. API Specification

## 5.1 API principles

Base path:
`/api/v1`

Content type:
`application/json`

Authentication:
`Authorization: Bearer <token>`

Request ID:
`X-Request-ID` optional from client; server generates one when absent.

Idempotency:
`Idempotency-Key` for supported mutation endpoints.

## 5.2 Standard success envelope

For single resources:

```json
{
  "data": {},
  "request_id": "..."
}
```

For lists:

```json
{
  "data": [],
  "page": {
    "next_cursor": "...",
    "has_more": true
  },
  "request_id": "..."
}
```

## 5.3 Standard error

```json
{
  "error": {
    "code": "VALIDATION_ERROR",
    "message": "Human-readable safe message",
    "details": [],
    "request_id": "..."
  }
}
```

Production responses MUST NOT include stack traces.

## 5.4 Jobs

### POST /jobs
Create a job.

### GET /jobs
List jobs.

Filters:
- status
- queue
- owner
- label
- created_after
- created_before

### GET /jobs/{job_id}
Get job.

### PATCH /jobs/{job_id}
Update metadata.

### DELETE /jobs/{job_id}
Soft-delete/archive job.

### POST /jobs/{job_id}/versions
Create version.

### GET /jobs/{job_id}/versions
List versions.

### POST /jobs/{job_id}/versions/{version_id}/publish
Publish version.

### POST /jobs/{job_id}/trigger
Create manual execution.

## 5.5 Schedules

### POST /schedules
Create.

### GET /schedules
List.

### GET /schedules/{id}
Get.

### PATCH /schedules/{id}
Modify.

### POST /schedules/{id}/pause
Pause.

### POST /schedules/{id}/resume
Resume.

### POST /schedules/{id}/preview
Return upcoming occurrences.

## 5.6 Executions

### GET /executions
Filters:
- job
- workflow
- status
- worker
- queue
- time range
- correlation ID

### GET /executions/{id}
Execution detail.

### POST /executions/{id}/cancel
Request cancellation.

### POST /executions/{id}/retry
Operator retry.

### POST /executions/{id}/dead-letter
Dead-letter.

### GET /executions/{id}/attempts
Attempts.

### GET /executions/{id}/logs
Logs.

## 5.7 Workflows

### POST /workflows
Create.

### GET /workflows
List.

### GET /workflows/{id}
Get.

### POST /workflows/{id}/versions
Create version.

### POST /workflows/{id}/versions/{version_id}/validate
Validate DAG.

### POST /workflows/{id}/versions/{version_id}/publish
Publish.

### POST /workflows/{id}/trigger
Trigger.

### POST /workflows/executions/{execution_id}/nodes/{node_id}/approve
Approve a human-in-the-loop manual approval node.

### POST /workflows/executions/{execution_id}/nodes/{node_id}/reject
Reject a human-in-the-loop manual approval node.

## 5.8 Workers

### POST /workers/register
Register/authenticate worker.

### POST /workers/{id}/heartbeat
Heartbeat.

### GET /workers
List.

### GET /workers/{id}
Get.

### POST /workers/{id}/drain
Drain.

### POST /workers/{id}/revoke
Revoke.

## 5.9 Queues

### GET /queues
List.

### POST /queues
Create.

### GET /queues/{id}
Get.

### PATCH /queues/{id}
Modify.

### POST /queues/{id}/pause
Pause.

### POST /queues/{id}/resume
Resume.

## 5.10 Users/RBAC

### GET /users
### POST /users
### PATCH /users/{id}
### POST /users/{id}/disable

Roles:
- OWNER
- ADMIN
- OPERATOR
- DEVELOPER
- AUDITOR
- VIEWER

Permissions MUST be granular internally.

## 5.11 API keys

### POST /api-keys
Create.

### GET /api-keys
List metadata.

### POST /api-keys/{id}/revoke
Revoke.

Raw key material MUST be shown only once.

## 5.12 Audit

### GET /audit-events

Filters:
- actor
- action
- resource
- time
- result

Audit data MUST be read-only through the public API.

## 5.13 Integrations

### GET /integrations
List configured third-party integrations (AWS, Datadog, etc).

### POST /integrations
Configure a new integration.

### PATCH /integrations/{id}
Update integration configuration.

### DELETE /integrations/{id}
Remove integration.

## 5.15 Auth & SSO

### POST /auth/login
Local login.

### POST /auth/sso/{provider}
Initiate SSO flow.

### POST /auth/refresh
Refresh JWT token.

## 5.16 Health

### GET /health/live
Process is alive.

### GET /health/ready
Dependencies are usable.

### GET /health
Detailed health for authorized operators.

## 5.14 Pagination

Use cursor pagination for large collections.

Cursor must encode:
- ordering fields
- last resource identity
- query version if needed

Cursors are opaque to clients.

## 5.15 Optimistic concurrency

Mutable resources SHOULD support ETags or explicit version numbers.

A stale update MUST return `409 CONFLICT`.

## 5.16 API compatibility

Breaking API changes require a new major API version or documented migration strategy.

Adding optional response fields is compatible.

Removing fields is breaking.

Changing enum meanings is breaking.

Changing default behavior in a way that changes execution semantics is breaking.
