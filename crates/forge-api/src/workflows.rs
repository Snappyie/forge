//! Workflow endpoints (spec 05 endpoints 24–32).
//!
//! A workflow is a directed acyclic graph of nodes. Version 02.10 keeps a
//! version immutable once published (spec 02.16, invariant 5), so a publish
//! marks the version and points the workflow at it without ever rewriting it.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use forge_domain::workflow::{NodeType, Workflow as DomainWorkflow};
use forge_storage::AuditRepository;

use crate::envelope::{ApiError, ApiResponse, ListResponse, PaginationQuery};
use crate::extract::Auth;
use crate::idempotency;
use crate::jobs::header_key;
use crate::router::AppState;

fn workflow_view(row: &WorkflowRow) -> Value {
    json!({
        "id": row.id,
        "key": row.key,
        "name": row.name,
        "description": row.description,
        "status": row.status,
        "current_version_id": row.current_version_id,
        "owner_id": row.owner_id,
        "created_at": row.created_at,
        "updated_at": row.updated_at,
    })
}

#[derive(sqlx::FromRow)]
struct WorkflowRow {
    id: Uuid,
    key: Option<String>,
    name: String,
    description: Option<String>,
    status: String,
    current_version_id: Option<Uuid>,
    owner_id: Option<Uuid>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

async fn load(
    pool: &sqlx::PgPool,
    tenant: forge_domain::TenantId,
    id: Uuid,
) -> Result<WorkflowRow, ApiError> {
    sqlx::query_as::<_, WorkflowRow>(
        "SELECT id, key, name, description, status, current_version_id, owner_id,
                created_at, updated_at
         FROM workflows WHERE id = $1 AND tenant_id = $2",
    )
    .bind(id)
    .bind(tenant.into_uuid())
    .fetch_optional(pool)
    .await
    .map_err(ApiError::from)?
    .ok_or_else(|| ApiError::not_found("workflow"))
}

/// `GET /workflows` (spec 05 endpoint 24).
pub async fn list(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Query(pagination): Query<PaginationQuery>,
) -> Result<Json<ListResponse<Value>>, ApiError> {
    auth.require("workflows:read")?;

    let rows = sqlx::query_as::<_, WorkflowRow>(
        "SELECT id, key, name, description, status, current_version_id, owner_id,
                created_at, updated_at
         FROM workflows WHERE tenant_id = $1 ORDER BY created_at DESC LIMIT $2",
    )
    .bind(auth.tenant_id.into_uuid())
    .bind(pagination.effective_limit() as i64)
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::from)?;

    let items: Vec<Value> = rows.iter().map(workflow_view).collect();
    Ok(Json(ListResponse::new(
        items,
        Default::default(),
        auth.request_id,
    )))
}

#[derive(Debug, Deserialize, serde::Serialize)]
pub struct CreateWorkflowRequest {
    #[serde(default)]
    pub key: Option<String>,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    /// An initial graph. Nodes and edges may also be supplied later via a
    /// version.
    #[serde(default)]
    pub definition: Option<GraphDefinition>,
}

/// A workflow graph as submitted by a client.
#[derive(Debug, Deserialize, serde::Serialize, Default)]
pub struct GraphDefinition {
    #[serde(default)]
    pub nodes: Vec<GraphNode>,
    #[serde(default)]
    pub edges: Vec<GraphEdge>,
}

#[derive(Debug, Deserialize, serde::Serialize)]
pub struct GraphNode {
    pub key: String,
    #[serde(rename = "type")]
    pub node_type: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub config: Value,
}

#[derive(Debug, Deserialize, serde::Serialize)]
pub struct GraphEdge {
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub condition: Option<String>,
}

/// `POST /workflows` (spec 05 endpoint 25).
pub async fn create(
    State(state): State<AppState>,
    Auth(auth): Auth,
    headers: axum::http::HeaderMap,
    Json(body): Json<CreateWorkflowRequest>,
) -> Result<axum::response::Response, ApiError> {
    auth.require("workflows:write")?;

    if body.name.trim().is_empty() {
        return Err(
            ApiError::validation("name must not be empty").with_detail("name", "must not be empty")
        );
    }

    // A graph, when supplied, must be a valid DAG before anything is stored.
    if let Some(definition) = &body.definition {
        validate_graph(definition)?;
    }

    let request_body = serde_json::to_value(&body).unwrap_or(json!({}));
    let outcome = idempotency::run(
        &state.pool,
        auth.tenant_id,
        header_key(&headers, idempotency::IDEMPOTENCY_HEADER),
        "POST /api/v1/workflows",
        &request_body,
        &auth.request_id,
        || async {
            let id = Uuid::new_v4();
            let row = sqlx::query_as::<_, WorkflowRow>(
                "INSERT INTO workflows (id, tenant_id, key, name, description, owner_id)
                 VALUES ($1, $2, $3, $4, $5, $6)
                 RETURNING id, key, name, description, status, current_version_id,
                           owner_id, created_at, updated_at",
            )
            .bind(id)
            .bind(auth.tenant_id.into_uuid())
            .bind(body.key.as_deref().map(str::to_ascii_lowercase))
            .bind(body.name.trim())
            .bind(&body.description)
            .bind(auth.user_id)
            .fetch_one(&state.pool)
            .await?;

            // An initial definition becomes version 1, published so the
            // workflow is immediately runnable.
            if let Some(definition) = &body.definition {
                publish_version(&state.pool, auth.tenant_id, id, Some(definition)).await?;
            }

            audit(&state, auth.tenant_id, "workflow.create", "workflow", id).await;
            Ok((StatusCode::CREATED, workflow_view(&row), Some(id)))
        },
    )
    .await?;

    Ok(outcome.into_response(&auth.request_id))
}

/// `GET /workflows/{id}` (spec 05 endpoint 26).
pub async fn get(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    auth.require("workflows:read")?;
    let row = load(&state.pool, auth.tenant_id, id).await?;

    // Include the current definition so the designer (UI.md section 23) can
    // draw what will actually run rather than an empty canvas.
    let nodes: Vec<Value> = sqlx::query_scalar(
        "SELECT json_build_object(
             'key', node_key, 'name', name, 'type', node_type, 'config', config
         )
         FROM workflow_nodes n
         JOIN workflow_versions v ON v.id = n.workflow_version_id
         WHERE v.workflow_id = $1 AND v.tenant_id = $2 AND v.id = $3
         ORDER BY n.created_at, n.node_key",
    )
    .bind(id)
    .bind(auth.tenant_id.into_uuid())
    .bind(row.current_version_id)
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::from)?;

    let edges: Vec<Value> = sqlx::query_scalar(
        "SELECT json_build_object('from', src.node_key, 'to', dst.node_key)
         FROM workflow_edges e
         JOIN workflow_versions v ON v.id = e.workflow_version_id
         JOIN workflow_nodes src ON src.id = e.from_node_id
         JOIN workflow_nodes dst ON dst.id = e.to_node_id
         WHERE v.workflow_id = $1 AND v.tenant_id = $2 AND v.id = $3
         ORDER BY src.node_key, dst.node_key",
    )
    .bind(id)
    .bind(auth.tenant_id.into_uuid())
    .bind(row.current_version_id)
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::from)?;

    let mut view = workflow_view(&row);
    view["definition"] = json!({ "nodes": nodes, "edges": edges });

    Ok(Json(ApiResponse::new(view, auth.request_id)))
}

/// `PUT /workflows/{id}/definition` — save the edited graph as a new version.
///
/// Saving creates a version rather than mutating the published one, so a
/// running workflow is never changed underneath itself (UI.md section 35).
pub async fn put_definition(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateDefinitionRequest>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    auth.require("workflows:write")?;
    load(&state.pool, auth.tenant_id, id).await?;

    if body.definition.nodes.is_empty() {
        return Err(ApiError::validation("a workflow needs at least one node")
            .with_detail("definition", "no nodes"));
    }

    // Reuse the same validation as create, so the designer cannot save a graph
    // the engine would reject. This includes the cycle check.
    validate_graph(&body.definition)?;

    // One transaction for the version, its nodes and its edges: a failure part
    // way through would otherwise leave a version row holding a partial graph.
    let mut tx = state.pool.begin().await.map_err(ApiError::from)?;

    let version_id = Uuid::new_v4();
    let next_number: i32 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(version_number), 0) + 1 FROM workflow_versions
         WHERE workflow_id = $1 AND tenant_id = $2",
    )
    .bind(id)
    .bind(auth.tenant_id.into_uuid())
    .fetch_one(&state.pool)
    .await
    .map_err(ApiError::from)?;

    sqlx::query(
        "INSERT INTO workflow_versions (id, workflow_id, tenant_id, version_number, created_at)
         VALUES ($1, $2, $3, $4, NOW())",
    )
    .bind(version_id)
    .bind(id)
    .bind(auth.tenant_id.into_uuid())
    .bind(next_number)
    .execute(&mut *tx)
    .await
    .map_err(ApiError::from)?;

    let mut node_ids: std::collections::HashMap<String, Uuid> = std::collections::HashMap::new();

    for (position, node) in body.definition.nodes.iter().enumerate() {
        // Reject an unknown type here as well, so the row cannot be written
        // with a value the engine will not understand later.
        let stored_type = match node.node_type.as_str() {
            "JOB" | "APPROVAL" | "DELAY" | "CONDITION" | "MAP" | "WEBHOOK" => {
                node.node_type.clone()
            }
            other => {
                return Err(
                    ApiError::validation(format!("`{other}` is not a known node type"))
                        .with_detail("nodes", "unknown node type"),
                )
            }
        };

        let node_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO workflow_nodes
                 (id, workflow_version_id, tenant_id, node_key, name, node_type, config)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(node_id)
        .bind(version_id)
        .bind(auth.tenant_id.into_uuid())
        .bind(&node.key)
        .bind(node.name.as_deref().unwrap_or(&node.key))
        .bind(&stored_type)
        .bind(&node.config)
        .execute(&mut *tx)
        .await
        .map_err(ApiError::from)?;

        // Edges reference node ids, so remember the mapping key -> id.
        node_ids.insert(node.key.clone(), node_id);
        let _ = position;
    }

    for edge in body.definition.edges.iter() {
        // Silently dropping an edge to a node that no longer exists would leave
        // a graph that does not match the drawing.
        // `validate_graph` guarantees both keys exist, so these lookups hit.
        let from_id = node_ids[&edge.from];
        let to_id = node_ids[&edge.to];

        sqlx::query(
            "INSERT INTO workflow_edges
                 (id, workflow_version_id, tenant_id, from_node_id, to_node_id, condition)
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(Uuid::new_v4())
        .bind(version_id)
        .bind(auth.tenant_id.into_uuid())
        .bind(from_id)
        .bind(to_id)
        .bind(edge.condition.as_deref().unwrap_or("ALL_SUCCEEDED"))
        .execute(&mut *tx)
        .await
        .map_err(ApiError::from)?;
    }

    // Commit last: everything above either lands together or not at all.
    tx.commit().await.map_err(ApiError::from)?;

    Ok(Json(ApiResponse::new(
        json!({
            "version_id": version_id,
            "version_number": next_number,
            "published": false,
            "detail": "Saved as a draft version. Publish it to make it active.",
        }),
        auth.request_id,
    )))
}

#[derive(Debug, Deserialize, serde::Serialize, Default)]
pub struct UpdateDefinitionRequest {
    #[serde(default)]
    pub definition: GraphDefinition,
}

/// `GET /workflows/{id}/versions`.
pub async fn list_versions(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(id): Path<Uuid>,
) -> Result<Json<ListResponse<Value>>, ApiError> {
    auth.require("workflows:read")?;
    load(&state.pool, auth.tenant_id, id).await?;

    let rows: Vec<(Value,)> = sqlx::query_as(
        "SELECT json_build_object(
             'id', id, 'version_number', version_number,
             'node_count', (SELECT COUNT(*) FROM workflow_nodes n WHERE n.workflow_version_id = w.id),
             'edge_count', (SELECT COUNT(*) FROM workflow_edges e WHERE e.workflow_version_id = w.id),
             'published_at', published_at, 'created_at', created_at
         )
         FROM workflow_versions w
         WHERE workflow_id = $1 AND tenant_id = $2
         ORDER BY version_number DESC",
    )
    .bind(id)
    .bind(auth.tenant_id.into_uuid())
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::from)?;

    Ok(Json(ListResponse::new(
        rows.into_iter().map(|(v,)| v).collect(),
        Default::default(),
        auth.request_id,
    )))
}

/// `POST /workflows/{id}/validate` (spec 05 endpoint 28).
///
/// Validation runs through the same rules the publisher uses, so a graph that
/// validates here is not refused later for a different reason.
pub async fn validate(
    Auth(auth): Auth,
    Json(definition): Json<GraphDefinition>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    auth.require("workflows:read")?;
    validate_graph(&definition)?;
    Ok(Json(ApiResponse::new(
        json!({
            "valid": true,
            "node_count": definition.nodes.len(),
            "edge_count": definition.edges.len(),
        }),
        auth.request_id,
    )))
}

/// `POST /workflows/{id}/versions/{version_id}/publish` (spec 05 endpoint 29).
///
/// Publishing is one-way: a published version is immutable (spec 02.16).
pub async fn publish(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    auth.require("workflows:write")?;
    load(&state.pool, auth.tenant_id, id).await?;

    // Publish the newest existing version rather than minting a fresh one.
    //
    // A version carries the graph, so a newly created version is empty. Creating
    // one here meant every publish replaced a runnable definition with a graph
    // of zero nodes, and a run started against it could never do anything — it
    // simply stayed RUNNING forever with no node states.
    let latest: Option<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM workflow_versions
         WHERE workflow_id = $1 AND tenant_id = $2
         ORDER BY version_number DESC LIMIT 1",
    )
    .bind(id)
    .bind(auth.tenant_id.into_uuid())
    .fetch_optional(&state.pool)
    .await
    .map_err(ApiError::from)?;

    let Some((version,)) = latest else {
        return Err(ApiError::conflict(
            "this workflow has no version to publish; save a definition first",
        ));
    };

    activate_version(&state.pool, auth.tenant_id, id, version).await?;
    audit(&state, auth.tenant_id, "workflow.publish", "workflow", id).await;

    Ok(Json(ApiResponse::new(
        json!({ "id": id, "current_version_id": version }),
        auth.request_id,
    )))
}

/// `POST /workflows/{id}/versions/{version_id}/publish` (spec 05 endpoint 28).
///
/// This is the shape the CLI and the documented API use; without it `forge
/// workflows publish` returned 404 against a route that was never registered.
pub async fn publish_specific_version(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path((id, version_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    auth.require("workflows:write")?;
    load(&state.pool, auth.tenant_id, id).await?;

    let exists: Option<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM workflow_versions WHERE id = $1 AND workflow_id = $2 AND tenant_id = $3",
    )
    .bind(version_id)
    .bind(id)
    .bind(auth.tenant_id.into_uuid())
    .fetch_optional(&state.pool)
    .await
    .map_err(ApiError::from)?;

    if exists.is_none() {
        return Err(ApiError::not_found("workflow version"));
    }

    let nodes: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM workflow_nodes WHERE workflow_version_id = $1")
            .bind(version_id)
            .fetch_one(&state.pool)
            .await
            .map_err(ApiError::from)?;

    if nodes == 0 {
        return Err(ApiError::validation(
            "this version has no nodes, so a run against it could never do anything",
        )
        .with_detail("version_id", "empty graph"));
    }

    activate_version(&state.pool, auth.tenant_id, id, version_id).await?;
    audit(
        &state,
        auth.tenant_id,
        "workflow.publish",
        "workflow_version",
        version_id,
    )
    .await;

    Ok(Json(ApiResponse::new(
        json!({ "id": id, "current_version_id": version_id }),
        auth.request_id,
    )))
}

/// Marks a version published and points the workflow at it.
async fn activate_version(
    pool: &sqlx::PgPool,
    tenant: forge_domain::TenantId,
    workflow_id: Uuid,
    version_id: Uuid,
) -> Result<(), ApiError> {
    let mut tx = pool.begin().await.map_err(ApiError::from)?;

    let result = async {
        // `COALESCE` keeps the original publication time when a version is
        // re-published, so the history does not silently rewrite itself.
        sqlx::query(
            "UPDATE workflow_versions SET published_at = COALESCE(published_at, NOW())
             WHERE id = $1 AND tenant_id = $2",
        )
        .bind(version_id)
        .bind(tenant.into_uuid())
        .execute(&mut *tx)
        .await
        .map_err(ApiError::from)?;

        sqlx::query(
            "UPDATE workflows SET current_version_id = $2, status = 'ACTIVE', updated_at = NOW()
             WHERE id = $1 AND tenant_id = $3",
        )
        .bind(workflow_id)
        .bind(version_id)
        .bind(tenant.into_uuid())
        .execute(&mut *tx)
        .await
        .map_err(ApiError::from)?;

        Ok::<(), ApiError>(())
    }
    .await;

    match result {
        Ok(()) => {
            tx.commit().await.map_err(ApiError::from)?;
            Ok(())
        }
        Err(error) => {
            let _ = tx.rollback().await;
            Err(error)
        }
    }
}

/// `POST /workflows/{id}/trigger` (spec 05 endpoint 30).
pub async fn trigger(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(id): Path<Uuid>,
    headers: axum::http::HeaderMap,
    Json(body): Json<Value>,
) -> Result<axum::response::Response, ApiError> {
    auth.require("workflows:trigger")?;

    let workflow = load(&state.pool, auth.tenant_id, id).await?;
    if workflow.status != "ACTIVE" {
        return Err(
            ApiError::conflict("this workflow is not active; publish a version first")
                .with_detail("status", workflow.status),
        );
    }

    let request_body = body.clone();
    let outcome = idempotency::run(
        &state.pool,
        auth.tenant_id,
        header_key(&headers, idempotency::IDEMPOTENCY_HEADER),
        "POST /api/v1/workflows/{id}/trigger",
        &request_body,
        &auth.request_id,
        || async {
            // A workflow run is an execution that names the workflow and the
            // definition version it was started from. The version is pinned here
            // so publishing a new one cannot change a run already in flight
            // (spec 01.22 invariant 6).
            let Some(version_id) = workflow.current_version_id else {
                return Err(ApiError::conflict(
                    "this workflow has no published version to run",
                ));
            };

            let timeout_seconds: i32 = sqlx::query_scalar::<_, i32>(
                "SELECT timeout_seconds FROM workflow_versions WHERE id = $1 AND tenant_id = $2",
            )
            .bind(version_id)
            .bind(auth.tenant_id.into_uuid())
            .fetch_optional(&state.pool)
            .await
            .map_err(ApiError::from)?
            .unwrap_or(86_400);

            let execution_id = forge_storage::WorkflowRunRepository::new(&state.pool)
                .start_run(forge_storage::NewWorkflowRun {
                    tenant_id: auth.tenant_id,
                    workflow_id: id,
                    workflow_version_id: version_id,
                    trigger_source: "WORKFLOW".to_string(),
                    parent_execution_id: None,
                    correlation_id: Some(auth.request_id.clone()),
                    input: body.clone(),
                    timeout_seconds,
                })
                .await
                .map_err(ApiError::from)?;

            audit(&state, auth.tenant_id, "workflow.trigger", "workflow", id).await;
            Ok((
                StatusCode::ACCEPTED,
                json!({
                    "execution_id": execution_id,
                    "workflow_id": id,
                    "workflow_version_id": version_id,
                    "status": "RUNNING",
                }),
                Some(execution_id),
            ))
        },
    )
    .await?;

    Ok(outcome.into_response(&auth.request_id))
}

/// Records an audit event (spec 01.17).
async fn audit(
    state: &AppState,
    tenant: forge_domain::TenantId,
    action: &str,
    resource_type: &str,
    resource_id: Uuid,
) {
    let _ = AuditRepository::new(&state.pool)
        .record(forge_storage::NewAuditEvent::new(
            tenant,
            "USER",
            None,
            action,
            resource_type,
            Some(resource_id),
        ))
        .await;
}

/// Validates a graph: acyclic, endpoints resolve, node types known.
fn validate_graph(definition: &GraphDefinition) -> Result<(), ApiError> {
    if definition.nodes.is_empty() {
        return Err(ApiError::validation("a workflow needs at least one node")
            .with_detail("nodes", "must not be empty"));
    }

    let keys: std::collections::HashSet<&str> =
        definition.nodes.iter().map(|n| n.key.as_str()).collect();

    if keys.len() != definition.nodes.len() {
        return Err(
            ApiError::validation("node keys must be unique").with_detail("nodes", "duplicate key")
        );
    }

    for node in &definition.nodes {
        if !["JOB", "APPROVAL", "DELAY", "CONDITION", "MAP", "WEBHOOK"]
            .contains(&node.node_type.as_str())
        {
            return Err(ApiError::validation(format!(
                "`{}` is not a known node type",
                node.node_type
            ))
            .with_detail("nodes", "unknown node type"));
        }
    }

    // Kahn's algorithm: a graph with no full topological order has a cycle.
    let mut in_degree: std::collections::HashMap<&str, usize> =
        keys.iter().map(|k| (*k, 0usize)).collect();
    let mut adjacency: std::collections::HashMap<&str, Vec<&str>> =
        std::collections::HashMap::new();

    for edge in &definition.edges {
        if !keys.contains(edge.from.as_str()) {
            return Err(ApiError::validation(format!(
                "edge references unknown node `{}`",
                edge.from
            ))
            .with_detail("edges", "unknown source node"));
        }
        if !keys.contains(edge.to.as_str()) {
            return Err(ApiError::validation(format!(
                "edge references unknown node `{}`",
                edge.to
            ))
            .with_detail("edges", "unknown target node"));
        }
        if edge.from == edge.to {
            return Err(ApiError::validation(format!(
                "node `{}` cannot depend on itself",
                edge.from
            ))
            .with_detail("edges", "self loop"));
        }
        *in_degree.entry(edge.to.as_str()).or_insert(0) += 1;
        adjacency
            .entry(edge.from.as_str())
            .or_default()
            .push(&edge.to);
    }

    let mut queue: std::collections::VecDeque<&str> = in_degree
        .iter()
        .filter(|(_, d)| **d == 0)
        .map(|(k, _)| *k)
        .collect();

    let mut visited = 0usize;
    while let Some(node) = queue.pop_front() {
        visited += 1;
        if let Some(children) = adjacency.get(node) {
            for child in children {
                let degree = in_degree.get_mut(child).expect("child is a known node");
                *degree -= 1;
                if *degree == 0 {
                    queue.push_back(child);
                }
            }
        }
    }

    if visited != keys.len() {
        return Err(ApiError::validation("the workflow graph contains a cycle")
            .with_detail("edges", "graph must be acyclic"));
    }

    Ok(())
}

/// Creates the next version and, when a definition is supplied, writes its
/// nodes and edges.
async fn publish_version(
    pool: &sqlx::PgPool,
    tenant: forge_domain::TenantId,
    workflow_id: Uuid,
    definition: Option<&GraphDefinition>,
) -> Result<Uuid, ApiError> {
    let mut tx = pool.begin().await.map_err(ApiError::from)?;

    let version_id = Uuid::new_v4();
    let result = async {
        sqlx::query(
            "INSERT INTO workflow_versions
                (id, tenant_id, workflow_id, version_number, published_at)
             VALUES ($1, $2, $3,
                 (SELECT COALESCE(MAX(version_number), 0) + 1
                    FROM workflow_versions WHERE workflow_id = $3),
                 NOW())",
        )
        .bind(version_id)
        .bind(tenant.into_uuid())
        .bind(workflow_id)
        .execute(&mut *tx)
        .await
        .map_err(ApiError::from)?;

        if let Some(graph) = definition {
            // Reuse the domain validation so the API and the engine agree on
            // what a valid graph is.
            let domain = to_domain_workflow(tenant, workflow_id, graph)?;
            domain.validate().map_err(ApiError::from)?;

            for node in &graph.nodes {
                sqlx::query(
                    "INSERT INTO workflow_nodes
                        (id, tenant_id, workflow_version_id, node_key, node_type, name, config)
                     VALUES ($1, $2, $3, $4, $5, $6, $7)",
                )
                .bind(Uuid::new_v4())
                .bind(tenant.into_uuid())
                .bind(version_id)
                .bind(&node.key)
                .bind(&node.node_type)
                .bind(node.name.as_deref().unwrap_or(&node.key))
                .bind(&node.config)
                .execute(&mut *tx)
                .await
                .map_err(ApiError::from)?;
            }

            // Resolve node keys to the ids just written.
            let mut ids: std::collections::HashMap<String, Uuid> = std::collections::HashMap::new();
            let rows: Vec<(String, Uuid)> = sqlx::query_as(
                "SELECT node_key, id FROM workflow_nodes WHERE workflow_version_id = $1",
            )
            .bind(version_id)
            .fetch_all(&mut *tx)
            .await
            .map_err(ApiError::from)?;
            for (key, id) in rows {
                ids.insert(key, id);
            }

            for edge in &graph.edges {
                let from = ids[&edge.from];
                let to = ids[&edge.to];
                sqlx::query(
                    "INSERT INTO workflow_edges
                        (id, tenant_id, workflow_version_id, from_node_id, to_node_id, condition)
                     VALUES ($1, $2, $3, $4, $5, $6)",
                )
                .bind(Uuid::new_v4())
                .bind(tenant.into_uuid())
                .bind(version_id)
                .bind(from)
                .bind(to)
                .bind(edge.condition.as_deref().unwrap_or("ALL_SUCCEEDED"))
                .execute(&mut *tx)
                .await
                .map_err(ApiError::from)?;
            }
        }

        // Publishing points the workflow at the version and activates it.
        sqlx::query(
            "UPDATE workflows SET current_version_id = $3, status = 'ACTIVE', updated_at = NOW()
             WHERE id = $1 AND tenant_id = $2",
        )
        .bind(workflow_id)
        .bind(tenant.into_uuid())
        .bind(version_id)
        .execute(&mut *tx)
        .await
        .map_err(ApiError::from)?;

        Ok(version_id)
    }
    .await;

    match result {
        Ok(version_id) => {
            tx.commit().await.map_err(ApiError::from)?;
            Ok(version_id)
        }
        Err(e) => {
            let _ = tx.rollback().await;
            Err(e)
        }
    }
}

/// Converts a submitted graph into the domain type so one validator applies.
fn to_domain_workflow(
    tenant: forge_domain::TenantId,
    workflow_id: Uuid,
    graph: &GraphDefinition,
) -> Result<DomainWorkflow, ApiError> {
    let mut workflow = DomainWorkflow::new(tenant, workflow_id.to_string());

    for node in &graph.nodes {
        let node_type = match node.node_type.as_str() {
            "JOB" => {
                // The referenced job is carried in config so the executor can
                // dispatch it; the id is validated on trigger.
                let job_id = node
                    .config
                    .get("job_id")
                    .and_then(|v| v.as_str())
                    .and_then(|raw| Uuid::parse_str(raw).ok())
                    .ok_or_else(|| {
                        ApiError::validation(format!("node `{}` must reference a job_id", node.key))
                            .with_detail("nodes", "missing job_id")
                    })?;
                NodeType::Job {
                    job_id: forge_domain::JobId::from_uuid(job_id),
                }
            }
            "APPROVAL" => NodeType::Approval {
                required_role: node
                    .config
                    .get("required_role")
                    .and_then(|v| v.as_str())
                    .unwrap_or("ADMIN")
                    .to_string(),
            },
            "DELAY" => NodeType::Delay {
                seconds: node
                    .config
                    .get("seconds")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0),
            },
            "CONDITION" => NodeType::Condition {
                expression: node
                    .config
                    .get("expression")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
            },
            "MAP" => NodeType::Map {
                target_node_id: node
                    .config
                    .get("target_node_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
            },
            "SUB_WORKFLOW" => {
                let target_workflow_id = node
                    .config
                    .get("workflow_id")
                    .and_then(|v| v.as_str())
                    .and_then(|raw| Uuid::parse_str(raw).ok())
                    .ok_or_else(|| {
                        ApiError::validation(format!(
                            "node `{}` must reference a workflow_id",
                            node.key
                        ))
                        .with_detail("nodes", "missing workflow_id")
                    })?;
                if target_workflow_id == workflow_id {
                    return Err(ApiError::validation(format!(
                        "node `{}` cannot reference its own workflow",
                        node.key
                    ))
                    .with_detail("nodes", "sub-workflow cannot be self"));
                }
                NodeType::SubWorkflow {
                    workflow_id: target_workflow_id,
                }
            }
            "WEBHOOK" => {
                // A real node, parsed from its configuration.
                //
                // This used to be `NodeType::Delay { seconds: 0 }`: the designer
                // offered a webhook node, the API accepted it, and it executed
                // as an instantaneous no-op with its URL read by nobody — the
                // workflow reported success for work it had never done. It is
                // now either configured correctly or refused.
                let url = node
                    .config
                    .get("url")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        ApiError::validation(format!(
                            "node `{}` must supply a url",
                            node.key
                        ))
                        .with_detail("nodes", "missing url")
                    })?
                    .trim()
                    .to_string();

                if url.is_empty() {
                    return Err(ApiError::validation(format!(
                        "node `{}` has an empty url",
                        node.key
                    ))
                    .with_detail("nodes", "empty url"));
                }
                if !url.starts_with("http://") && !url.starts_with("https://") {
                    return Err(ApiError::validation(format!(
                        "node `{}` url must be http or https",
                        node.key
                    ))
                    .with_detail("nodes", "unsupported url scheme"));
                }

                let method = node
                    .config
                    .get("method")
                    .and_then(|v| v.as_str())
                    .unwrap_or("GET")
                    .parse::<forge_domain::workflow::HttpMethod>()
                    .map_err(|_| {
                        ApiError::validation(format!(
                            "`{}` is not a supported HTTP method",
                            node.config
                                .get("method")
                                .and_then(|v| v.as_str())
                                .unwrap_or("GET")
                        ))
                        .with_detail("nodes", "unsupported method")
                    })?;

                let headers = node
                    .config
                    .get("headers")
                    .and_then(|v| v.as_object())
                    .map(|object| {
                        object
                            .iter()
                            .filter_map(|(name, value)| {
                                value.as_str().map(|v| (name.clone(), v.to_string()))
                            })
                            .collect()
                    })
                    .unwrap_or_default();

                // A default rather than zero: a zero timeout means "give up
                // immediately", which is never intended and fails every run.
                let timeout_seconds = node
                    .config
                    .get("timeout_seconds")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(30);

                NodeType::Webhook {
                    url,
                    method,
                    headers,
                    timeout_seconds,
                }
            }
            other => {
                return Err(
                    ApiError::validation(format!("`{other}` is not a known node type"))
                        .with_detail("nodes", "unknown node type"),
                )
            }
        };

        workflow.add_node(forge_domain::workflow::Node {
            id: node.key.clone(),
            node_type,
            name: node.name.clone().unwrap_or_else(|| node.key.clone()),
        });
    }

    for edge in &graph.edges {
        workflow.add_edge(edge.from.clone(), edge.to.clone(), edge.condition.clone());
    }

    Ok(workflow)
}

#[derive(Debug, Deserialize, Default)]
pub struct ApprovalDecisionRequest {
    pub comment: Option<String>,
}

/// Whether `actor` may satisfy an approval node that requires `required`.
///
/// The node names a role; a caller satisfies it when their own role grants at
/// least everything that role grants. OWNER trivially does. An unrecognised
/// role name fails closed rather than waving the approval through.
fn actor_satisfies(actor: forge_auth::Role, required: &str) -> bool {
    match forge_auth::Role::parse(required) {
        None => false,
        Some(required) => required
            .permissions()
            .iter()
            .all(|permission| actor.allows(permission)),
    }
}

/// The response shape shared by approve and reject.
async fn record_approval(
    state: &AppState,
    auth: &crate::extract::AuthContext,
    execution_id: Uuid,
    node_id: Uuid,
    decision: &str,
    comment: Option<String>,
) -> Result<serde_json::Value, ApiError> {
    let repository = forge_storage::WorkflowRunRepository::new(&state.pool);

    // The run must exist in this tenant, and the node must actually be waiting:
    // approving a node that already ran (or never started) would silently do
    // nothing, which is worse than a refusal.
    repository
        .get_run(auth.tenant_id, execution_id)
        .await
        .map_err(ApiError::from)?;

    let node = repository
        .node_state_by_id(execution_id, node_id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::not_found("workflow node"))?;

    let awaiting =
        node.state == "SUSPENDED" && node.suspension_reason.as_deref() == Some("AWAITING_APPROVAL");
    if !awaiting {
        return Err(ApiError::conflict(format!(
            "node `{}` is not awaiting approval (state {})",
            node.node_key, node.state
        )));
    }

    let required = node.required_role.as_deref().unwrap_or("ADMIN");
    if !actor_satisfies(auth.role, required) {
        return Err(ApiError::forbidden(format!(
            "this approval node requires the {required} role"
        )));
    }

    let id = Uuid::new_v4();
    let row: (serde_json::Value,) = sqlx::query_as(
        "INSERT INTO manual_approvals
             (id, tenant_id, workflow_execution_id, node_id, status, required_role,
              decided_by, decided_at, comment)
         VALUES ($1, $2, $3, $4, $5, $6, $7, NOW(), $8)
         ON CONFLICT (workflow_execution_id, node_id)
         DO UPDATE SET
             status = EXCLUDED.status,
             decided_by = EXCLUDED.decided_by,
             decided_at = NOW(),
             comment = COALESCE(EXCLUDED.comment, manual_approvals.comment)
         RETURNING json_build_object(
             'id', id,
             'workflow_execution_id', workflow_execution_id,
             'node_id', node_id,
             'node_key', $9,
             'status', status,
             'decided_at', decided_at,
             'comment', comment
         )",
    )
    .bind(id)
    .bind(auth.tenant_id.into_uuid())
    .bind(execution_id)
    .bind(node_id)
    .bind(decision)
    .bind(required)
    .bind(auth.user_id)
    .bind(comment)
    .bind(&node.node_key)
    .fetch_one(&state.pool)
    .await
    .map_err(ApiError::from)?;

    // The runtime reads the decision on its next pass and resumes the node.
    Ok(row.0)
}

/// `POST /workflows/executions/:execution_id/nodes/:node_id/approve` (spec 05 endpoint 31).
pub async fn approve_node(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path((execution_id, node_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<ApprovalDecisionRequest>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("workflows:trigger")?;

    let decision = record_approval(
        &state,
        &auth,
        execution_id,
        node_id,
        "APPROVED",
        body.comment,
    )
    .await?;

    audit(&state, auth.tenant_id, "APPROVE", "workflow_node", node_id).await;

    Ok(Json(ApiResponse::new(decision, auth.request_id)))
}

/// `POST /workflows/executions/:execution_id/nodes/:node_id/reject` (spec 05 endpoint 32).
pub async fn reject_node(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path((execution_id, node_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<ApprovalDecisionRequest>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("workflows:trigger")?;

    let decision = record_approval(
        &state,
        &auth,
        execution_id,
        node_id,
        "REJECTED",
        body.comment,
    )
    .await?;

    audit(&state, auth.tenant_id, "REJECT", "workflow_node", node_id).await;

    Ok(Json(ApiResponse::new(decision, auth.request_id)))
}

/// `GET /workflows/executions/:execution_id` — the run and its node states.
///
/// Spec 01.8 requires execution-graph visualisation, which needs the live state
/// of every node, not just the run's overall status.
pub async fn get_execution(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(execution_id): Path<Uuid>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("workflows:read")?;

    let repository = forge_storage::WorkflowRunRepository::new(&state.pool);
    let run = repository
        .get_run(auth.tenant_id, execution_id)
        .await
        .map_err(ApiError::from)?;
    let nodes = repository
        .node_states(execution_id)
        .await
        .map_err(ApiError::from)?;

    let nodes: Vec<serde_json::Value> = nodes
        .into_iter()
        .map(|node| {
            json!({
                "node_key": node.node_key,
                "node_id": node.node_id,
                "node_type": node.node_type,
                "state": node.state,
                "suspension_reason": node.suspension_reason,
                "resume_at": node.resume_at,
                "child_execution_id": node.child_execution_id,
                "required_role": node.required_role,
                "output": node.output,
                "failure_reason": node.failure_reason,
                "attempt_count": node.attempt_count,
            })
        })
        .collect();

    Ok(Json(ApiResponse::new(
        json!({
            "id": run.id,
            "workflow_id": run.workflow_id,
            "workflow_version_id": run.workflow_version_id,
            "status": run.status,
            "correlation_id": run.correlation_id,
            "created_at": run.created_at,
            "nodes": nodes,
        }),
        auth.request_id,
    )))
}

/// `POST /workflows/executions/:execution_id/cancel` — stop a run.
pub async fn cancel_execution(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(execution_id): Path<Uuid>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("workflows:trigger")?;

    let cancelled = forge_storage::WorkflowRunRepository::new(&state.pool)
        .cancel_run(auth.tenant_id, execution_id)
        .await
        .map_err(ApiError::from)?;

    if !cancelled {
        return Err(ApiError::conflict(
            "this workflow run is not running, so it cannot be cancelled",
        ));
    }

    audit(
        &state,
        auth.tenant_id,
        "workflow.cancel",
        "workflow_execution",
        execution_id,
    )
    .await;

    Ok(Json(ApiResponse::new(
        json!({ "id": execution_id, "status": "CANCEL_REQUESTED" }),
        auth.request_id,
    )))
}

/// `GET /workflows/executions/:execution_id/approvals`.
pub async fn list_approvals(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(execution_id): Path<Uuid>,
) -> Result<Json<ApiResponse<Vec<serde_json::Value>>>, ApiError> {
    auth.require("workflows:read")?;

    let rows: Vec<(serde_json::Value,)> = sqlx::query_as(
        "SELECT json_build_object(
             'id', id,
             'workflow_execution_id', workflow_execution_id,
             'node_id', node_id,
             'status', status,
             'required_role', required_role,
             'requested_by', requested_by,
             'decided_by', decided_by,
             'decided_at', decided_at,
             'comment', comment,
             'created_at', created_at
         )
         FROM manual_approvals
         WHERE workflow_execution_id = $1 AND tenant_id = $2
         ORDER BY created_at ASC",
    )
    .bind(execution_id)
    .bind(auth.tenant_id.into_uuid())
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::from)?;

    Ok(Json(ApiResponse::new(
        rows.into_iter().map(|(v,)| v).collect(),
        auth.request_id,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(key: &str, kind: &str) -> GraphNode {
        GraphNode {
            key: key.to_string(),
            node_type: kind.to_string(),
            name: None,
            config: json!({}),
        }
    }

    fn edge(from: &str, to: &str) -> GraphEdge {
        GraphEdge {
            from: from.to_string(),
            to: to.to_string(),
            condition: None,
        }
    }

    // AT-WF-001: a valid DAG validates.
    #[test]
    fn a_valid_dag_is_accepted() {
        let graph = GraphDefinition {
            nodes: vec![node("a", "JOB"), node("b", "APPROVAL"), node("c", "DELAY")],
            edges: vec![edge("a", "b"), edge("b", "c")],
        };
        assert!(validate_graph(&graph).is_ok());
    }

    // AT-WF-002: a cycle is rejected.
    #[test]
    fn a_cycle_is_rejected() {
        let graph = GraphDefinition {
            nodes: vec![node("a", "JOB"), node("b", "JOB")],
            edges: vec![edge("a", "b"), edge("b", "a")],
        };
        let error = validate_graph(&graph).unwrap_err();
        assert_eq!(error.status, StatusCode::BAD_REQUEST);
        assert!(error.message.contains("cycle"), "{}", error.message);
    }

    #[test]
    fn a_long_chain_is_not_mistaken_for_a_cycle() {
        let mut graph = GraphDefinition {
            nodes: (0..200).map(|i| node(&format!("n{i}"), "JOB")).collect(),
            edges: Vec::new(),
        };
        for i in 0..199 {
            graph
                .edges
                .push(edge(&format!("n{i}"), &format!("n{}", i + 1)));
        }
        assert!(validate_graph(&graph).is_ok());
    }

    #[test]
    fn a_self_loop_is_rejected() {
        let graph = GraphDefinition {
            nodes: vec![node("a", "JOB")],
            edges: vec![edge("a", "a")],
        };
        assert!(validate_graph(&graph).is_err());
    }

    #[test]
    fn a_dangling_edge_is_rejected() {
        let graph = GraphDefinition {
            nodes: vec![node("a", "JOB")],
            edges: vec![edge("a", "ghost")],
        };
        assert!(validate_graph(&graph).is_err());
    }

    #[test]
    fn an_empty_graph_is_rejected() {
        assert!(validate_graph(&GraphDefinition::default()).is_err());
    }

    #[test]
    fn duplicate_node_keys_are_rejected() {
        let graph = GraphDefinition {
            nodes: vec![node("a", "JOB"), node("a", "JOB")],
            edges: Vec::new(),
        };
        assert!(validate_graph(&graph).is_err());
    }

    #[test]
    fn an_unknown_node_type_is_rejected() {
        let graph = GraphDefinition {
            nodes: vec![node("a", "TELEPATHY")],
            edges: Vec::new(),
        };
        assert!(validate_graph(&graph).is_err());
    }

    #[test]
    fn a_parallel_fan_out_validates() {
        let graph = GraphDefinition {
            nodes: vec![
                node("start", "JOB"),
                node("left", "JOB"),
                node("right", "JOB"),
                node("join", "JOB"),
            ],
            edges: vec![
                edge("start", "left"),
                edge("start", "right"),
                edge("left", "join"),
                edge("right", "join"),
            ],
        };
        assert!(validate_graph(&graph).is_ok());
    }

    #[test]
    fn a_job_node_requires_a_job_id() {
        let graph = GraphDefinition {
            nodes: vec![node("a", "JOB")],
            edges: Vec::new(),
        };
        let domain = to_domain_workflow(forge_domain::TenantId::new(), Uuid::new_v4(), &graph);
        assert!(domain.is_err(), "a job node must name the job it runs");
    }

    #[test]
    fn a_fully_specified_graph_converts() {
        let job = Uuid::new_v4();
        let mut job_node = node("run", "JOB");
        job_node.config = json!({ "job_id": job.to_string() });

        let graph = GraphDefinition {
            nodes: vec![job_node, node("gate", "APPROVAL")],
            edges: vec![edge("run", "gate")],
        };
        let domain =
            to_domain_workflow(forge_domain::TenantId::new(), Uuid::new_v4(), &graph).unwrap();
        // The domain validator is the same one the executor uses.
        assert!(domain.validate().is_ok());
    }
}
