//! Command implementations, one section per spec 16 subsection.

use serde_json::{json, Value};

use crate::api::ApiClient;
use crate::context::{self, Config, Context};
use crate::error::{CliError, Result};
use crate::output::{OutputFormat, Render, Renderable, Table};
use crate::{
    active_context, detail, list_from, parse_json, query, render, AuthCommands, DetailView,
    ExecutionCommands, JobCommands, QueueCommands, WorkerCommands, WorkflowCommands,
};

// ---------------------------------------------------------------------------
// 16.2 Authentication
// ---------------------------------------------------------------------------

pub async fn auth(
    config: &Config,
    action: &AuthCommands,
    format: OutputFormat,
) -> Result<Option<String>> {
    match action {
        AuthCommands::Login {
            email,
            password,
            api_url,
        } => {
            let url = match api_url {
                Some(url) => url.clone(),
                None => config
                    .active()
                    .map(|c| c.api_url.clone())
                    .or_else(|| std::env::var("FORGE_API_URL").ok())
                    .ok_or_else(|| {
                        CliError::Usage(
                            "no server URL; pass --api-url or set FORGE_API_URL".to_string(),
                        )
                    })?,
            };

            let name = config
                .active()
                .map(|c| c.name.clone())
                .unwrap_or_else(|| "default".to_string());
            let mut context = Context::new(&name, &url);

            let password = match password {
                Some(p) => p.clone(),
                None => crate::read_password()?,
            };

            let client = ApiClient::new(&context)?;
            let data = client
                .post(
                    "/auth/login",
                    json!({ "email": email, "password": password }),
                )
                .await?;

            context.access_token = data["access_token"].as_str().map(str::to_string);
            context.refresh_token = data["refresh_token"].as_str().map(str::to_string);
            context.tenant_id = data["tenant_id"].as_str().map(str::to_string);

            if context.access_token.is_none() {
                return Err(CliError::Authentication(
                    "the server did not return an access token".to_string(),
                ));
            }

            // Read what the view needs before the context is stored.
            let tenant_id = context.tenant_id.clone();
            let api_url = context.api_url.clone();

            let mut updated = config.clone();
            updated.put(context);
            context::save(&updated)?;

            let view = detail(
                "signed in",
                json!({
                    "context": name,
                    "api_url": api_url,
                    "tenant_id": tenant_id,
                    // The token itself is deliberately not echoed.
                    "authenticated": true,
                }),
            );
            Ok(Some(render(&view, format)))
        }

        AuthCommands::Logout => {
            let context = active_context(config)?;
            if context.is_authenticated() {
                // Best effort: the local credential is discarded either way.
                let _ = ApiClient::new(&context)?
                    .post("/auth/logout", json!({}))
                    .await;
            }

            let mut updated = config.clone();
            if let Some(active) = updated.active_mut() {
                active.access_token = None;
                active.refresh_token = None;
            }
            context::save(&updated)?;

            let view = detail("signed out", json!({ "authenticated": false }));
            Ok(Some(render(&view, format)))
        }

        AuthCommands::Status => {
            let context = active_context(config)?;
            let view = detail(
                "status",
                json!({
                    "context": context.name,
                    "api_url": context.api_url,
                    "tenant_id": context.tenant_id,
                    "authenticated": context.is_authenticated(),
                }),
            );
            Ok(Some(render(&view, format)))
        }

        AuthCommands::Contexts => {
            let items: Vec<Value> = config
                .contexts
                .iter()
                .map(|c| {
                    json!({
                        "name": c.name,
                        "api_url": c.api_url,
                        "current": config.current.as_deref() == Some(c.name.as_str()),
                        "authenticated": c.is_authenticated(),
                    })
                })
                .collect();

            let view = list_from(
                &["CURRENT", "NAME", "API URL", "AUTHENTICATED"],
                &["current", "name", "api_url", "authenticated"],
                Value::Array(items),
            );
            Ok(Some(render(&view, format)))
        }
    }
}

// ---------------------------------------------------------------------------
// 16.3 Jobs
// ---------------------------------------------------------------------------

pub async fn jobs(
    config: &Config,
    action: &JobCommands,
    format: OutputFormat,
) -> Result<Option<String>> {
    let client = crate::authenticated_client(config)?;

    let view = match action {
        JobCommands::List {
            status,
            search,
            limit,
            cursor,
        } => {
            let path = format!(
                "/jobs{}",
                query(&[
                    ("status", status.clone()),
                    ("search", search.clone()),
                    (
                        "limit",
                        limit.map(|l| l.to_string()).filter(|_| limit.is_some()),
                    ),
                    ("cursor", cursor.clone()),
                ])
            );
            let data = client.get(&path).await?;
            Renderable::List(list_from(
                &["ID", "NAME", "STATUS", "PRIORITY", "UPDATED"],
                &["id", "name", "status", "priority", "updated_at"],
                data,
            ))
        }

        JobCommands::Get { id } => {
            let data = client.get(&format!("/jobs/{id}")).await?;
            Renderable::Detail(detail("job", data))
        }

        JobCommands::Create {
            name,
            key,
            description,
            priority,
        } => {
            let mut body = json!({ "name": name, "priority": priority });
            if let Some(key) = key {
                body["key"] = json!(key);
            }
            if let Some(description) = description {
                body["description"] = json!(description);
            }
            let data = client.post("/jobs", body).await?;
            Renderable::Detail(detail("job created", data))
        }

        JobCommands::Update {
            id,
            name,
            description,
            priority,
            expected_updated_at,
        } => {
            let current = client.get(&format!("/jobs/{id}")).await?;
            // Optimistic concurrency needs the timestamp the client last read;
            // read it here when the caller did not supply one.
            let expected = match expected_updated_at {
                Some(value) => value.clone(),
                None => current["updated_at"]
                    .as_str()
                    .map(str::to_string)
                    .ok_or_else(|| {
                        CliError::Usage(
                            "could not determine the current updated_at; pass --expected-updated-at"
                                .to_string(),
                        )
                    })?,
            };

            let mut body = json!({ "expected_updated_at": expected });
            if let Some(name) = name {
                body["name"] = json!(name);
            }
            if let Some(description) = description {
                body["description"] = json!(description);
            }
            if let Some(priority) = priority {
                body["priority"] = json!(priority);
            }
            let data = client.patch(&format!("/jobs/{id}"), body).await?;
            Renderable::Detail(detail("job updated", data))
        }

        JobCommands::Version {
            id,
            execution_type,
            timeout_seconds,
            execution_config,
        } => {
            let body = json!({
                "execution_type": execution_type,
                "timeout_seconds": timeout_seconds,
                "execution_config": match execution_config {
                    Some(raw) => parse_json(raw, "execution-config")?,
                    None => json!({}),
                },
            });
            let data = client.post(&format!("/jobs/{id}/versions"), body).await?;
            Renderable::Detail(detail("version created", data))
        }

        JobCommands::Versions { id } => {
            let data = client.get(&format!("/jobs/{id}/versions")).await?;
            Renderable::List(list_from(
                &["VERSION", "TYPE", "PUBLISHED", "CREATED"],
                &[
                    "version_number",
                    "execution_type",
                    "published_at",
                    "created_at",
                ],
                data,
            ))
        }

        JobCommands::Publish { id, version } => {
            // The CLI takes a human-facing version number, so resolve it to the
            // version id the endpoint expects.
            let versions = client.get(&format!("/jobs/{id}/versions")).await?;
            let list = versions.as_array().cloned().unwrap_or_default();
            let wanted: i64 = version
                .parse()
                .map_err(|_| CliError::Usage(format!("`{version}` is not a version number")))?;
            let target = list
                .iter()
                .find(|v| v["version_number"].as_i64() == Some(wanted))
                .ok_or_else(|| CliError::NotFound(format!("version {wanted} of job {id}")))?;

            let version_id = target["id"].as_str().unwrap_or_default();
            let data = client
                .post(
                    &format!("/jobs/{id}/versions/{version_id}/publish"),
                    json!({}),
                )
                .await?;
            Renderable::Detail(detail("version published", data))
        }

        JobCommands::Run {
            id,
            input,
            idempotency_key,
        } => {
            let body = json!({
                "input": match input {
                    Some(raw) => parse_json(raw, "input")?,
                    None => json!({}),
                }
            });
            let data = match idempotency_key {
                Some(key) => {
                    client
                        .post_idempotent(&format!("/jobs/{id}/trigger"), body, key)
                        .await?
                }
                None => client.post(&format!("/jobs/{id}/trigger"), body).await?,
            };
            Renderable::Detail(detail("execution created", data))
        }

        JobCommands::Pause { id } => {
            Renderable::Detail(pause_schedule(&client, id, "pause").await?)
        }
        JobCommands::Resume { id } => {
            Renderable::Detail(pause_schedule(&client, id, "resume").await?)
        }
        JobCommands::Archive { id } => {
            let data = client.delete(&format!("/jobs/{id}")).await?;
            Renderable::Detail(detail("job archived", data))
        }
    };

    Ok(Some(render(&view, format)))
}

/// Pauses or resumes every schedule belonging to a job.
async fn pause_schedule(client: &ApiClient, job_id: &str, action: &str) -> Result<DetailView> {
    let schedules = client
        .get("/schedules?limit=200")
        .await
        .unwrap_or(Value::Array(vec![]));

    let list = schedules.as_array().cloned().unwrap_or_default();
    let mine: Vec<&Value> = list
        .iter()
        .filter(|s| s["target_id"].as_str() == Some(job_id))
        .collect();

    if mine.is_empty() {
        return Err(CliError::NotFound(format!(
            "job {job_id} has no schedule to {action}"
        )));
    }

    let mut changed = 0;
    for schedule in mine {
        let id = schedule["id"].as_str().unwrap_or_default();
        client
            .post(&format!("/schedules/{id}/{action}"), json!({}))
            .await?;
        changed += 1;
    }

    Ok(detail(
        &format!("schedules {action}d"),
        json!({ "job_id": job_id, "schedules_changed": changed }),
    ))
}

// ---------------------------------------------------------------------------
// 16.4 Workflows
// ---------------------------------------------------------------------------

pub async fn workflows(
    config: &Config,
    action: &WorkflowCommands,
    format: OutputFormat,
) -> Result<Option<String>> {
    let client = crate::authenticated_client(config)?;

    let view = match action {
        WorkflowCommands::List => {
            let data = client.get("/workflows").await?;
            Renderable::List(list_from(
                &["ID", "KEY", "NAME", "STATUS"],
                &["id", "key", "name", "status"],
                data,
            ))
        }
        WorkflowCommands::Get { id } => {
            let data = client.get(&format!("/workflows/{id}")).await?;
            Renderable::Detail(detail("workflow", data))
        }
        WorkflowCommands::Create {
            name,
            key,
            definition,
        } => {
            let mut body = json!({ "name": name });
            if let Some(key) = key {
                body["key"] = json!(key);
            }
            if let Some(raw) = definition {
                body["definition"] = parse_json(raw, "definition")?;
            }
            let data = client.post("/workflows", body).await?;
            Renderable::Detail(detail("workflow created", data))
        }
        WorkflowCommands::Validate { file } => {
            let contents = std::fs::read_to_string(file)
                .map_err(|e| CliError::Usage(format!("could not read {}: {e}", file.display())))?;
            let document = serde_json::from_str::<Value>(&contents).map_err(|e| {
                CliError::Usage(format!("{} is not valid JSON: {e}", file.display()))
            })?;

            // Validation runs through the API so the client and the server
            // apply the same rules; nothing is saved.
            let data = client
                .post("/workflows/validate", json!({ "definition": document }))
                .await?;
            Renderable::Detail(detail("workflow valid", data))
        }
        WorkflowCommands::Publish { id, version } => {
            let data = client
                .post(
                    &format!("/workflows/{id}/versions/{version}/publish"),
                    json!({}),
                )
                .await?;
            Renderable::Detail(detail("workflow published", data))
        }
        WorkflowCommands::Run {
            id,
            input,
            idempotency_key,
        } => {
            let body = json!({
                "input": match input {
                    Some(raw) => parse_json(raw, "input")?,
                    None => json!({}),
                }
            });
            let data = match idempotency_key {
                Some(key) => {
                    client
                        .post_idempotent(&format!("/workflows/{id}/trigger"), body, key)
                        .await?
                }
                None => {
                    client
                        .post(&format!("/workflows/{id}/trigger"), body)
                        .await?
                }
            };
            Renderable::Detail(detail("workflow execution created", data))
        }
    };

    Ok(Some(render(&view, format)))
}

// ---------------------------------------------------------------------------
// 16.5 Executions
// ---------------------------------------------------------------------------

pub async fn executions(
    config: &Config,
    action: &ExecutionCommands,
    format: OutputFormat,
) -> Result<Option<String>> {
    let client = crate::authenticated_client(config)?;

    let view = match action {
        ExecutionCommands::List {
            job,
            status,
            worker,
            limit,
            cursor,
        } => {
            let path = format!(
                "/executions{}",
                query(&[
                    ("job", job.clone()),
                    ("status", status.clone()),
                    ("worker", worker.clone()),
                    (
                        "limit",
                        limit.map(|l| l.to_string()).filter(|_| limit.is_some()),
                    ),
                    ("cursor", cursor.clone()),
                ])
            );
            let data = client.get(&path).await?;
            Renderable::List(list_from(
                &["ID", "JOB", "STATUS", "WORKER", "ATTEMPTS", "CREATED"],
                &[
                    "id",
                    "job_id",
                    "status",
                    "worker_id",
                    "attempt_count",
                    "created_at",
                ],
                data,
            ))
        }
        ExecutionCommands::Get { id } => {
            let data = client.get(&format!("/executions/{id}")).await?;
            Renderable::Detail(detail("execution", data))
        }
        ExecutionCommands::Logs { id } => {
            let data = client.get(&format!("/executions/{id}/logs")).await?;
            let lines = data["lines"].as_array().cloned().unwrap_or_default();

            let mut table = Table::new(["TIME", "STREAM", "CONTENT"]);
            for line in &lines {
                table.push(vec![
                    crate::render_cell(&line["at"]),
                    line["stream"].as_str().unwrap_or("-").to_string(),
                    line["content"].as_str().unwrap_or("").to_string(),
                ]);
            }
            Renderable::Custom(Box::new(LogsView { lines, table }))
        }
        ExecutionCommands::Cancel { id } => {
            let data = client
                .post(&format!("/executions/{id}/cancel"), json!({}))
                .await?;
            Renderable::Detail(detail("cancellation requested", data))
        }
        ExecutionCommands::Retry {
            id,
            error_class,
            reason,
        } => {
            let mut body = json!({});
            if let Some(class) = error_class {
                body["error_class"] = json!(class);
            }
            if let Some(reason) = reason {
                body["reason"] = json!(reason);
            }
            let data = client
                .post(&format!("/executions/{id}/retry"), body)
                .await?;
            Renderable::Detail(detail("retry scheduled", data))
        }
        ExecutionCommands::DeadLetter { id } => {
            let data = client
                .post(&format!("/executions/{id}/dead-letter"), json!({}))
                .await?;
            Renderable::Detail(detail("dead-lettered", data))
        }
    };

    Ok(Some(render(&view, format)))
}

struct LogsView {
    lines: Vec<Value>,
    table: Table,
}
impl Render for LogsView {
    fn summary(&self) -> String {
        format!("{} log line(s)", self.lines.len())
    }
    fn table(&self) -> Table {
        self.table.clone()
    }
    fn data(&self) -> Value {
        json!({ "lines": self.lines, "count": self.lines.len() })
    }
}

// ---------------------------------------------------------------------------
// 16.6 Workers
// ---------------------------------------------------------------------------

pub async fn workers(
    config: &Config,
    action: &WorkerCommands,
    format: OutputFormat,
) -> Result<Option<String>> {
    let client = crate::authenticated_client(config)?;

    let view = match action {
        WorkerCommands::List {
            status,
            limit,
            cursor,
        } => {
            let path = format!(
                "/workers{}",
                query(&[
                    ("status", status.clone()),
                    (
                        "limit",
                        limit.map(|l| l.to_string()).filter(|_| limit.is_some()),
                    ),
                    ("cursor", cursor.clone()),
                ])
            );
            let data = client.get(&path).await?;
            Renderable::List(list_from(
                &["ID", "NAME", "STATUS", "DRAINING", "HEARTBEAT"],
                &["id", "name", "status", "draining", "last_heartbeat_at"],
                data,
            ))
        }
        WorkerCommands::Get { id } => {
            let data = client.get(&format!("/workers/{id}")).await?;
            Renderable::Detail(detail("worker", data))
        }
        WorkerCommands::Drain { id } => {
            let data = client
                .post(&format!("/workers/{id}/drain"), json!({}))
                .await?;
            Renderable::Detail(detail("worker draining", data))
        }
        WorkerCommands::Revoke { id } => {
            let data = client
                .post(&format!("/workers/{id}/revoke"), json!({}))
                .await?;
            Renderable::Detail(detail("worker revoked", data))
        }
    };

    Ok(Some(render(&view, format)))
}

// ---------------------------------------------------------------------------
// 16.7 Queues
// ---------------------------------------------------------------------------

pub async fn queues(
    config: &Config,
    action: &QueueCommands,
    format: OutputFormat,
) -> Result<Option<String>> {
    let client = crate::authenticated_client(config)?;

    let view = match action {
        QueueCommands::List => {
            let data = client.get("/queues").await?;
            Renderable::List(list_from(
                &["ID", "NAME", "MAX CONCURRENCY"],
                &["id", "name", "max_concurrency"],
                data,
            ))
        }
        QueueCommands::Get { id } => {
            let data = client.get(&format!("/queues/{id}")).await?;
            Renderable::Detail(detail("queue", data))
        }
        QueueCommands::Create {
            name,
            max_concurrency,
        } => {
            let mut body = json!({ "name": name });
            if let Some(limit) = max_concurrency {
                body["max_concurrency"] = json!(limit);
            }
            let data = client.post("/queues", body).await?;
            Renderable::Detail(detail("queue created", data))
        }
        QueueCommands::Pause { id } => {
            let data = client
                .post(&format!("/queues/{id}/pause"), json!({}))
                .await?;
            Renderable::Detail(detail("queue paused", data))
        }
        QueueCommands::Resume { id } => {
            let data = client
                .post(&format!("/queues/{id}/resume"), json!({}))
                .await?;
            Renderable::Detail(detail("queue resumed", data))
        }
    };

    Ok(Some(render(&view, format)))
}
