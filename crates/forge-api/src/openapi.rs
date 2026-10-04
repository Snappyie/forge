//! OpenAPI 3.1 description of the public API (spec 05, spec 13).
//!
//! The document is generated from one table so the route table and the
//! published contract cannot drift apart. Choices the specification left open —
//! the cron dialect, the DST policy, cursor parameter names — are documented
//! here explicitly, which is what spec 09.4 and 09.6 require.

use serde_json::{json, Value};

/// Builds the OpenAPI document for a server at `base_url`.
pub fn document(base_url: &str) -> Value {
    json!({
        "openapi": "3.1.0",
        "info": {
            "title": "Forge API",
            "version": "1.0.0",
            "description": "Distributed job orchestration platform.",
            "license": { "name": "MIT" },
        },
        "servers": [{ "url": base_url }],
        "components": {
            "securitySchemes": {
                "bearerAuth": {
                    "type": "http",
                    "scheme": "bearer",
                    "bearerFormat": "JWT",
                    "description": "Access token from POST /api/v1/auth/login.",
                }
            },
            "parameters": {
                "cursor": {
                    "name": "cursor",
                    "in": "query",
                    "required": false,
                    "description": "Opaque pagination cursor from a previous page's `page.next_cursor`. Clients must treat it as opaque.",
                    "schema": { "type": "string" }
                },
                "limit": {
                    "name": "limit",
                    "in": "query",
                    "required": false,
                    "description": "Page size. Defaults to 50 and is capped at 200.",
                    "schema": { "type": "integer", "minimum": 1, "maximum": 200, "default": 50 }
                },
                "sort": {
                    "name": "sort",
                    "in": "query",
                    "required": false,
                    "description": "Sort order, prefixed with `-` for descending, e.g. `-created_at`.",
                    "schema": { "type": "string" }
                },
                "idempotencyKey": {
                    "name": "Idempotency-Key",
                    "in": "header",
                    "required": false,
                    "description": "Makes the request idempotent. The same key with the same body replays the original response; the same key with a different body returns 409.",
                    "schema": { "type": "string", "maxLength": 255 }
                }
            },
            "schemas": {
                "Error": {
                    "type": "object",
                    "required": ["error"],
                    "properties": {
                        "error": {
                            "type": "object",
                            "required": ["code", "message", "request_id"],
                            "properties": {
                                "code": {
                                    "type": "string",
                                    "description": "Spec 02.14 error class.",
                                    "examples": [
                                        "VALIDATION_ERROR", "AUTHENTICATION_REQUIRED",
                                        "AUTHORIZATION_DENIED", "NOT_FOUND", "CONFLICT",
                                        "RATE_LIMITED", "DEPENDENCY_UNAVAILABLE",
                                        "INTERNAL_ERROR"
                                    ]
                                },
                                "message": { "type": "string" },
                                "details": {
                                    "type": "array",
                                    "items": {
                                        "type": "object",
                                        "properties": {
                                            "field": { "type": "string" },
                                            "message": { "type": "string" }
                                        }
                                    }
                                },
                                "request_id": { "type": "string" }
                            }
                        }
                    }
                },
                "Page": {
                    "type": "object",
                    "properties": {
                        "next_cursor": {
                            "type": ["string", "null"],
                            "description": "Null on the last page."
                        },
                        "has_more": { "type": "boolean" }
                    }
                }
            },
            "responses": {
                "Error400": { "description": "Validation failed", "content": { "application/json": { "schema": { "$ref": "#/components/schemas/Error" } } } },
                "Error401": { "description": "Authentication required", "content": { "application/json": { "schema": { "$ref": "#/components/schemas/Error" } } } },
                "Error403": { "description": "Authorization denied", "content": { "application/json": { "schema": { "$ref": "#/components/schemas/Error" } } } },
                "Error404": { "description": "Not found, or not visible to this tenant", "content": { "application/json": { "schema": { "$ref": "#/components/schemas/Error" } } } },
                "Error409": { "description": "Conflict", "content": { "application/json": { "schema": { "$ref": "#/components/schemas/Error" } } } },
                "Error429": { "description": "Rate limited", "content": { "application/json": { "schema": { "$ref": "#/components/schemas/Error" } } } },
                "Error500": { "description": "Internal error", "content": { "application/json": { "schema": { "$ref": "#/components/schemas/Error" } } } }
            }
        },
        "security": [{ "bearerAuth": [] }],
        "tags": [
            { "name": "jobs" }, { "name": "job-versions" }, { "name": "schedules" },
            { "name": "executions" }, { "name": "workflows" }, { "name": "workers" },
            { "name": "queues" }, { "name": "users" }, { "name": "api-keys" },
            { "name": "auth" }, { "name": "audit" }, { "name": "integrations" },
            { "name": "system" },
        ],
        "paths": paths(),
        // Choices the specification left to the implementation, published where
        // a client will actually read them (spec 09.4, 09.5, 09.6).
        "x-forge-scheduling": schedule_notes(),
    })
}

/// The cron dialect and DST behaviour, stated where a client will read them.
fn schedule_notes() -> Value {
    json!({
        "cronDialect": {
            "fields": 5,
            "order": ["minute", "hour", "day-of-month", "month", "day-of-week"],
            "description": "Standard five-field cron. Six- and seven-field expressions are rejected with 400.",
            "dayOfWeekNumbering": "Days are numbered from Sunday, where 0 is Sunday and 1 is Monday. This differs from the Unix convention; `1-5` therefore means Sunday through Thursday, not weekdays."
        },
        "timezones": {
            "required": true,
            "description": "Every recurring schedule must name an IANA timezone such as `Asia/Kolkata`. The server never falls back to the host's local time."
        },
        "daylightSaving": {
            "nonexistentLocalTime": "Shifted forward past the gap.",
            "ambiguousLocalTime": "Fires once, on the first (earlier) occurrence."
        },
        "misfirePolicy": {
            "values": ["SKIP", "FIRE_ONCE", "CATCH_UP"],
            "default": "FIRE_ONCE",
            "catchUpLimitDefault": 100,
            "catchUpLimitConfigurable": true
        }
    })
}

fn op(
    summary: &str,
    tag: &str,
    id: &str,
    permission: Option<&str>,
    parameters: Vec<Value>,
    request_body: Option<Value>,
    success_status: &str,
) -> Value {
    let mut operation = json!({
        "summary": summary,
        "operationId": id,
        "tags": [tag],
        "responses": {
            success_status: { "description": "Success", "content": { "application/json": { "schema": { "type": "object" } } } },
            "400": { "$ref": "#/components/responses/Error400" },
            "401": { "$ref": "#/components/responses/Error401" },
            "403": { "$ref": "#/components/responses/Error403" },
            "409": { "$ref": "#/components/responses/Error409" },
        }
    });

    if let Some(permission) = permission {
        operation["description"] = json!(format!("Requires the `{permission}` permission."));
    }
    if !parameters.is_empty() {
        operation["parameters"] = Value::Array(parameters);
    }
    if let Some(body) = request_body {
        operation["requestBody"] = body;
    }
    operation
}

fn paging() -> Vec<Value> {
    vec![
        json!({ "$ref": "#/components/parameters/cursor" }),
        json!({ "$ref": "#/components/parameters/limit" }),
        json!({ "$ref": "#/components/parameters/sort" }),
    ]
}

fn json_body(schema: Value) -> Value {
    json!({
        "required": true,
        "content": { "application/json": { "schema": schema } }
    })
}

fn id_param(name: &str) -> Value {
    json!({
        "name": name,
        "in": "path",
        "required": true,
        "schema": { "type": "string", "format": "uuid" }
    })
}

fn paths() -> Value {
    let mut paths = serde_json::Map::new();

    // --- jobs (spec 05 endpoints 1-5) ---
    paths.insert(
        "/jobs".into(),
        json!({
            "get": op("List jobs", "jobs", "listJobs", Some("jobs:read"), paging(), None, "200"),
            "post": op(
                "Create a job", "jobs", "createJob", Some("jobs:write"),
                vec![json!({ "$ref": "#/components/parameters/idempotencyKey" })],
                Some(json_body(json!({
                    "type": "object",
                    "required": ["name"],
                    "properties": {
                        "name": { "type": "string", "maxLength": 255 },
                        "key": { "type": "string", "description": "Unique within the tenant." },
                        "description": { "type": "string" },
                        "priority": {
                            "type": "string",
                            "enum": ["CRITICAL", "HIGH", "NORMAL", "LOW", "BACKGROUND"],
                            "default": "NORMAL"
                        }
                    }
                }))),
                "201",
            ),
        }),
    );

    let job_id = id_param("job_id");
    paths.insert(
        "/jobs/{job_id}".into(),
        json!({
            "get": op("Get a job", "jobs", "getJob", Some("jobs:read"), vec![job_id.clone()], None, "200"),
            "patch": op(
                "Update a job", "jobs", "updateJob", Some("jobs:write"),
                vec![job_id.clone()],
                Some(json_body(json!({
                    "type": "object",
                    "required": ["expectedUpdatedAt"],
                    "description": "Optimistic concurrency: supply the `updated_at` you last read. A stale value returns 409.",
                    "properties": {
                        "name": { "type": "string" },
                        "description": { "type": ["string", "null"] },
                        "key": { "type": "string" },
                        "priority": { "type": "string" },
                        "expectedUpdatedAt": { "type": "string", "format": "date-time" }
                    }
                }))),
                "200",
            ),
            "delete": op("Archive a job", "jobs", "archiveJob", Some("jobs:delete"), vec![job_id.clone()], None, "200"),
        }),
    );

    // --- job versions (spec 05 endpoints 6-8) ---
    paths.insert(
        "/jobs/{job_id}/versions".into(),
        json!({
            "get": op("List job versions", "job-versions", "listJobVersions", Some("job_versions:read"), vec![job_id.clone()], None, "200"),
            "post": op(
                "Create a job version", "job-versions", "createJobVersion", Some("job_versions:write"),
                vec![job_id.clone()],
                Some(json_body(json!({
                    "type": "object",
                    "properties": {
                        "execution_type": {
                            "type": "string",
                            "enum": ["HTTP_REQUEST", "CONTAINER_COMMAND", "WORKER_TASK"],
                            "default": "WORKER_TASK"
                        },
                        "execution_config": { "type": "object" },
                        "timeout_seconds": { "type": "integer", "minimum": 1, "default": 3600 }
                    }
                }))),
                "201",
            ),
        }),
    );

    paths.insert(
        "/jobs/{job_id}/versions/{version_id}/publish".into(),
        json!({
            "post": op(
                "Publish a job version", "job-versions", "publishJobVersion",
                Some("job_versions:write"),
                vec![job_id.clone(), id_param("version_id")],
                None,
                "200",
            )
        }),
    );

    // --- trigger (spec 05 endpoint 9) ---
    paths.insert(
        "/jobs/{job_id}/trigger".into(),
        json!({
            "post": op(
                "Create a manual execution", "executions", "triggerJob", Some("jobs:trigger"),
                vec![job_id.clone(), json!({ "$ref": "#/components/parameters/idempotencyKey" })],
                Some(json_body(json!({
                    "type": "object",
                    "properties": {
                        "input": { "type": "object" },
                        "version_id": { "type": "string", "format": "uuid" }
                    }
                }))),
                "202",
            )
        }),
    );

    // --- schedules (spec 05 endpoints 10-16) ---
    paths.insert(
        "/schedules".into(),
        json!({
            "get": op("List schedules", "schedules", "listSchedules", Some("schedules:read"), paging(), None, "200"),
            "post": op(
                "Create a schedule", "schedules", "createSchedule", Some("schedules:write"),
                vec![],
                Some(json_body(json!({
                    "type": "object",
                    "required": ["target_id", "timezone"],
                    "description": "See the top-level `x-forge-scheduling` section for the cron dialect, timezone rules, and DST policy.",
                    "properties": {
                        "target_type": { "type": "string", "enum": ["JOB", "WORKFLOW"], "default": "JOB" },
                        "target_id": { "type": "string", "format": "uuid" },
                        "target_version_policy": {
                            "type": "string",
                            "enum": ["PINNED", "LATEST_PUBLISHED"],
                            "default": "LATEST_PUBLISHED"
                        },
                        "schedule_type": {
                            "type": "string",
                            "enum": ["CRON", "ONE_TIME", "INTERVAL"],
                            "default": "CRON"
                        },
                        "expression": { "type": "string", "description": "Five-field cron." },
                        "timezone": { "type": "string", "description": "IANA name, e.g. Asia/Kolkata." },
                        "misfire_policy": {
                            "type": "string",
                            "enum": ["SKIP", "FIRE_ONCE", "CATCH_UP"],
                            "default": "FIRE_ONCE"
                        },
                        "catch_up_limit": { "type": "integer", "default": 100, "minimum": 1 },
                        "one_time_at": { "type": "string", "format": "date-time" },
                        "interval_seconds": { "type": "integer", "minimum": 1 }
                    }
                }))),
                "201",
            ),
        }),
    );

    let schedule_id = id_param("id");
    paths.insert(
        "/schedules/{id}".into(),
        json!({
            "get": op("Get a schedule", "schedules", "getSchedule", Some("schedules:read"), vec![schedule_id.clone()], None, "200"),
            "patch": op("Update a schedule", "schedules", "updateSchedule", Some("schedules:write"), vec![schedule_id.clone()], Some(json_body(json!({ "type": "object" }))), "200"),
        }),
    );
    paths.insert(
        "/schedules/{id}/pause".into(),
        json!({ "post": op("Pause a schedule", "schedules", "pauseSchedule", Some("schedules:write"), vec![schedule_id.clone()], None, "200") }),
    );
    paths.insert(
        "/schedules/{id}/resume".into(),
        json!({ "post": op("Resume a schedule", "schedules", "resumeSchedule", Some("schedules:write"), vec![schedule_id.clone()], None, "200") }),
    );
    paths.insert(
        "/schedules/{id}/preview".into(),
        json!({
            "post": op(
                "Preview upcoming occurrences", "schedules", "previewSchedule", Some("schedules:read"),
                vec![schedule_id.clone()],
                Some(json_body(json!({
                    "type": "object",
                    "properties": { "count": { "type": "integer", "default": 10, "minimum": 1, "maximum": 50 } }
                }))),
                "200",
            )
        }),
    );

    // --- executions (spec 05 endpoints 17-23) ---
    paths.insert(
        "/executions".into(),
        json!({
            "get": op(
                "List executions", "executions", "listExecutions", Some("executions:read"),
                paging().into_iter().chain([
                    json!({ "name": "job", "in": "query", "schema": { "type": "string", "format": "uuid" } }),
                    json!({ "name": "status", "in": "query", "schema": { "type": "string", "enum": [
                        "SCHEDULED","QUEUED","DISPATCHED","RUNNING","SUCCEEDED","FAILED",
                        "TIMED_OUT","CANCEL_REQUESTED","CANCELLED","RETRY_SCHEDULED",
                        "DEAD_LETTERED","ABANDONED"
                    ] } }),
                    json!({ "name": "worker", "in": "query", "schema": { "type": "string", "format": "uuid" } }),
                    json!({ "name": "queue", "in": "query", "schema": { "type": "string", "format": "uuid" } }),
                    json!({ "name": "correlation_id", "in": "query", "schema": { "type": "string" } }),
                ]).collect(),
                None,
                "200",
            )
        }),
    );

    let execution_id = id_param("id");
    paths.insert(
        "/executions/{id}".into(),
        json!({
            "get": op("Get an execution", "executions", "getExecution", Some("executions:read"), vec![execution_id.clone()], None, "200"),
        }),
    );
    paths.insert(
        "/executions/{id}/cancel".into(),
        json!({ "post": op("Request cancellation", "executions", "cancelExecution", Some("executions:cancel"), vec![execution_id.clone()], None, "200") }),
    );
    paths.insert(
        "/executions/{id}/retry".into(),
        json!({ "post": op("Retry an execution", "executions", "retryExecution", Some("executions:retry"), vec![execution_id.clone()], Some(json_body(json!({ "type": "object", "properties": { "error_class": { "type": "string" } } }))), "200") }),
    );
    paths.insert(
        "/executions/{id}/dead-letter".into(),
        json!({ "post": op("Dead-letter an execution", "executions", "deadLetterExecution", Some("executions:retry"), vec![execution_id.clone()], None, "200") }),
    );
    paths.insert(
        "/executions/{id}/attempts".into(),
        json!({ "get": op("List execution attempts", "executions", "listExecutionAttempts", Some("executions:read"), vec![execution_id.clone()], None, "200") }),
    );
    paths.insert(
        "/executions/{id}/logs".into(),
        json!({ "get": op("Read execution logs", "executions", "getExecutionLogs", Some("executions:read"), vec![execution_id.clone()], None, "200") }),
    );

    // --- workers (spec 05 endpoints 33-38) ---
    paths.insert(
        "/workers/register".into(),
        json!({
            "post": op(
                "Register a worker", "workers", "registerWorker", Some("workers:admin"), vec![],
                Some(json_body(json!({
                    "type": "object",
                    "required": ["hostname"],
                    "properties": {
                        "name": { "type": "string" },
                        "hostname": { "type": "string" },
                        "version": { "type": "string" },
                        "capabilities": { "type": "array", "items": { "type": "string" } },
                        "labels": { "type": "object" }
                    }
                }))),
                "201",
            )
        }),
    );
    paths.insert(
        "/workers".into(),
        json!({
            "get": op(
                "List workers", "workers", "listWorkers", Some("workers:read"),
                paging().into_iter().chain([
                    json!({ "name": "status", "in": "query", "schema": { "type": "string", "enum": [
                        "REGISTERING","READY","BUSY","DRAINING","OFFLINE","REVOKED"
                    ] } }),
                ]).collect(),
                None, "200",
            )
        }),
    );

    let worker_id = id_param("id");
    paths.insert(
        "/workers/{id}".into(),
        json!({ "get": op("Get a worker", "workers", "getWorker", Some("workers:read"), vec![worker_id.clone()], None, "200") }),
    );
    paths.insert(
        "/workers/{id}/heartbeat".into(),
        json!({ "post": op("Record a worker heartbeat", "workers", "heartbeatWorker", Some("workers:admin"), vec![worker_id.clone()], None, "200") }),
    );
    paths.insert(
        "/workers/{id}/drain".into(),
        json!({ "post": op("Drain a worker", "workers", "drainWorker", Some("workers:admin"), vec![worker_id.clone()], None, "200") }),
    );
    paths.insert(
        "/workers/{id}/revoke".into(),
        json!({ "post": op("Revoke a worker", "workers", "revokeWorker", Some("workers:admin"), vec![worker_id.clone()], None, "200") }),
    );

    // --- queues (spec 05 endpoints 39-44) ---
    paths.insert(
        "/queues".into(),
        json!({
            "get": op("List queues", "queues", "listQueues", Some("queues:read"), vec![], None, "200"),
            "post": op(
                "Create a queue", "queues", "createQueue", Some("queues:write"), vec![],
                Some(json_body(json!({
                    "type": "object",
                    "required": ["name"],
                    "properties": {
                        "name": { "type": "string" },
                        "max_concurrency": { "type": "integer", "minimum": 1 }
                    }
                }))),
                "201",
            ),
        }),
    );

    let queue_id = id_param("id");
    paths.insert(
        "/queues/{id}".into(),
        json!({
            "get": op("Get a queue", "queues", "getQueue", Some("queues:read"), vec![queue_id.clone()], None, "200"),
            "patch": op("Update a queue", "queues", "updateQueue", Some("queues:write"), vec![queue_id.clone()], Some(json_body(json!({ "type": "object" }))), "200"),
        }),
    );
    paths.insert(
        "/queues/{id}/pause".into(),
        json!({ "post": op("Pause a queue", "queues", "pauseQueue", Some("queues:write"), vec![queue_id.clone()], None, "200") }),
    );
    paths.insert(
        "/queues/{id}/resume".into(),
        json!({ "post": op("Resume a queue", "queues", "resumeQueue", Some("queues:write"), vec![queue_id.clone()], None, "200") }),
    );

    // --- users (spec 05 endpoints 45-48) ---
    paths.insert(
        "/users".into(),
        json!({
            "get": op("List users", "users", "listUsers", Some("users:read"), vec![], None, "200"),
            "post": op(
                "Create a user", "users", "createUser", Some("users:write"), vec![],
                Some(json_body(json!({
                    "type": "object",
                    "required": ["email", "password", "role"],
                    "properties": {
                        "email": { "type": "string", "format": "email" },
                        "password": { "type": "string", "minLength": 12 },
                        "role": { "type": "string", "enum": ["OWNER","ADMIN","OPERATOR","DEVELOPER","AUDITOR","VIEWER"] },
                        "display_name": { "type": "string" }
                    }
                }))),
                "201",
            ),
        }),
    );
    paths.insert(
        "/users/{id}".into(),
        json!({ "patch": op("Update a user", "users", "updateUser", Some("users:write"), vec![id_param("id")], Some(json_body(json!({ "type": "object" }))), "200") }),
    );
    paths.insert(
        "/users/{id}/disable".into(),
        json!({ "post": op("Disable a user", "users", "disableUser", Some("users:write"), vec![id_param("id")], None, "200") }),
    );

    // --- API keys (spec 05 endpoints 49-51) ---
    paths.insert(
        "/api-keys".into(),
        json!({
            "get": op("List API keys", "api-keys", "listApiKeys", Some("users:write"), vec![], None, "200"),
            "post": op(
                "Create an API key", "api-keys", "createApiKey", Some("users:write"), vec![],
                Some(json_body(json!({
                    "type": "object",
                    "required": ["name"],
                    "properties": { "name": { "type": "string" } }
                }))),
                "201",
            ),
        }),
    );
    paths.insert(
        "/api-keys/{id}/revoke".into(),
        json!({ "post": op("Revoke an API key", "api-keys", "revokeApiKey", Some("users:write"), vec![id_param("id")], None, "200") }),
    );

    // --- audit (spec 05 endpoint 52) ---
    paths.insert(
        "/audit-events".into(),
        json!({
            "get": op(
                "List audit events", "audit", "listAuditEvents", Some("audit:read"),
                paging().into_iter().chain([
                    json!({ "name": "actor", "in": "query", "schema": { "type": "string", "format": "uuid" } }),
                    json!({ "name": "action", "in": "query", "schema": { "type": "string" } }),
                    json!({ "name": "resource", "in": "query", "schema": { "type": "string" } }),
                ]).collect(),
                None, "200",
            )
        }),
    );

    // --- integrations (spec 05 endpoints 53-56) ---
    paths.insert(
        "/integrations".into(),
        json!({
            "get": op("List integrations", "integrations", "listIntegrations", Some("settings:write"), vec![], None, "200"),
            "post": op(
                "Create an integration", "integrations", "createIntegration", Some("settings:write"), vec![],
                Some(json_body(json!({
                    "type": "object",
                    "required": ["kind", "name"],
                    "description": "Configuration references secrets by identifier; embedding secret material is refused.",
                    "properties": {
                        "kind": { "type": "string" },
                        "name": { "type": "string" },
                        "config": { "type": "object" },
                        "secret_reference": { "type": "string" }
                    }
                }))),
                "201",
            ),
        }),
    );
    paths.insert(
        "/integrations/{id}".into(),
        json!({
            "patch": op("Update an integration", "integrations", "updateIntegration", Some("settings:write"), vec![id_param("id")], Some(json_body(json!({ "type": "object" }))), "200"),
            "delete": op("Delete an integration", "integrations", "deleteIntegration", Some("settings:write"), vec![id_param("id")], None, "200"),
        }),
    );

    // --- auth (spec 05 endpoints 57-59) ---
    paths.insert(
        "/auth/register".into(),
        json!({
            "post": op(
                "Register a user", "auth", "register", None, vec![],
                Some(json_body(json!({
                    "type": "object",
                    "required": ["email", "password"],
                    "properties": {
                        "email": { "type": "string", "format": "email" },
                        "password": { "type": "string", "minLength": 12 },
                        "tenant_name": { "type": "string" },
                        "display_name": { "type": "string" }
                    }
                }))),
                "201",
            )
        }),
    );
    paths.insert(
        "/auth/login".into(),
        json!({
            "post": op(
                "Sign in", "auth", "login", None, vec![],
                Some(json_body(json!({
                    "type": "object",
                    "required": ["email", "password"],
                    "properties": {
                        "email": { "type": "string", "format": "email" },
                        "password": { "type": "string" }
                    }
                }))),
                "200",
            )
        }),
    );
    paths.insert(
        "/auth/refresh".into(),
        json!({
            "post": op(
                "Refresh an access token", "auth", "refresh", None, vec![],
                Some(json_body(json!({
                    "type": "object",
                    "required": ["refresh_token"],
                    "properties": { "refresh_token": { "type": "string" } }
                }))),
                "200",
            )
        }),
    );
    paths.insert(
        "/auth/logout".into(),
        json!({ "post": op("Sign out", "auth", "logout", None, vec![], None, "200") }),
    );

    // --- system (spec 05 endpoints 60-62) ---
    let mut live = op(
        "Liveness probe",
        "system",
        "healthLive",
        None,
        vec![],
        None,
        "200",
    );
    live["security"] = json!([]);
    paths.insert("/health/live".into(), json!({ "get": live }));

    let mut ready = op(
        "Readiness probe",
        "system",
        "healthReady",
        None,
        vec![],
        None,
        "200",
    );
    ready["security"] = json!([]);
    paths.insert("/health/ready".into(), json!({ "get": ready }));

    paths.insert(
        "/health".into(),
        json!({ "get": op("Detailed health for an operator", "system", "healthDetail", Some("audit:read"), vec![], None, "200") }),
    );
    paths.insert(
        "/metrics".into(),
        json!({ "get": op("Prometheus metrics", "system", "metrics", Some("audit:read"), vec![], None, "200") }),
    );

    // The scheduling notes are an OpenAPI extension on the document root,
    // added by `document()`, so they are not inserted here.
    Value::Object(paths)
}

/// Serves the document as a value; the handler adds the content type.
pub fn document_for(base_url: &str) -> Value {
    document(base_url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_document_is_valid_openapi_31() {
        let doc = document("http://localhost:3000/api/v1");
        assert_eq!(doc["openapi"], "3.1.0");
        assert_eq!(doc["info"]["title"], "Forge API");
        assert!(doc["paths"].is_object());
        assert!(
            doc["paths"].as_object().unwrap().len() > 40,
            "the spec 05 surface is covered"
        );
    }

    /// Spec 09.4 requires the cron parser behaviour to be documented.
    #[test]
    fn the_cron_dialect_is_documented() {
        let doc = document("http://localhost:3000/api/v1");
        let notes = &doc["x-forge-scheduling"];
        assert_eq!(notes["cronDialect"]["fields"], 5);
        assert!(
            notes["cronDialect"]["dayOfWeekNumbering"]
                .as_str()
                .unwrap()
                .contains("Sunday"),
            "the Sunday-based numbering must be stated"
        );
    }

    #[test]
    fn the_dst_policy_is_documented() {
        let notes = &document("x")["x-forge-scheduling"];
        assert!(notes["daylightSaving"]["nonexistentLocalTime"].is_string());
        assert!(notes["daylightSaving"]["ambiguousLocalTime"].is_string());
    }

    #[test]
    fn misfire_and_catch_up_are_documented() {
        let notes = &document("x")["x-forge-scheduling"];
        assert_eq!(notes["misfirePolicy"]["default"], "FIRE_ONCE");
        assert_eq!(notes["misfirePolicy"]["catchUpLimitDefault"], 100);
        assert_eq!(notes["misfirePolicy"]["catchUpLimitConfigurable"], true);
    }

    /// Spec 05 §5.14 defines cursor semantics; the parameter names are pinned here.
    #[test]
    fn the_pagination_parameter_names_are_pinned() {
        let doc = document("x");
        let params = &doc["components"]["parameters"];
        assert_eq!(params["cursor"]["name"], "cursor");
        assert_eq!(params["limit"]["name"], "limit");
        assert_eq!(params["sort"]["name"], "sort");
        assert_eq!(params["idempotencyKey"]["name"], "Idempotency-Key");
        assert_eq!(params["limit"]["schema"]["default"], 50);
    }

    #[test]
    fn every_operation_has_an_id_and_a_tag() {
        let doc = document("x");
        for (path, methods) in doc["paths"].as_object().unwrap() {
            if path.starts_with("x-") {
                continue;
            }
            for (method, operation) in methods.as_object().unwrap() {
                assert!(
                    operation["operationId"].is_string(),
                    "{method} {path} needs an operationId"
                );
                assert!(
                    !operation["tags"].as_array().unwrap().is_empty(),
                    "{method} {path} needs a tag"
                );
            }
        }
    }

    #[test]
    fn auth_endpoints_are_public_and_everything_else_is_not() {
        let doc = document("x");
        assert_eq!(
            doc["paths"]["/auth/login"]["post"]["security"],
            serde_json::Value::Null,
            "login has no security override, so it inherits the global scheme"
        );
        assert_eq!(doc["paths"]["/health/live"]["get"]["security"], json!([]));

        // A tenant-scoped route documents the permission it needs.
        assert!(doc["paths"]["/jobs"]["post"]["description"]
            .as_str()
            .unwrap()
            .contains("jobs:write"));
    }

    #[test]
    fn error_responses_reference_the_shared_schema() {
        let doc = document("x");
        assert_eq!(
            doc["paths"]["/jobs"]["post"]["responses"]["400"]["$ref"],
            "#/components/responses/Error400"
        );
    }
}
