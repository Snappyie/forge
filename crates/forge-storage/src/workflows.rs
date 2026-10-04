//! Workflow run persistence (spec 02.11, spec 10).
//!
//! A workflow run *is* an [`crate::ExecutionRow`]: the `executions` row carries
//! the workflow, the pinned definition version, the trigger source and the
//! engine's accumulated context, while [`workflow_node_states`] records where
//! each node got to.
//!
//! Persisting node progress is what makes the DAG engine in `forge-executor`
//! usable at all: the engine is a pure function of `(definition, node states,
//! context, now)`, so every pass over a run has to be able to rebuild those
//! inputs from the database rather than from process memory.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use forge_domain::workflow::{Node, NodeType, Workflow};
use forge_domain::TenantId;

use crate::error::{Result, StorageError};

/// One node's persisted progress inside a run.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct NodeStateRow {
    pub node_key: String,
    pub node_id: Option<Uuid>,
    pub node_type: String,
    pub state: String,
    pub suspension_reason: Option<String>,
    pub resume_at: Option<DateTime<Utc>>,
    pub child_execution_id: Option<Uuid>,
    pub required_role: Option<String>,
    pub output: Option<Value>,
    pub failure_reason: Option<String>,
    pub attempt_count: i32,
}

/// A run the runtime may advance.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct WorkflowRunRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub workflow_id: Uuid,
    pub workflow_version_id: Uuid,
    pub status: String,
    pub correlation_id: Option<String>,
    pub workflow_context: Value,
    pub created_at: DateTime<Utc>,
}

/// A node's ticket to be written back after a pass.
#[derive(Debug, Clone)]
pub struct NodeStateUpdate {
    pub node_key: String,
    pub state: String,
    pub suspension_reason: Option<String>,
    pub resume_at: Option<DateTime<Utc>>,
    pub child_execution_id: Option<Uuid>,
    pub required_role: Option<String>,
    pub output: Option<Value>,
    pub failure_reason: Option<String>,
    pub attempt_count: i32,
}

/// A child execution's current status, read back into the run.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ChildExecutionRow {
    pub node_key: String,
    pub child_execution_id: Uuid,
    pub status: String,
    pub output: Option<Value>,
    pub error_message: Option<String>,
}

/// The status of one dispatched execution.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ExecutionOutcomeRow {
    pub id: Uuid,
    pub status: String,
    pub output: Option<Value>,
    pub error_message: Option<String>,
}

/// Where a job node's work should be sent.
#[derive(Debug, Clone)]
pub struct JobDispatchTarget {
    pub job_version_id: Uuid,
    pub queue_id: Option<Uuid>,
    pub priority: String,
}

/// The job row a dispatch decision reads, so the query stays a named shape.
#[derive(Debug, Clone, sqlx::FromRow)]
struct JobDispatchRow {
    current_version_id: Option<Uuid>,
    latest_version_id: Option<Uuid>,
    default_queue_id: Option<Uuid>,
    priority: String,
}

/// A child execution to create for a workflow node.
#[derive(Debug, Clone)]
pub struct NewChildExecution {
    pub tenant_id: TenantId,
    pub job_id: Uuid,
    pub job_version_id: Uuid,
    pub queue_id: Option<Uuid>,
    /// The workflow run that owns this work.
    pub parent_execution_id: Uuid,
    pub correlation_id: Option<String>,
    pub input: Value,
    pub priority: String,
}

/// Where a sub-workflow node's work should be sent.
#[derive(Debug, Clone)]
pub struct WorkflowDispatchTarget {
    pub workflow_version_id: Uuid,
    pub timeout_seconds: i32,
}

/// Everything needed to start a run.
#[derive(Debug, Clone)]
pub struct NewWorkflowRun {
    pub tenant_id: TenantId,
    pub workflow_id: Uuid,
    pub workflow_version_id: Uuid,
    pub trigger_source: String,
    pub parent_execution_id: Option<Uuid>,
    pub correlation_id: Option<String>,
    pub input: Value,
    pub timeout_seconds: i32,
}

/// Repository for workflow runs and their node states.
pub struct WorkflowRunRepository<'a> {
    pool: &'a PgPool,
}

impl<'a> WorkflowRunRepository<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Loads a published definition as the domain graph the engine consumes.
    ///
    /// Edges are stored by node id and addressed by node *key*, so the join
    /// translates between them; the engine must never see a database id.
    pub async fn load_definition(
        &self,
        tenant_id: TenantId,
        workflow_id: Uuid,
        version_id: Uuid,
    ) -> Result<Workflow> {
        let name: Option<(String,)> =
            sqlx::query_as("SELECT name FROM workflows WHERE id = $1 AND tenant_id = $2")
                .bind(workflow_id)
                .bind(tenant_id.into_uuid())
                .fetch_optional(self.pool)
                .await
                .map_err(StorageError::from_sqlx)?;

        let (name,) = name.ok_or_else(|| StorageError::not_found("workflow"))?;

        let nodes: Vec<(String, String, String, Value)> = sqlx::query_as(
            "SELECT node_key, node_type, name, config
             FROM workflow_nodes
             WHERE workflow_version_id = $1 AND tenant_id = $2
             ORDER BY node_key",
        )
        .bind(version_id)
        .bind(tenant_id.into_uuid())
        .fetch_all(self.pool)
        .await
        .map_err(StorageError::from_sqlx)?;

        let edges: Vec<(String, String, Option<String>)> = sqlx::query_as(
            "SELECT from_node.node_key, to_node.node_key, e.condition
             FROM workflow_edges e
             JOIN workflow_nodes from_node ON from_node.id = e.from_node_id
             JOIN workflow_nodes to_node ON to_node.id = e.to_node_id
             WHERE e.workflow_version_id = $1 AND e.tenant_id = $2",
        )
        .bind(version_id)
        .bind(tenant_id.into_uuid())
        .fetch_all(self.pool)
        .await
        .map_err(StorageError::from_sqlx)?;

        let mut workflow = Workflow::new(tenant_id, name);
        workflow.id = workflow_id;

        for (node_key, node_type, node_name, config) in nodes {
            let node_type = node_type_from_storage(&node_type, &config, &node_key)?;
            workflow.add_node(Node {
                id: node_key,
                node_type,
                name: node_name,
            });
        }

        for (from, to, condition) in edges {
            workflow.add_edge(from, to, condition);
        }

        Ok(workflow)
    }

    /// Starts a run and seeds one state row per node of the pinned version.
    ///
    /// Returns the run's execution id. The caller is responsible for having
    /// validated that the workflow is active.
    pub async fn start_run(&self, run: NewWorkflowRun) -> Result<Uuid> {
        let id = Uuid::new_v4();
        // The engine's `Condition` nodes read the run's input directly, and a
        // `Map` node reads `map_items`, so the input seeds the context. A
        // non-object input is wrapped rather than discarded.
        let context = if run.input.is_object() {
            run.input.clone()
        } else {
            serde_json::json!({ "input": run.input })
        };

        let mut tx = self.pool.begin().await.map_err(StorageError::from_sqlx)?;

        let result = async {
            sqlx::query(
                "INSERT INTO executions
                     (id, tenant_id, workflow_id, workflow_version_id, status, trigger_source,
                      correlation_id, input, workflow_context, started_at, parent_execution_id)
                 VALUES ($1, $2, $3, $4, 'RUNNING', $5, $6, $7, $8, NOW(), $9)",
            )
            .bind(id)
            .bind(run.tenant_id.into_uuid())
            .bind(run.workflow_id)
            .bind(run.workflow_version_id)
            .bind(&run.trigger_source)
            .bind(run.correlation_id.as_deref())
            .bind(&run.input)
            .bind(&context)
            .bind(run.parent_execution_id)
            .execute(&mut *tx)
            .await
            .map_err(StorageError::from_sqlx)?;

            sqlx::query(
                "INSERT INTO workflow_node_states
                     (id, tenant_id, workflow_execution_id, workflow_version_id,
                      node_key, node_id, node_type, state)
                 SELECT gen_random_uuid(), $2, $1, $3, n.node_key, n.id, n.node_type, 'PENDING'
                 FROM workflow_nodes n
                 WHERE n.workflow_version_id = $3 AND n.tenant_id = $2",
            )
            .bind(id)
            .bind(run.tenant_id.into_uuid())
            .bind(run.workflow_version_id)
            .execute(&mut *tx)
            .await
            .map_err(StorageError::from_sqlx)?;

            Ok::<(), StorageError>(())
        }
        .await;

        match result {
            Ok(()) => {
                tx.commit().await.map_err(StorageError::from_sqlx)?;
                Ok(id)
            }
            Err(error) => {
                let _ = tx.rollback().await;
                Err(error)
            }
        }
    }

    /// Claims up to `limit` runs for this pass.
    ///
    /// The claim is a soft lease: stamping `updated_at` means a second server
    /// running the same loop skips a run another server touched within
    /// `stale_secs`. That keeps the runtime safe to scale horizontally without a
    /// separate lock table (ADR-0009's "leaderless, database-coordinated" model).
    pub async fn claim_runnable(&self, limit: i64, stale_secs: f64) -> Result<Vec<WorkflowRunRow>> {
        sqlx::query_as::<_, WorkflowRunRow>(
            "UPDATE executions SET updated_at = NOW()
             WHERE id IN (
                 SELECT id FROM executions
                 WHERE workflow_id IS NOT NULL
                   AND job_id IS NULL
                   AND status IN ('RUNNING', 'CANCEL_REQUESTED')
                   AND updated_at < NOW() - make_interval(secs => $2)
                 ORDER BY created_at ASC
                 FOR UPDATE SKIP LOCKED
                 LIMIT $1
             )
             RETURNING id, tenant_id, workflow_id, workflow_version_id, status,
                       correlation_id, workflow_context, created_at",
        )
        .bind(limit)
        .bind(stale_secs)
        .fetch_all(self.pool)
        .await
        .map_err(StorageError::from_sqlx)
    }

    /// Reads one run, scoped to its tenant.
    pub async fn get_run(&self, tenant_id: TenantId, run_id: Uuid) -> Result<WorkflowRunRow> {
        sqlx::query_as::<_, WorkflowRunRow>(
            "SELECT id, tenant_id, workflow_id, workflow_version_id, status,
                    correlation_id, workflow_context, created_at
             FROM executions
             WHERE id = $1 AND tenant_id = $2
               AND workflow_id IS NOT NULL AND job_id IS NULL",
        )
        .bind(run_id)
        .bind(tenant_id.into_uuid())
        .fetch_optional(self.pool)
        .await
        .map_err(StorageError::from_sqlx)?
        .ok_or_else(|| StorageError::not_found("workflow execution"))
    }

    /// Every node state of a run, ordered for deterministic replay.
    pub async fn node_states(&self, run_id: Uuid) -> Result<Vec<NodeStateRow>> {
        sqlx::query_as::<_, NodeStateRow>(
            "SELECT node_key, node_id, node_type, state, suspension_reason, resume_at,
                    child_execution_id, required_role, output, failure_reason, attempt_count
             FROM workflow_node_states
             WHERE workflow_execution_id = $1
             ORDER BY node_key",
        )
        .bind(run_id)
        .fetch_all(self.pool)
        .await
        .map_err(StorageError::from_sqlx)
    }

    /// Writes a node's progress back.
    pub async fn save_node_state(&self, run_id: Uuid, update: &NodeStateUpdate) -> Result<()> {
        sqlx::query(
            "UPDATE workflow_node_states SET
                 state = $3,
                 suspension_reason = $4,
                 resume_at = $5,
                 child_execution_id = COALESCE($6, child_execution_id),
                 required_role = COALESCE($7, required_role),
                 output = COALESCE($8, output),
                 failure_reason = $9,
                 attempt_count = $10,
                 updated_at = NOW()
             WHERE workflow_execution_id = $1 AND node_key = $2",
        )
        .bind(run_id)
        .bind(&update.node_key)
        .bind(&update.state)
        .bind(update.suspension_reason.as_deref())
        .bind(update.resume_at)
        .bind(update.child_execution_id)
        .bind(update.required_role.as_deref())
        .bind(update.output.as_ref())
        .bind(update.failure_reason.as_deref())
        .bind(update.attempt_count)
        .execute(self.pool)
        .await
        .map_err(StorageError::from_sqlx)?;
        Ok(())
    }

    /// Persists the engine's accumulated context so the next pass resumes from
    /// it. Node outputs and condition results live here.
    pub async fn save_context(&self, run_id: Uuid, context: &Value) -> Result<()> {
        sqlx::query(
            "UPDATE executions SET workflow_context = $2, updated_at = NOW() WHERE id = $1",
        )
        .bind(run_id)
        .bind(context)
        .execute(self.pool)
        .await
        .map_err(StorageError::from_sqlx)?;
        Ok(())
    }

    /// The child executions a run has dispatched, with their current status.
    pub async fn child_executions(&self, run_id: Uuid) -> Result<Vec<ChildExecutionRow>> {
        sqlx::query_as::<_, ChildExecutionRow>(
            "SELECT s.node_key, s.child_execution_id, e.status, e.output, e.error_message
             FROM workflow_node_states s
             JOIN executions e ON e.id = s.child_execution_id
             WHERE s.workflow_execution_id = $1",
        )
        .bind(run_id)
        .fetch_all(self.pool)
        .await
        .map_err(StorageError::from_sqlx)
    }

    /// The database id of a node, needed to key an approval row.
    /// Creates the child execution a workflow node dispatches.
    ///
    /// A child is a normal job execution — it is claimed by a worker through
    /// the usual dispatch path — but it carries `parent_execution_id` so the run
    /// can read its outcome back.
    pub async fn create_child_execution(&self, child: NewChildExecution) -> Result<Uuid> {
        let id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO executions
                 (id, tenant_id, job_id, job_version_id, queue_id, status, trigger_source,
                  parent_execution_id, correlation_id, input, priority)
             VALUES ($1, $2, $3, $4, $5, 'QUEUED', 'WORKFLOW', $6, $7, $8, $9)",
        )
        .bind(id)
        .bind(child.tenant_id.into_uuid())
        .bind(child.job_id)
        .bind(child.job_version_id)
        .bind(child.queue_id)
        .bind(child.parent_execution_id)
        .bind(child.correlation_id.as_deref())
        .bind(&child.input)
        .bind(&child.priority)
        .execute(self.pool)
        .await
        .map_err(StorageError::from_sqlx)?;
        Ok(id)
    }

    /// Reads the outcome of specific executions.
    pub async fn execution_outcomes(&self, ids: &[Uuid]) -> Result<Vec<ExecutionOutcomeRow>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        sqlx::query_as::<_, ExecutionOutcomeRow>(
            "SELECT id, status, output, error_message FROM executions WHERE id = ANY($1)",
        )
        .bind(ids)
        .fetch_all(self.pool)
        .await
        .map_err(StorageError::from_sqlx)
    }

    /// Resolves which job version and queue a JOB node should execute.
    ///
    /// Returns `None` when the job does not exist, is archived, or has no
    /// published version — all of which mean the node cannot run, and the
    /// caller turns that into a node failure with a reason rather than a stall.
    pub async fn job_dispatch_target(
        &self,
        tenant_id: TenantId,
        job_id: Uuid,
    ) -> Result<Option<JobDispatchTarget>> {
        let row: Option<JobDispatchRow> = sqlx::query_as(
            "SELECT j.current_version_id,
                    (SELECT v.id FROM job_versions v
                      WHERE v.job_id = j.id AND v.published_at IS NOT NULL
                      ORDER BY v.version_number DESC LIMIT 1) AS latest_version_id,
                    j.default_queue_id,
                    j.priority
             FROM jobs j
             WHERE j.id = $1 AND j.tenant_id = $2 AND j.status <> 'ARCHIVED'",
        )
        .bind(job_id)
        .bind(tenant_id.into_uuid())
        .fetch_optional(self.pool)
        .await
        .map_err(StorageError::from_sqlx)?;

        let Some(row) = row else {
            return Ok(None);
        };

        Ok(row
            .current_version_id
            .or(row.latest_version_id)
            .map(|job_version_id| JobDispatchTarget {
                job_version_id,
                queue_id: row.default_queue_id,
                priority: row.priority,
            }))
    }

    /// Resolves which workflow version a SUB_WORKFLOW node should execute.
    pub async fn workflow_dispatch_target(
        &self,
        tenant_id: TenantId,
        workflow_id: Uuid,
    ) -> Result<Option<WorkflowDispatchTarget>> {
        let row: Option<(Option<Uuid>, Option<Uuid>)> = sqlx::query_as(
            "SELECT w.current_version_id,
                    (SELECT v.id FROM workflow_versions v
                      WHERE v.workflow_id = w.id AND v.published_at IS NOT NULL
                      ORDER BY v.version_number DESC LIMIT 1) AS latest_version_id
             FROM workflows w
             WHERE w.id = $1 AND w.tenant_id = $2 AND w.status <> 'ARCHIVED'",
        )
        .bind(workflow_id)
        .bind(tenant_id.into_uuid())
        .fetch_optional(self.pool)
        .await
        .map_err(StorageError::from_sqlx)?;

        let Some((current, latest)) = row else {
            return Ok(None);
        };

        let Some(version_id) = current.or(latest) else {
            return Ok(None);
        };

        let timeout_seconds: i32 = sqlx::query_scalar(
            "SELECT COALESCE(timeout_seconds, 86400) FROM workflow_versions WHERE id = $1 AND tenant_id = $2",
        )
        .bind(version_id)
        .bind(tenant_id.into_uuid())
        .fetch_optional(self.pool)
        .await
        .map_err(StorageError::from_sqlx)?
        .unwrap_or(86_400);

        Ok(Some(WorkflowDispatchTarget {
            workflow_version_id: version_id,
            timeout_seconds,
        }))
    }

    /// Reads a node's state from its database id.
    ///
    /// The approval endpoints address a node by the id the caller sees in the
    /// API, which is the `workflow_nodes` id rather than the engine's key.
    pub async fn node_state_by_id(
        &self,
        run_id: Uuid,
        node_id: Uuid,
    ) -> Result<Option<NodeStateRow>> {
        sqlx::query_as::<_, NodeStateRow>(
            "SELECT node_key, node_id, node_type, state, suspension_reason, resume_at,
                    child_execution_id, required_role, output, failure_reason, attempt_count
             FROM workflow_node_states
             WHERE workflow_execution_id = $1 AND node_id = $2",
        )
        .bind(run_id)
        .bind(node_id)
        .fetch_optional(self.pool)
        .await
        .map_err(StorageError::from_sqlx)
    }

    /// The database id of a node, needed to key an approval row.
    pub async fn node_id_for(&self, run_id: Uuid, node_key: &str) -> Result<Option<Uuid>> {
        let row: Option<(Option<Uuid>,)> = sqlx::query_as(
            "SELECT node_id FROM workflow_node_states
             WHERE workflow_execution_id = $1 AND node_key = $2",
        )
        .bind(run_id)
        .bind(node_key)
        .fetch_optional(self.pool)
        .await
        .map_err(StorageError::from_sqlx)?;
        Ok(row.and_then(|(id,)| id))
    }

    /// Opens (or re-opens) the approval request for a suspended node.
    pub async fn request_approval(
        &self,
        tenant_id: TenantId,
        run_id: Uuid,
        node_id: Uuid,
        required_role: &str,
        requested_by: Option<Uuid>,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO manual_approvals
                 (id, tenant_id, workflow_execution_id, node_id, status, required_role, requested_by)
             VALUES (gen_random_uuid(), $1, $2, $3, 'PENDING', $4, $5)
             ON CONFLICT (workflow_execution_id, node_id)
             DO UPDATE SET required_role = EXCLUDED.required_role",
        )
        .bind(tenant_id.into_uuid())
        .bind(run_id)
        .bind(node_id)
        .bind(required_role)
        .bind(requested_by)
        .execute(self.pool)
        .await
        .map_err(StorageError::from_sqlx)?;
        Ok(())
    }

    /// The recorded decision for a node, if one has been made.
    pub async fn approval_decision(&self, run_id: Uuid, node_id: Uuid) -> Result<Option<String>> {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT status FROM manual_approvals
             WHERE workflow_execution_id = $1 AND node_id = $2",
        )
        .bind(run_id)
        .bind(node_id)
        .fetch_optional(self.pool)
        .await
        .map_err(StorageError::from_sqlx)?;
        Ok(row.map(|(status,)| status))
    }

    /// Finishes a run. Only a non-terminal run can be settled, so a late pass
    /// cannot overwrite the outcome.
    pub async fn settle_run(
        &self,
        run_id: Uuid,
        status: &str,
        error_message: Option<&str>,
        output: Option<&Value>,
    ) -> Result<bool> {
        let affected = sqlx::query(
            "UPDATE executions SET status = $2, error_message = COALESCE($3, error_message),
                    output = COALESCE($4, output),
                    ended_at = NOW(), updated_at = NOW()
             WHERE id = $1 AND workflow_id IS NOT NULL AND job_id IS NULL
               AND status IN ('RUNNING', 'CANCEL_REQUESTED')",
        )
        .bind(run_id)
        .bind(status)
        .bind(error_message)
        .bind(output)
        .execute(self.pool)
        .await
        .map_err(StorageError::from_sqlx)?
        .rows_affected();
        Ok(affected > 0)
    }

    /// Marks every node that has not finished as failed, with the given reason.
    ///
    /// Used when a run is torn down for a reason that is not any single node's
    /// fault — a workflow-level timeout, or an operator cancellation that the
    /// children never acknowledged.
    pub async fn fail_open_nodes(&self, run_id: Uuid, reason: &str) -> Result<u64> {
        let affected = sqlx::query(
            "UPDATE workflow_node_states
             SET state = 'FAILED', failure_reason = $2, updated_at = NOW()
             WHERE workflow_execution_id = $1
               AND state IN ('PENDING', 'RUNNING', 'SUSPENDED')",
        )
        .bind(run_id)
        .bind(reason)
        .execute(self.pool)
        .await
        .map_err(StorageError::from_sqlx)?
        .rows_affected();
        Ok(affected)
    }

    /// Asks every child execution of a run to stop.
    ///
    /// Children are found by `parent_execution_id`, which covers both a single
    /// job node and every child of a fan-out.
    pub async fn cancel_children(&self, run_id: Uuid) -> Result<u64> {
        let affected = sqlx::query(
            "UPDATE executions SET status = 'CANCEL_REQUESTED', updated_at = NOW()
             WHERE parent_execution_id = $1
               AND status IN ('SCHEDULED', 'QUEUED', 'DISPATCHED', 'RUNNING', 'RETRY_SCHEDULED')",
        )
        .bind(run_id)
        .execute(self.pool)
        .await
        .map_err(StorageError::from_sqlx)?
        .rows_affected();
        Ok(affected)
    }

    /// Requests cancellation of a run and of every child it has dispatched.
    ///
    /// Cancellation is cooperative (ADR-0016): the run stops being advanced and
    /// each child is asked to stop, exactly as `POST /executions/{id}/cancel`
    /// would.
    pub async fn cancel_run(&self, tenant_id: TenantId, run_id: Uuid) -> Result<bool> {
        let mut tx = self.pool.begin().await.map_err(StorageError::from_sqlx)?;

        let result = async {
            let affected = sqlx::query(
                "UPDATE executions SET status = 'CANCEL_REQUESTED', updated_at = NOW()
                 WHERE id = $1 AND tenant_id = $2
                   AND workflow_id IS NOT NULL AND job_id IS NULL
                   AND status = 'RUNNING'",
            )
            .bind(run_id)
            .bind(tenant_id.into_uuid())
            .execute(&mut *tx)
            .await
            .map_err(StorageError::from_sqlx)?
            .rows_affected();

            if affected == 0 {
                return Ok(false);
            }

            sqlx::query(
                "UPDATE executions SET status = 'CANCEL_REQUESTED', updated_at = NOW()
                 WHERE tenant_id = $1 AND parent_execution_id = $2
                   AND status IN ('SCHEDULED', 'QUEUED', 'DISPATCHED', 'RUNNING',
                                  'RETRY_SCHEDULED')",
            )
            .bind(tenant_id.into_uuid())
            .bind(run_id)
            .execute(&mut *tx)
            .await
            .map_err(StorageError::from_sqlx)?;

            Ok(true)
        }
        .await;

        match result {
            Ok(done) => {
                tx.commit().await.map_err(StorageError::from_sqlx)?;
                Ok(done)
            }
            Err(error) => {
                let _ = tx.rollback().await;
                Err(error)
            }
        }
    }

    /// Runs whose deadline has passed, for the timeout sweeper.
    pub async fn timed_out_runs(&self, limit: i64) -> Result<Vec<WorkflowRunRow>> {
        sqlx::query_as::<_, WorkflowRunRow>(
            "SELECT e.id, e.tenant_id, e.workflow_id, e.workflow_version_id, e.status,
                    e.correlation_id, e.workflow_context, e.created_at
             FROM executions e
             JOIN workflow_versions v ON v.id = e.workflow_version_id
             WHERE e.workflow_id IS NOT NULL AND e.job_id IS NULL
               AND e.status = 'RUNNING'
               AND e.created_at + make_interval(secs => v.timeout_seconds) < NOW()
             ORDER BY e.created_at ASC
             LIMIT $1",
        )
        .bind(limit)
        .fetch_all(self.pool)
        .await
        .map_err(StorageError::from_sqlx)
    }
}

/// Rebuilds a node's type from its stored row.
///
/// The mapping is the inverse of the one the API applies when saving a
/// definition, so a graph that validated on save loads identically here.
fn node_type_from_storage(raw_type: &str, config: &Value, node_key: &str) -> Result<NodeType> {
    let invalid = |detail: String| StorageError::Validation(detail);

    match raw_type {
        "JOB" => {
            let job_id = config
                .get("job_id")
                .and_then(Value::as_str)
                .and_then(|raw| Uuid::parse_str(raw).ok())
                .ok_or_else(|| {
                    invalid(format!(
                        "node `{node_key}` does not reference a valid job_id"
                    ))
                })?;
            Ok(NodeType::Job {
                job_id: forge_domain::JobId::from_uuid(job_id),
            })
        }
        "APPROVAL" => Ok(NodeType::Approval {
            required_role: config
                .get("required_role")
                .and_then(Value::as_str)
                .unwrap_or("ADMIN")
                .to_string(),
        }),
        "DELAY" => Ok(NodeType::Delay {
            seconds: config.get("seconds").and_then(Value::as_u64).unwrap_or(0),
        }),
        "CONDITION" => {
            let expression = config
                .get("expression")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            if expression.trim().is_empty() {
                return Err(invalid(format!(
                    "condition node `{node_key}` has no expression"
                )));
            }
            Ok(NodeType::Condition { expression })
        }
        "MAP" => {
            let target = config
                .get("target_node_id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            if target.is_empty() {
                return Err(invalid(format!("map node `{node_key}` has no target node")));
            }
            Ok(NodeType::Map {
                target_node_id: target,
            })
        }
        "SUB_WORKFLOW" => {
            let workflow_id = config
                .get("workflow_id")
                .and_then(Value::as_str)
                .and_then(|raw| Uuid::parse_str(raw).ok())
                .ok_or_else(|| {
                    invalid(format!(
                        "sub-workflow node `{node_key}` does not reference a valid workflow_id"
                    ))
                })?;
            Ok(NodeType::SubWorkflow { workflow_id })
        }
        // A webhook node is an outbound HTTP call. Until the dispatcher performs
        // it, treating it as a zero-second delay would silently succeed, so it
        // is refused loudly instead: a node that never fires is worse than one
        // that fails with a reason.
        "WEBHOOK" => Err(invalid(format!(
            "webhook node `{node_key}` is not supported by this runtime yet"
        ))),
        other => Err(invalid(format!("`{other}` is not a known node type"))),
    }
}

/// The terminal statuses a run can settle into.
pub fn is_terminal_run_status(status: &str) -> bool {
    matches!(
        status,
        "SUCCEEDED" | "FAILED" | "CANCELLED" | "TIMED_OUT" | "DEAD_LETTERED"
    )
}

/// Whether a child execution has finished, one way or another.
pub fn is_terminal_execution_status(status: &str) -> bool {
    matches!(
        status,
        "SUCCEEDED" | "FAILED" | "CANCELLED" | "TIMED_OUT" | "DEAD_LETTERED"
    )
}
