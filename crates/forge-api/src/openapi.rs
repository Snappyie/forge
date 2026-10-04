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
        // One entry per tag the operations actually use, so a client
        // generator's group list matches the routes rather than an older
        // hand-written subset.
        "tags": [
            { "name": "public" },
            { "name": "auth" },
            { "name": "jobs" },
            { "name": "schedules" },
            { "name": "queues" },
            { "name": "workers" },
            { "name": "executions" },
            { "name": "workflows" },
            { "name": "alerts" },
            { "name": "incidents" },
            { "name": "notifications" },
            { "name": "webhooks" },
            { "name": "integrations" },
            { "name": "savedViews" },
            { "name": "undo" },
            { "name": "dashboard" },
            { "name": "search" },
            { "name": "assistant" },
            { "name": "system" },
            { "name": "apiKeys" },
            { "name": "audit" },
            { "name": "users" },
            { "name": "admin" },
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
    op_public(
        tag == "public",
        Op {
            summary,
            tag,
            id,
            permission,
            parameters,
            request_body,
            success_status,
        },
    )
}

/// An operation's shape, bundled so [`op_public`] stays under the argument limit.
struct Op<'a> {
    summary: &'a str,
    tag: &'a str,
    id: &'a str,
    permission: Option<&'a str>,
    parameters: Vec<Value>,
    request_body: Option<Value>,
    success_status: &'a str,
}

/// As [`op`], but able to mark the operation explicitly public.
fn op_public(is_public: bool, o: Op<'_>) -> Value {
    let Op {
        summary,
        tag,
        id,
        permission,
        parameters,
        request_body,
        success_status,
    } = o;
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
    // In OpenAPI an absent or null `security` inherits the global scheme, so a
    // public operation must say `security: []` explicitly. Leaving it off
    // tells a generated client that login requires a token it cannot yet have.
    if is_public {
        operation["security"] = json!([]);
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
            "get": op("List job versions", "jobs", "listJobVersions", Some("job_versions:read"), vec![job_id.clone()], None, "200"),
            "post": op(
                "Create a job version", "jobs", "createJobVersion", Some("job_versions:write"),
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
                "Publish a job version", "jobs", "publishJobVersion",
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
    paths.insert(
        "/schedules/explain".into(),
        json!({
            "post": op(
                "Preview an unsaved schedule expression",
                "schedules",
                "explainSchedule",
                Some("schedules:read"),
                vec![],
                Some(json_body(json!({
                    "type": "object",
                    "required": ["expression", "timezone"],
                    "properties": {
                        "expression": { "type": "string", "description": "Five-field cron expression." },
                        "timezone": {
                            "type": "string",
                            "description": "IANA timezone. Required; never inferred from the server."
                        },
                        "count": { "type": "integer", "default": 10, "minimum": 1, "maximum": 50 }
                    }
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
        "/executions/{id}/fail".into(),
        json!({
            "post": op(
                "Report execution failure",
                "executions",
                "failExecution",
                Some("executions:write"),
                vec![execution_id.clone()],
                Some(json_body(json!({
                    "type": "object",
                    "properties": {
                        "error_message": { "type": "string" },
                        "error_code": { "type": "string" }
                    }
                }))),
                "200",
            ),
        }),
    );
    paths.insert(
        "/executions/{id}/attempts".into(),
        json!({ "get": op("List execution attempts", "executions", "listExecutionAttempts", Some("executions:read"), vec![execution_id.clone()], None, "200") }),
    );
    paths.insert(
        "/executions/{id}/logs".into(),
        json!({
            "get": op("Read execution logs", "executions", "getExecutionLogs", Some("executions:read"), vec![execution_id.clone()], None, "200"),
            "post": op(
                "Append execution log line",
                "executions",
                "appendExecutionLog",
                Some("executions:write"),
                vec![execution_id.clone()],
                Some(json_body(json!({
                    "type": "object",
                    "required": ["message"],
                    "properties": {
                        "message": { "type": "string" },
                        "level": { "type": "string" }
                    }
                }))),
                "200",
            ),
        }),
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
    paths.insert(
        "/queues/{id}/dequeue".into(),
        json!({
            "post": op(
                "Dequeue an execution for worker consumption",
                "queues",
                "dequeueQueue",
                Some("queues:write"),
                vec![queue_id.clone()],
                Some(json_body(json!({
                    "type": "object",
                    "properties": {
                        "worker_id": { "type": "string", "format": "uuid" },
                        "lease_duration_seconds": { "type": "integer" }
                    }
                }))),
                "200",
            ),
        }),
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

    paths.insert(
        "/teams".into(),
        json!({
            "get": op(
                "List teams for this tenant",
                "users",
                "listTeams",
                Some("users:read"),
                vec![],
                None,
                "200",
            ),
            "post": op(
                "Create a team",
                "users",
                "createTeam",
                Some("users:write"),
                vec![],
                Some(json_body(json!({
                    "type": "object",
                    "required": ["name"],
                    "properties": {
                        "name": { "type": "string" },
                        "description": { "type": "string" }
                    }
                }))),
                "201",
            ),
        }),
    );

    paths.insert(
        "/teams/{id}".into(),
        json!({
            "get": op(
                "Get team details and members",
                "users",
                "getTeam",
                Some("users:read"),
                vec![id_param("id")],
                None,
                "200",
            ),
            "delete": op(
                "Delete a team",
                "users",
                "deleteTeam",
                Some("users:write"),
                vec![id_param("id")],
                None,
                "200",
            ),
        }),
    );

    // --- API keys (spec 05 endpoints 49-51) ---
    paths.insert(
        "/api-keys".into(),
        json!({
            "get": op("List API keys", "apiKeys", "listApiKeys", Some("users:write"), vec![], None, "200"),
            "post": op(
                "Create an API key", "apiKeys", "createApiKey", Some("users:write"), vec![],
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
        json!({ "post": op("Revoke an API key", "apiKeys", "revokeApiKey", Some("users:write"), vec![id_param("id")], None, "200") }),
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
            "post": op_public(
                true,
                Op {
                    summary: "Register a user",
                    tag: "public",
                    id: "register",
                    permission: None,
                    parameters: vec![],
                    request_body: Some(json_body(json!({
                    "type": "object",
                    "required": ["email", "password"],
                    "properties": {
                        "email": { "type": "string", "format": "email" },
                        "password": { "type": "string", "minLength": 12 },
                        "tenant_name": { "type": "string" },
                        "display_name": { "type": "string" }
                    }
                }))),
                    success_status: "201",
                },
            )
        }),
    );
    paths.insert(
        "/auth/login".into(),
        json!({
            "post": op_public(
                true,
                Op {
                    summary: "Sign in",
                    tag: "public",
                    id: "login",
                    permission: None,
                    parameters: vec![],
                    request_body: Some(json_body(json!({
                    "type": "object",
                    "required": ["email", "password"],
                    "properties": {
                        "email": { "type": "string", "format": "email" },
                        "password": { "type": "string" }
                    }
                }))),
                    success_status: "200",
                },
            )
        }),
    );
    paths.insert(
        "/auth/refresh".into(),
        json!({
            "post": op_public(
                true,
                Op {
                    summary: "Refresh an access token",
                    tag: "public",
                    id: "refresh",
                    permission: None,
                    parameters: vec![],
                    request_body: Some(json_body(json!({
                        "type": "object",
                        "required": ["refresh_token"],
                        "properties": { "refresh_token": { "type": "string" } }
                    }))),
                    success_status: "200",
                },
            )
        }),
    );
    paths.insert(
        "/auth/logout".into(),
        // Sign-out consumes the token it revokes, so it stays authenticated.
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
    // Generated by `cargo run --bin forge-openapi-fill`.
    // Every served route is described here; the drift tests in this module
    // fail if the router gains a path this document does not describe.

    paths.insert(
        "/admin/purge-idempotency".into(),
        json!({
            "post": op(
                "Purge expired idempotency records",
                "admin",
                "adminpurgeidempotencyPost",
                Some("settings:write"),
                vec![],
                None,
                "200",
            ),
        }),
    );

    paths.insert(
        "/alert-rules".into(),
        json!({
            "get": op(
                "Configured alert rules",
                "alerts",
                "alertrulesGet",
                Some("settings:read"),
                paging(),
                None,
                "200",
            ),
            "post": op(
                "Create an alert rule",
                "alerts",
                "alertrulesPost",
                Some("settings:write"),
                vec![],
                None,
                "201",
            ),
        }),
    );

    paths.insert(
        "/alert-rules/{id}".into(),
        json!({
            "delete": op(
                "Delete an alert rule",
                "alerts",
                "alertrulesidDelete",
                Some("settings:write"),
                vec![],
                None,
                "200",
            ),
            "patch": op(
                "Change a rule's thresholds or enabled state",
                "alerts",
                "alertrulesidPatch",
                Some("settings:write"),
                vec![],
                None,
                "200",
            ),
        }),
    );

    paths.insert(
        "/alerts".into(),
        json!({
            "get": op(
                "Alerts for this tenant, filterable by status, severity and kind",
                "alerts",
                "alertsGet",
                Some("audit:read"),
                paging(),
                None,
                "200",
            ),
        }),
    );

    paths.insert(
        "/alerts/{id}/acknowledge".into(),
        json!({
            "post": op(
                "Acknowledge an open alert; a second attempt is not found",
                "alerts",
                "alertsidacknowledgePost",
                Some("audit:read"),
                vec![],
                None,
                "200",
            ),
        }),
    );

    paths.insert(
        "/alerts/{id}/resolve".into(),
        json!({
            "post": op(
                "Resolve an open alert",
                "alerts",
                "resolveAlert",
                Some("audit:read"),
                vec![id_param("id")],
                None,
                "200",
            ),
        }),
    );

    paths.insert(
        "/alerts/{id}/snooze".into(),
        json!({
            "post": op(
                "Snooze an alert for a duration",
                "alerts",
                "snoozeAlert",
                Some("audit:read"),
                vec![id_param("id")],
                Some(json_body(json!({
                    "type": "object",
                    "properties": {
                        "duration_minutes": { "type": "integer" }
                    }
                }))),
                "200",
            ),
        }),
    );

    paths.insert(
        "/alerts/summary".into(),
        json!({
            "get": op(
                "Counts of open alerts by severity",
                "alerts",
                "alertssummaryGet",
                Some("audit:read"),
                paging(),
                None,
                "200",
            ),
        }),
    );

    paths.insert(
        "/api-keys/{id}/expiry".into(),
        json!({
            "put": op(
                "Set or clear a key's expiry",
                "apiKeys",
                "apikeysidexpiryPut",
                Some("settings:write"),
                vec![],
                None,
                "200",
            ),
        }),
    );

    paths.insert(
        "/api-keys/{id}/rotate".into(),
        json!({
            "post": op(
                "Issue a replacement secret and invalidate the old one",
                "apiKeys",
                "apikeysidrotatePost",
                Some("settings:write"),
                vec![],
                None,
                "200",
            ),
        }),
    );

    paths.insert(
        "/assistant/ask".into(),
        json!({
            "post": op(
                "Answer a question from stored data; unknown questions are refused, not guessed",
                "assistant",
                "assistantaskPost",
                Some("jobs:read"),
                vec![],
                None,
                "200",
            ),
        }),
    );

    paths.insert(
        "/assistant/propose".into(),
        json!({
            "post": op(
                "Propose a configuration change; never applies it",
                "assistant",
                "assistantproposePost",
                Some("jobs:read"),
                vec![],
                None,
                "201",
            ),
        }),
    );

    paths.insert(
        "/dashboard".into(),
        json!({
            "get": op(
                "Operational summary for the dashboard",
                "dashboard",
                "dashboardGet",
                Some("executions:read"),
                paging(),
                None,
                "200",
            ),
        }),
    );

    paths.insert(
        "/emergency".into(),
        json!({
            "get": op(
                "Every stop control and whether it is engaged",
                "system",
                "emergencyGet",
                Some("jobs:read"),
                paging(),
                None,
                "200",
            ),
        }),
    );

    paths.insert(
        "/emergency/cancel-running".into(),
        json!({
            "post": op(
                "Cancel all dispatched and running executions; a reason is required",
                "system",
                "emergencycancelrunningPost",
                Some("executions:write"),
                vec![],
                None,
                "200",
            ),
        }),
    );

    paths.insert(
        "/executions/{id}/complete".into(),
        json!({
            "post": op(
                "Worker protocol: report an execution's outcome and release its lease",
                "executions",
                "executionsidcompletePost",
                Some("executions:write"),
                vec![],
                None,
                "200",
            ),
        }),
    );

    paths.insert(
        "/executions/{id}/dispatch".into(),
        json!({
            "post": op(
                "Worker protocol: dispatch a queued execution directly",
                "executions",
                "executionsiddispatchPost",
                Some("executions:write"),
                vec![],
                None,
                "200",
            ),
        }),
    );

    paths.insert(
        "/executions/{id}/heartbeat".into(),
        json!({
            "post": op(
                "Worker protocol: renew the lease on a dispatched execution",
                "executions",
                "executionsidheartbeatPost",
                Some("executions:write"),
                vec![],
                None,
                "200",
            ),
        }),
    );

    paths.insert(
        "/executions/{id}/metrics".into(),
        json!({
            "get": op(
                "CPU, memory and network samples over the run",
                "executions",
                "executionsidmetricsGet",
                Some("executions:read"),
                vec![],
                None,
                "200",
            ),
            "post": op(
                "Record a resource sample; a repeated offset corrects the earlier one",
                "executions",
                "executionsidmetricsPost",
                Some("executions:write"),
                vec![],
                None,
                "200",
            ),
        }),
    );

    paths.insert(
        "/executions/{id}/timeline".into(),
        json!({
            "get": op(
                "Lifecycle stages for an execution, using only timestamps that exist",
                "executions",
                "executionsidtimelineGet",
                Some("executions:read"),
                vec![],
                None,
                "200",
            ),
        }),
    );

    paths.insert(
        "/incidents".into(),
        json!({
            "get": op(
                "Incidents, newest first",
                "incidents",
                "incidentsGet",
                Some("audit:read"),
                paging(),
                None,
                "200",
            ),
        }),
    );

    paths.insert(
        "/incidents/{id}".into(),
        json!({
            "get": op(
                "One incident with its alerts and timeline",
                "incidents",
                "incidentsidGet",
                Some("audit:read"),
                vec![],
                None,
                "200",
            ),
        }),
    );

    paths.insert(
        "/incidents/{id}/acknowledge".into(),
        json!({
            "post": op(
                "Acknowledge an incident",
                "incidents",
                "acknowledgeIncident",
                Some("audit:read"),
                vec![id_param("id")],
                None,
                "200",
            ),
        }),
    );

    paths.insert(
        "/incidents/{id}/assign".into(),
        json!({
            "post": op(
                "Assign an incident to an operator",
                "incidents",
                "assignIncident",
                Some("audit:read"),
                vec![id_param("id")],
                Some(json_body(json!({
                    "type": "object",
                    "required": ["assigned_to"],
                    "properties": {
                        "assigned_to": { "type": "string" }
                    }
                }))),
                "200",
            ),
        }),
    );

    paths.insert(
        "/incidents/{id}/resolve".into(),
        json!({
            "post": op(
                "Resolve an incident",
                "incidents",
                "resolveIncident",
                Some("audit:read"),
                vec![id_param("id")],
                None,
                "200",
            ),
        }),
    );

    paths.insert(
        "/integrations/{id}/test".into(),
        json!({
            "post": op(
                "Inspect an integration's configuration; no external dial is attempted",
                "integrations",
                "integrationsidtestPost",
                Some("settings:write"),
                vec![],
                None,
                "200",
            ),
        }),
    );

    paths.insert(
        "/job-dependencies/{edge_id}".into(),
        json!({
            "delete": op(
                "Remove a dependency edge",
                "jobs",
                "jobdependenciesedgeidDelete",
                Some("jobs:write"),
                vec![],
                None,
                "200",
            ),
        }),
    );

    paths.insert(
        "/jobs/{job_id}/dependencies".into(),
        json!({
            "get": op(
                "What this job waits for, and what waits for it",
                "jobs",
                "jobsjobiddependenciesGet",
                Some("jobs:read"),
                paging(),
                None,
                "200",
            ),
            "post": op(
                "Declare that this job depends on another",
                "jobs",
                "jobsjobiddependenciesPost",
                Some("jobs:write"),
                vec![],
                None,
                "201",
            ),
        }),
    );

    paths.insert(
        "/jobs/{job_id}/executions".into(),
        json!({
            "get": op(
                "Executions for one job",
                "executions",
                "jobsjobidexecutionsGet",
                Some("executions:read"),
                paging(),
                None,
                "200",
            ),
        }),
    );

    paths.insert(
        "/jobs/{job_id}/health".into(),
        json!({
            "get": op(
                "Reliability and duration percentiles for a job",
                "jobs",
                "jobsjobidhealthGet",
                Some("jobs:read"),
                paging(),
                None,
                "200",
            ),
        }),
    );

    paths.insert(
        "/jobs/{job_id}/sla".into(),
        json!({
            "get": op(
                "Per-run SLA outcomes for a job",
                "jobs",
                "jobsjobidslaGet",
                Some("jobs:read"),
                paging(),
                None,
                "200",
            ),
            "put": op(
                "Set or clear the job's SLA target",
                "jobs",
                "jobsjobidslaPut",
                Some("jobs:write"),
                vec![],
                None,
                "200",
            ),
        }),
    );

    paths.insert(
        "/jobs/bulk".into(),
        json!({
            "post": op(
                "Pause, resume, archive or run many jobs; reports each outcome separately",
                "jobs",
                "jobsbulkPost",
                Some("jobs:write"),
                vec![],
                None,
                "200",
            ),
        }),
    );

    paths.insert(
        "/jobs/export".into(),
        json!({
            "get": op(
                "Every job and schedule as a portable JSON document",
                "jobs",
                "jobsexportGet",
                Some("jobs:read"),
                paging(),
                None,
                "200",
            ),
        }),
    );

    paths.insert(
        "/jobs/import".into(),
        json!({
            "post": op(
                "Create jobs from an exported document; reports each entry",
                "jobs",
                "jobsimportPost",
                Some("jobs:write"),
                vec![],
                None,
                "201",
            ),
        }),
    );

    paths.insert(
        "/maintenance".into(),
        json!({
            "delete": op(
                "Lift maintenance mode",
                "system",
                "maintenanceDelete",
                Some("settings:write"),
                vec![],
                None,
                "200",
            ),
            "get": op(
                "Whether the tenant is currently in maintenance mode",
                "system",
                "maintenanceGet",
                Some("jobs:read"),
                paging(),
                None,
                "200",
            ),
            "post": op(
                "Hold new scheduling for this tenant; a reason is required",
                "system",
                "maintenancePost",
                Some("settings:write"),
                vec![],
                None,
                "201",
            ),
        }),
    );

    paths.insert(
        "/notification-preferences".into(),
        json!({
            "get": op(
                "Notification preferences, with documented defaults when unset",
                "notifications",
                "notificationpreferencesGet",
                Some("executions:read"),
                paging(),
                None,
                "200",
            ),
        }),
    );

    paths.insert(
        "/notification-preferences/update".into(),
        json!({
            "post": op(
                "Update notification preferences",
                "notifications",
                "notificationpreferencesupdatePost",
                Some("executions:read"),
                vec![],
                None,
                "200",
            ),
        }),
    );

    paths.insert(
        "/notifications".into(),
        json!({
            "get": op(
                "The caller's notifications and unread count",
                "notifications",
                "notificationsGet",
                Some("executions:read"),
                paging(),
                None,
                "200",
            ),
        }),
    );

    paths.insert(
        "/notifications/read".into(),
        json!({
            "post": op(
                "Mark every notification read",
                "notifications",
                "notificationsreadPost",
                Some("executions:read"),
                vec![],
                None,
                "200",
            ),
        }),
    );

    paths.insert(
        "/openapi.json".into(),
        json!({
            "get": op_public(
                true,
                Op {
                    summary: "The OpenAPI document describing every route this server serves",
                    tag: "public",
                    id: "openapiDocument",
                    permission: None,
                    parameters: vec![],
                    request_body: None,
                    success_status: "200",
                },
            ),
        }),
    );

    paths.insert(
        "/saved-views".into(),
        json!({
            "get": op(
                "The caller's saved views plus their tenant's shared views",
                "savedViews",
                "savedviewsGet",
                Some("jobs:read"),
                paging(),
                None,
                "200",
            ),
            "post": op(
                "Save a filter state for reuse",
                "savedViews",
                "savedviewsPost",
                Some("jobs:read"),
                vec![],
                None,
                "201",
            ),
        }),
    );

    paths.insert(
        "/saved-views/{id}".into(),
        json!({
            "delete": op(
                "Delete a saved view the caller owns",
                "savedViews",
                "savedviewsidDelete",
                Some("jobs:read"),
                vec![],
                None,
                "200",
            ),
        }),
    );

    paths.insert(
        "/scheduler/heartbeat".into(),
        json!({
            "post": op(
                "Record the scheduler's evaluation lag",
                "system",
                "schedulerheartbeatPost",
                Some("executions:write"),
                vec![],
                None,
                "200",
            ),
        }),
    );

    paths.insert(
        "/search".into(),
        json!({
            "get": op(
                "Search jobs, executions, workers and alerts with grouped counts",
                "search",
                "searchGet",
                Some("jobs:read"),
                paging(),
                None,
                "200",
            ),
        }),
    );

    paths.insert(
        "/sla/compliance".into(),
        json!({
            "get": op(
                "Tenant-wide SLA compliance over the last 30 days",
                "dashboard",
                "slacomplianceGet",
                Some("audit:read"),
                paging(),
                None,
                "200",
            ),
        }),
    );

    paths.insert(
        "/system/health".into(),
        json!({
            "get": op(
                "Per-component health, reporting `unknown` rather than `healthy` when unmeasured",
                "system",
                "systemhealthGet",
                Some("audit:read"),
                paging(),
                None,
                "200",
            ),
        }),
    );

    paths.insert(
        "/undo".into(),
        json!({
            "get": op(
                "Actions the caller can still reverse",
                "undo",
                "undoGet",
                Some("jobs:read"),
                paging(),
                None,
                "200",
            ),
        }),
    );

    paths.insert(
        "/undo/{id}".into(),
        json!({
            "post": op(
                "Reverse a recorded action; single use",
                "undo",
                "undoidPost",
                Some("jobs:write"),
                vec![],
                None,
                "200",
            ),
        }),
    );

    paths.insert(
        "/upcoming".into(),
        json!({
            "get": op(
                "Scheduled executions and upcoming schedule runs, merged in time order",
                "dashboard",
                "upcomingGet",
                Some("jobs:read"),
                paging(),
                None,
                "200",
            ),
        }),
    );

    paths.insert(
        "/webhooks".into(),
        json!({
            "get": op(
                "Webhook subscriptions and their connection state",
                "webhooks",
                "webhooksGet",
                Some("settings:read"),
                paging(),
                None,
                "200",
            ),
            "post": op(
                "Create a webhook; the URL is checked against the SSRF rules",
                "webhooks",
                "webhooksPost",
                Some("settings:write"),
                vec![],
                None,
                "201",
            ),
        }),
    );

    paths.insert(
        "/webhooks/{id}".into(),
        json!({
            "delete": op(
                "Delete a webhook and its delivery history",
                "webhooks",
                "webhooksidDelete",
                Some("settings:write"),
                vec![],
                None,
                "200",
            ),
        }),
    );

    paths.insert(
        "/webhooks/{id}/deliveries".into(),
        json!({
            "get": op(
                "Recent delivery attempts with request and response",
                "webhooks",
                "webhooksiddeliveriesGet",
                Some("settings:read"),
                vec![],
                None,
                "200",
            ),
        }),
    );

    paths.insert(
        "/webhooks/{id}/test".into(),
        json!({
            "post": op(
                "Queue a test delivery and report the recorded outcome",
                "webhooks",
                "webhooksidtestPost",
                Some("settings:write"),
                vec![],
                None,
                "200",
            ),
        }),
    );

    paths.insert(
        "/workers/{worker_id}/claim".into(),
        json!({
            "post": op(
                "Worker protocol: take the next queued execution this worker may run",
                "workers",
                "workersworkeridclaimPost",
                Some("workers:admin"),
                vec![],
                None,
                "200",
            ),
        }),
    );

    paths.insert(
        "/workflows".into(),
        json!({
            "get": op(
                "List workflows",
                "workflows",
                "workflowsGet",
                Some("workflows:read"),
                paging(),
                None,
                "200",
            ),
            "post": op(
                "Create a workflow with its definition",
                "workflows",
                "workflowsPost",
                Some("workflows:write"),
                vec![],
                None,
                "201",
            ),
        }),
    );

    paths.insert(
        "/workflows/{id}".into(),
        json!({
            "get": op(
                "One workflow, including its stored definition",
                "workflows",
                "workflowsidGet",
                Some("workflows:read"),
                vec![],
                None,
                "200",
            ),
        }),
    );

    paths.insert(
        "/workflows/{id}/definition".into(),
        json!({
            "put": op(
                "Save an edited graph as a new draft version",
                "workflows",
                "workflowsiddefinitionPut",
                Some("workflows:write"),
                vec![],
                None,
                "200",
            ),
        }),
    );

    paths.insert(
        "/workflows/{id}/publish".into(),
        json!({
            "post": op(
                "Publish a workflow version, making it active",
                "workflows",
                "workflowsidpublishPost",
                Some("workflows:write"),
                vec![],
                None,
                "200",
            ),
        }),
    );

    paths.insert(
        "/workflows/{id}/versions/{version_id}/publish".into(),
        json!({
            "post": op(
                "Publish a specific workflow version",
                "workflows",
                "publishWorkflowVersion",
                Some("workflows:write"),
                vec![id_param("id"), id_param("version_id")],
                None,
                "200",
            ),
        }),
    );

    paths.insert(
        "/workflows/{id}/trigger".into(),
        json!({
            "post": op(
                "Start a workflow run",
                "workflows",
                "workflowsidtriggerPost",
                Some("workflows:write"),
                vec![],
                None,
                "202",
            ),
        }),
    );

    paths.insert(
        "/workflows/{id}/versions".into(),
        json!({
            "get": op(
                "Version history for a workflow",
                "workflows",
                "workflowsidversionsGet",
                Some("workflows:read"),
                vec![],
                None,
                "200",
            ),
        }),
    );

    paths.insert(
        "/workflows/validate".into(),
        json!({
            "post": op(
                "Validate a graph without persisting it",
                "workflows",
                "workflowsvalidatePost",
                Some("workflows:read"),
                vec![],
                None,
                "200",
            ),
        }),
    );

    // --- human-in-the-loop approvals (spec 05 §5.7 endpoints 31-32) ---
    // A workflow run suspends on an APPROVAL node until an authorised user
    // decides; these are the endpoints the console and the approval queue call.
    let workflow_execution = id_param("execution_id");
    let workflow_node = id_param("node_id");
    paths.insert(
        "/workflows/executions/{execution_id}".into(),
        json!({
            "get": op(
                "One workflow run with the live state of every node",
                "workflows",
                "getWorkflowExecution",
                Some("workflows:read"),
                vec![workflow_execution.clone()],
                None,
                "200",
            ),
        }),
    );
    paths.insert(
        "/workflows/executions/{execution_id}/cancel".into(),
        json!({
            "post": op(
                "Request cancellation of a workflow run and its children",
                "workflows",
                "cancelWorkflowExecution",
                Some("workflows:trigger"),
                vec![workflow_execution.clone()],
                None,
                "200",
            ),
        }),
    );
    paths.insert(
        "/workflows/executions/{execution_id}/approvals".into(),
        json!({
            "get": op(
                "List the approval decisions recorded for a workflow run",
                "workflows",
                "listWorkflowApprovals",
                Some("workflows:read"),
                vec![workflow_execution.clone()],
                None,
                "200",
            ),
        }),
    );
    let approval_decision = Some(json_body(json!({
        "type": "object",
        "properties": { "comment": { "type": "string", "maxLength": 2000 } }
    })));
    paths.insert(
        "/workflows/executions/{execution_id}/nodes/{node_id}/approve".into(),
        json!({
            "post": op(
                "Approve a human-in-the-loop node so the run can continue",
                "workflows",
                "approveWorkflowNode",
                Some("workflows:trigger"),
                vec![workflow_execution.clone(), workflow_node.clone()],
                approval_decision.clone(),
                "200",
            ),
        }),
    );
    paths.insert(
        "/workflows/executions/{execution_id}/nodes/{node_id}/reject".into(),
        json!({
            "post": op(
                "Reject a human-in-the-loop node, failing that branch",
                "workflows",
                "rejectWorkflowNode",
                Some("workflows:trigger"),
                vec![workflow_execution, workflow_node],
                approval_decision,
                "200",
            ),
        }),
    );

    Value::Object(paths)
}

/// Serves the document as a value; the handler adds the content type.
pub fn document_for(base_url: &str) -> Value {
    document(base_url)
}

/// Normalises a path for comparison: `{id}` in OpenAPI and `:id` in axum name
/// the same segment.
#[cfg(test)]
fn normalise(path: &str) -> String {
    let stripped = path.strip_prefix("/api/v1").unwrap_or(path);
    let stripped = stripped.trim_end_matches('/');
    let mut out = String::from("/");
    for segment in stripped.split('/').filter(|s| !s.is_empty()) {
        if !out.ends_with('/') {
            out.push('/');
        }
        if let Some(name) = segment.strip_prefix(':') {
            out.push('{');
            out.push_str(name);
            out.push('}');
        } else {
            out.push_str(segment);
        }
    }
    out
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
        // An absent `security` inherits the global scheme, which would tell a
        // generated client that login needs a token it does not yet have. The
        // public entry points therefore declare `security: []` explicitly.
        assert_eq!(doc["paths"]["/auth/login"]["post"]["security"], json!([]));
        assert_eq!(
            doc["paths"]["/auth/register"]["post"]["security"],
            json!([])
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

/// Every path the router actually serves, read from `router.rs` at compile time
/// by the test below.
///
/// The document is written by hand, so the only thing that stops it drifting is
/// a check that fails the build when the two disagree. This is that check's
/// input: the set of concrete paths, with `:param` segments kept so they can be
/// compared against the document's OpenAPI `{param}` spelling.
#[cfg(test)]
pub(crate) fn router_paths_from(source: &str) -> Vec<String> {
    // Route paths are written as `format!("{PREFIX}/thing")`, sometimes split
    // across lines, so the macro call itself is the anchor rather than a bare
    // `{PREFIX}` mention.
    let mut paths = Vec::new();
    let mut rest = source;
    while let Some(at) = rest.find("format!(") {
        let after = &rest[at + "format!(".len()..];
        let Some(quote) = after.find('"') else {
            rest = after;
            continue;
        };
        if after[..quote].trim().is_empty() {
            let inside = &after[quote + 1..];
            if let Some(end_quote) = inside.find('"') {
                let literal = &inside[..end_quote];
                if let Some(path) = literal.strip_prefix("{PREFIX}") {
                    paths.push(path.trim().to_string());
                    rest = &inside[end_quote..];
                    continue;
                }
            }
        }
        rest = after;
    }
    paths.sort();
    paths.dedup();
    paths
}

#[cfg(test)]
mod drift {
    use super::*;
    use std::collections::BTreeSet;

    /// Reads the router source from disk and normalises every path it serves.
    fn served_paths() -> BTreeSet<String> {
        let source = include_str!("router.rs");
        router_paths_from(source)
            .into_iter()
            .map(|p| normalise(&p))
            .collect()
    }

    /// Every path the published document describes.
    fn documented_paths() -> BTreeSet<String> {
        document("http://localhost:3000/api/v1")["paths"]
            .as_object()
            .expect("paths is an object")
            .keys()
            .cloned()
            .collect()
    }

    /// A served route that the document does not describe is drift: a client
    /// generated from this contract could not find it.
    #[test]
    fn every_served_route_is_documented() {
        let served = served_paths();
        let documented = documented_paths();

        let undocumented: Vec<&String> = served.difference(&documented).collect();
        assert!(
            undocumented.is_empty(),
            "these routes are served but absent from the OpenAPI document: {undocumented:?}"
        );
    }

    /// A documented path that nothing serves is the other half of drift: a
    /// client would generate a call that 404s.
    #[test]
    fn every_documented_path_is_served() {
        let served = served_paths();
        let documented = documented_paths();

        let unserved: Vec<&String> = documented.difference(&served).collect();
        assert!(
            unserved.is_empty(),
            "the document describes paths the router does not serve: {unserved:?}"
        );
    }

    /// The declared tag list and the tags the operations use must agree, so a
    /// generated client's grouping is not silently out of date.
    #[test]
    fn declared_tags_match_the_operations() {
        let doc = document("http://localhost:3000/api/v1");
        let declared: std::collections::BTreeSet<String> = doc["tags"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|t| t["name"].as_str().map(String::from))
            .collect();

        let mut used: std::collections::BTreeSet<String> = Default::default();
        for item in doc["paths"].as_object().unwrap().values() {
            for method in ["get", "post", "put", "patch", "delete"] {
                if let Some(tag) = item.get(method).and_then(|o| o["tags"][0].as_str()) {
                    used.insert(tag.to_string());
                }
            }
        }

        let undeclared: Vec<&String> = used.difference(&declared).collect();
        let unused: Vec<&String> = declared.difference(&used).collect();
        assert!(
            undeclared.is_empty(),
            "operations use undeclared tags: {undeclared:?}"
        );
        assert!(
            unused.is_empty(),
            "declared tags are used by nothing: {unused:?}"
        );
    }

    /// The document must be complete enough to generate a client from, which
    /// means every operation carries an id, a tag, and a security statement.
    #[test]
    fn every_operation_is_generatable() {
        let doc = document("http://localhost:3000/api/v1");
        let paths = doc["paths"].as_object().unwrap();

        let mut problems: Vec<String> = Vec::new();
        for (path, item) in paths {
            let Some(operations) = item.as_object() else {
                problems.push(format!("{path} is not an object"));
                continue;
            };
            for (method, operation) in operations {
                if !matches!(
                    method.as_str(),
                    "get" | "post" | "put" | "patch" | "delete" | "head" | "options"
                ) {
                    continue;
                }
                if operation["operationId"].as_str().unwrap_or("").is_empty() {
                    problems.push(format!("{path} {method} has no operationId"));
                }
                if operation["tags"].as_array().is_none_or(|t| t.is_empty()) {
                    problems.push(format!("{path} {method} has no tag"));
                }
                // A null/absent `security` inherits the global scheme, so an
                // explicitly public operation must say `security: []`.
                let is_public = operation["security"].as_array().is_some();
                let declared_public = path == "/health/live" || path == "/health/ready";
                if declared_public && !is_public {
                    problems.push(format!(
                        "{path} {method} is public but inherits the global security scheme"
                    ));
                }
            }
        }
        assert!(problems.is_empty(), "{problems:#?}");
    }
}
