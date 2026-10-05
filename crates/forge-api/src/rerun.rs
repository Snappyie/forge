//! Partial rerun: resume a failed workflow run from where it failed.
//!
//! `redesign.md` §3 names "rerun only the failed portion" as a differentiator,
//! and §2.C lists "execution replay, historical inspection and partial reruns".
//! Neither existed: a failed run could only be inspected or triggered again from
//! the top.
//!
//! The distinction that matters is what is *preserved*. A full rerun re-executes
//! everything, including work that already succeeded — which for a pipeline that
//! charges a card or sends an email means doing it twice. This resets only the
//! nodes that did not succeed and leaves completed ones, and their outputs,
//! exactly as they were.

use axum::extract::{Path, State};
use axum::Json;
use chrono::Utc;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use forge_domain::TenantId;
use forge_storage::WorkflowRunRepository;

use crate::envelope::{ApiError, ApiResponse};
use crate::extract::Auth;
use crate::router::AppState;

/// Which nodes to reset.
#[derive(Debug, Deserialize)]
pub struct RerunRequest {
    /// Nodes to re-execute. Omitted means "every node that did not succeed",
    /// which is the case the feature exists for.
    #[serde(default)]
    pub nodes: Option<Vec<String>>,
    /// Re-run even nodes that succeeded. Needed when the *successful* work is
    /// what turned out to be wrong — an external system changed underneath a
    /// green pipeline. Off by default because it is the destructive direction.
    #[serde(default)]
    pub include_completed: bool,
}

/// A run row, carrying only what the rerun path reads.
///
/// Narrow deliberately: selecting columns nothing uses invites the situation
/// where a schema change to one of them looks like a change to this feature.
#[derive(sqlx::FromRow)]
struct RunRow {
    status: String,
    correlation_id: Option<String>,
}

/// A node state row, for deciding what to reset.
#[derive(sqlx::FromRow)]
struct NodeRow {
    node_key: String,
    state: String,
}

/// `POST /workflows/executions/{id}/rerun` — resume a failed run.
///
/// Reuses the *same* run row rather than creating a new one, so the history of
/// what failed and what was retried stays in one place instead of being split
/// across two runs an operator has to correlate.
pub async fn rerun_execution(
    State(state): State<AppState>,
    Path(execution_id): Path<Uuid>,
    Auth(auth): Auth,
    Json(body): Json<RerunRequest>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("workflows:write")?;

    let run: Option<RunRow> = sqlx::query_as(
        "SELECT status, correlation_id FROM executions WHERE id = $1 AND tenant_id = $2",
    )
    .bind(execution_id)
    .bind(auth.tenant_id.into_uuid())
    .fetch_optional(&state.pool)
    .await
    .map_err(ApiError::from)?;

    let Some(run) = run else {
        return Err(ApiError::not_found("workflow run"));
    };

    // Only a settled run can be resumed. Re-running a live run would race the
    // driver that is already advancing it.
    if !forge_storage::is_terminal_run_status(&run.status) {
        return Err(ApiError::conflict(format!(
            "this run is {}, not finished; cancel it before rerunning",
            run.status.to_lowercase()
        ))
        .with_detail("status", run.status.clone()));
    }

    // A workflow run is an execution with no job. A job execution reaching this
    // endpoint is a caller mistake, not a rerun.
    let is_workflow_run: bool = sqlx::query_scalar(
        "SELECT workflow_id IS NOT NULL FROM executions WHERE id = $1",
    )
    .bind(execution_id)
    .fetch_one(&state.pool)
    .await
    .map_err(ApiError::from)?;
    if !is_workflow_run {
        return Err(ApiError::validation(
            "this is a job execution, not a workflow run",
        )
        .with_detail("workflow_id", "null"));
    }

    let nodes: Vec<NodeRow> = sqlx::query_as(
        "SELECT node_key, state FROM workflow_node_states
          WHERE workflow_execution_id = $1 ORDER BY node_key",
    )
    .bind(execution_id)
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::from)?;

    // Decide what to reset. Anything not `COMPLETED` needs redoing: FAILED
    // obviously, and also PENDING and RUNNING, which a settled run leaves behind
    // for nodes downstream of the failure that never started.
    let requested: Option<Vec<String>> = body
        .nodes
        .as_ref()
        .map(|list| list.iter().map(|key| key.trim().to_string()).collect());

    if let Some(wanted) = &requested {
        let known: Vec<&str> = nodes.iter().map(|n| n.node_key.as_str()).collect();
        for key in wanted {
            if !known.contains(&key.as_str()) {
                return Err(ApiError::validation(format!(
                    "this run has no node `{key}`"
                ))
                .with_detail("nodes", "unknown node"));
            }
        }
    }

    /*
     * Decide what to reset, in one place.
     *
     * These were two chained filters, and the second silently undid the first:
     * `include_completed` was applied *after* the selection, so a completed node
     * could never be reset even when the caller explicitly asked for it. The
     * flag is a widening of the selection, not a filter over it.
     */
    let mut reset: Vec<&NodeRow> = nodes
        .iter()
        .filter(|node| match &requested {
            // An explicit list is the caller's decision, honoured as given.
            Some(wanted) => wanted.iter().any(|key| key == &node.node_key),
            // No explicit list: everything that did not succeed, plus
            // successful nodes only when the caller asked for them.
            None => node.state != "COMPLETED" || body.include_completed,
        })
        .collect();

    // Resetting a completed node discards its output, so the node's dependents
    // re-evaluate rather than reading a stale result.
    reset.sort_by(|a, b| a.node_key.cmp(&b.node_key));

    if reset.is_empty() {
        return Err(ApiError::validation(
            "nothing to rerun: every node in this run has already succeeded. \\
             Pass include_completed to redo successful work anyway.",
        )
        .with_detail("nodes", "all completed"));
    }

    let repository = WorkflowRunRepository::new(&state.pool);
    for node in &reset {
        // `reset_node` rather than `save_node_state`: the latter coalesces
        // `output`, so passing None would *preserve* the previous attempt's
        // result and a downstream condition would branch on it.
        repository
            .reset_node(execution_id, &node.node_key)
            .await
            .map_err(ApiError::from)?;
    }

    /*
     * Back to RUNNING so the driver advances it again.
     *
     * `correlation_id` is carried forward and suffixed rather than reset: the
     * rerun is the same logical operation, and an operator searching for it by
     * correlation id should find the original and its retries together.
     */
    let correlation_id = format!(
        "{}#rerun-{}",
        run.correlation_id.clone().unwrap_or_else(|| format!("run-{execution_id}")),
        Utc::now().timestamp()
    );

    sqlx::query(
        "UPDATE executions
            SET status = 'RUNNING',
                updated_at = NOW(),
                correlation_id = $2,
                started_at = COALESCE(started_at, NOW()),
                ended_at = NULL
          WHERE id = $1",
    )
    .bind(execution_id)
    .bind(&correlation_id)
    .execute(&state.pool)
    .await
    .map_err(ApiError::from)?;

    Ok(Json(ApiResponse::new(
        json!({
            "execution_id": execution_id,
            "reset_nodes": reset.iter().map(|n| &n.node_key).collect::<Vec<_>>(),
            "preserved_nodes": nodes
                .iter()
                .filter(|n| !reset.iter().any(|r| r.node_key == n.node_key))
                .map(|n| &n.node_key)
                .collect::<Vec<_>>(),
            "status": "RUNNING",
        }),
        auth.request_id,
    )))
}

/// Reads a run's node states, for the console's rerun dialog.
///
/// Returns which nodes would be reset for each candidate policy, so the dialog
/// can show the operator exactly what "rerun the failed part" means before they
/// commit to it.
pub async fn rerun_preview(
    State(state): State<AppState>,
    Path(execution_id): Path<Uuid>,
    Auth(auth): Auth,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("workflows:read")?;

    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT node_key, state FROM workflow_node_states
          WHERE workflow_execution_id = $1 AND tenant_id = $2
          ORDER BY node_key",
    )
    .bind(execution_id)
    .bind(auth.tenant_id.into_uuid())
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::from)?;

    if rows.is_empty() {
        return Err(ApiError::not_found("workflow run"));
    }

    let mut nodes = Vec::with_capacity(rows.len());
    for (key, state) in &rows {
        nodes.push(json!({
            "node_key": key,
            "state": state,
            // A settled run leaves PENDING and RUNNING behind for nodes
            // downstream of a failure, and those need redoing too.
            "would_rerun": state != "COMPLETED",
        }));
    }

    Ok(Json(ApiResponse::new(
        json!({ "nodes": nodes }),
        auth.request_id,
    )))
}

/// Reads a run's stored context, for inspection.
pub async fn run_context(
    State(state): State<AppState>,
    Path(execution_id): Path<Uuid>,
    Auth(auth): Auth,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("workflows:read")?;

    let row: Option<(serde_json::Value, String)> = sqlx::query_as(
        "SELECT workflow_context, status FROM executions
          WHERE id = $1 AND tenant_id = $2 AND workflow_id IS NOT NULL",
    )
    .bind(execution_id)
    .bind(auth.tenant_id.into_uuid())
    .fetch_optional(&state.pool)
    .await
    .map_err(ApiError::from)?;

    let Some((context, status)) = row else {
        return Err(ApiError::not_found("workflow run"));
    };

    let _ = TenantId::from_uuid(auth.tenant_id.into_uuid());
    Ok(Json(ApiResponse::new(
        json!({ "context": context, "status": status }),
        auth.request_id,
    )))
}
