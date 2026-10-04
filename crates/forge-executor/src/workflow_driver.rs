//! Drives workflow runs to completion (spec 02.11, spec 10).
//!
//! [`crate::workflow_engine`] is a pure decision function: given a graph, the
//! node states and the accumulated context, it says what work is outstanding.
//! This module is the other half — it loads that state from the database, applies
//! the engine's decisions as real work (dispatch a job, ask a human, wait), and
//! writes the result back.
//!
//! The split matters: the engine has no I/O, so it is exhaustively unit-testable,
//! and the driver has no scheduling logic, so it cannot invent a second set of
//! semantics.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use forge_domain::workflow::NodeType;
use forge_domain::TenantId;
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use forge_storage::{
    is_terminal_execution_status, NewChildExecution, NodeStateRow, NodeStateUpdate,
    WorkflowRunRepository, WorkflowRunRow,
};

use crate::workflow_engine::{NodeState, SuspensionReason, WorkflowAction, WorkflowExecution};

/// What one pass over a run achieved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunOutcome {
    /// The run took a step and is still going.
    Progressed,
    /// The run reached a terminal state.
    Settled,
}

/// Aggregated result of a driver pass, for the runtime's logs and metrics.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkflowTickReport {
    pub runs_advanced: u64,
    pub runs_settled: u64,
    pub nodes_dispatched: u64,
    pub runs_timed_out: u64,
    pub errors: Vec<(Uuid, String)>,
}

impl WorkflowTickReport {
    pub fn is_empty(&self) -> bool {
        self.runs_advanced == 0 && self.runs_timed_out == 0 && self.errors.is_empty()
    }
}

/// Loads, advances and persists workflow runs.
pub struct WorkflowDriver<'a> {
    pool: &'a PgPool,
    /// Ceiling on how many children one `MAP` node may fan out to in a single
    /// pass (spec 17's `FORGE_MAX_FANOUT`).
    max_fanout: usize,
}

impl<'a> WorkflowDriver<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self {
            pool,
            max_fanout: 100,
        }
    }

    pub fn with_max_fanout(mut self, max_fanout: usize) -> Self {
        self.max_fanout = max_fanout.max(1);
        self
    }

    /// Advances up to `batch` runs that are due, then enforces run timeouts.
    ///
    /// `stale_secs` is the soft lease: a run another server advanced more
    /// recently than that is skipped, so two servers never fight over one graph.
    pub async fn tick(
        &self,
        now: DateTime<Utc>,
        batch: i64,
        stale_secs: f64,
    ) -> WorkflowTickReport {
        let mut report = WorkflowTickReport::default();
        let repository = WorkflowRunRepository::new(self.pool);

        match repository.claim_runnable(batch, stale_secs).await {
            Ok(runs) => {
                for run in runs {
                    let run_id = run.id;
                    match self.advance_run(run, now).await {
                        Ok(outcome) => {
                            report.runs_advanced += 1;
                            if outcome == RunOutcome::Settled {
                                report.runs_settled += 1;
                            }
                        }
                        Err(reason) => report.errors.push((run_id, reason)),
                    }
                }
            }
            Err(error) => report.errors.push((Uuid::nil(), error.to_string())),
        }

        match repository.timed_out_runs(batch).await {
            Ok(runs) => {
                for run in runs {
                    let run_id = run.id;
                    match self.time_out_run(run, now).await {
                        Ok(()) => report.runs_timed_out += 1,
                        Err(reason) => report.errors.push((run_id, reason)),
                    }
                }
            }
            Err(error) => report.errors.push((Uuid::nil(), error.to_string())),
        }

        report
    }

    /// One pass over one run.
    pub async fn advance_run(
        &self,
        run: WorkflowRunRow,
        now: DateTime<Utc>,
    ) -> Result<RunOutcome, String> {
        let repository = WorkflowRunRepository::new(self.pool);
        let tenant = TenantId::from_uuid(run.tenant_id);

        let definition = repository
            .load_definition(tenant, run.workflow_id, run.workflow_version_id)
            .await
            .map_err(|error| error.to_string())?;
        let rows = repository
            .node_states(run.id)
            .await
            .map_err(|error| error.to_string())?;

        let mut execution = WorkflowExecution::new(definition);
        // Cloned rather than moved: the run row is still needed for its id and
        // correlation id further down.
        execution.context = match &run.workflow_context {
            Value::Object(map) => map.clone().into_iter().collect(),
            _ => HashMap::new(),
        };
        for row in &rows {
            execution
                .node_states
                .insert(row.node_key.clone(), to_domain_state(row));
        }

        // 1. Fold in whatever the dispatched children have finished with.
        let children = children_by_node(&rows);
        self.report_children(&mut execution, &children).await?;

        // 2. Fold in any human decisions.
        self.apply_approval_decisions(&mut execution, &rows, run.id)
            .await?;

        // A cancellation only settles once the work it asked to stop has
        // actually stopped (ADR-0016: cancellation is cooperative).
        if run.status == "CANCEL_REQUESTED" {
            return self.settle_cancellation(&repository, &run, &children).await;
        }

        // 3. Ask the engine what is outstanding.
        let actions = execution
            .advance_at(now)
            .map_err(|error| error.to_string())?;

        // 4. Apply it.
        let mut known_children = children;
        let mut node_outputs: HashMap<String, Value> = HashMap::new();
        let mut dispatched = 0u64;

        for action in actions {
            match action {
                WorkflowAction::DispatchJob { node_id, job_id } => {
                    match self
                        .dispatch_child(&run, job_id, Value::Null, &node_id)
                        .await
                    {
                        Ok(child_id) => {
                            known_children.entry(node_id).or_default().push(child_id);
                            dispatched += 1;
                        }
                        Err(reason) => execution.fail_node(&node_id, &reason),
                    }
                }

                WorkflowAction::ScheduleDelay { .. } | WorkflowAction::NodeFailed { .. } => {
                    // The engine already recorded the node's new state; the
                    // write-back below is what makes it durable.
                }

                WorkflowAction::RequestApproval {
                    node_id,
                    required_role,
                } => {
                    if let Some(node_id_db) = repository
                        .node_id_for(run.id, &node_id)
                        .await
                        .map_err(|error| error.to_string())?
                    {
                        repository
                            .request_approval(tenant, run.id, node_id_db, &required_role, None)
                            .await
                            .map_err(|error| error.to_string())?;
                    }
                }

                WorkflowAction::FanOut {
                    node_id,
                    target_node_id,
                    items,
                } => {
                    let target_job =
                        execution
                            .workflow
                            .nodes
                            .get(&target_node_id)
                            .and_then(|node| match &node.node_type {
                                NodeType::Job { job_id } => Some(job_id.into_uuid()),
                                _ => None,
                            });

                    let Some(job_id) = target_job else {
                        execution.fail_node(
                            &node_id,
                            &format!("map target `{target_node_id}` is not a job node"),
                        );
                        continue;
                    };

                    let mut child_ids = Vec::new();
                    let mut failure = None;
                    for (index, item) in items.into_iter().take(self.max_fanout).enumerate() {
                        let suffix = format!("{node_id}[{index}]");
                        match self.dispatch_child(&run, job_id, item, &suffix).await {
                            Ok(child_id) => child_ids.push(child_id),
                            Err(reason) => {
                                failure = Some(reason);
                                break;
                            }
                        }
                    }

                    match failure {
                        Some(reason) => {
                            for child_id in &child_ids {
                                let _ = self.request_child_cancel(*child_id).await;
                            }
                            execution.fail_node(&node_id, &reason);
                        }
                        None if child_ids.is_empty() => {
                            execution.fail_node(&node_id, "no items to fan out over");
                        }
                        None => {
                            dispatched += child_ids.len() as u64;
                            // The map node's target is the fanned-out work, so it
                            // must not also be dispatched as an ordinary node. Marking
                            // it Running is enough: `ready_nodes` only starts Pending
                            // nodes, and the target is settled when its children are.
                            execution
                                .node_states
                                .insert(target_node_id.clone(), NodeState::Running);
                            // The fan-out children have to survive a restart, so
                            // the node carries their ids.
                            node_outputs.insert(
                                node_id.clone(),
                                serde_json::json!({
                                    "fan_out_children": child_ids
                                        .iter()
                                        .map(|id| id.to_string())
                                        .collect::<Vec<_>>()
                                }),
                            );
                            known_children.insert(node_id, child_ids);
                        }
                    }
                }
            }
        }

        // 5. Persist node progress and context.
        for (node_key, state) in &execution.node_states {
            let child = known_children
                .get(node_key)
                .and_then(|ids| ids.first())
                .copied();
            let update = to_update(
                node_key,
                state,
                child,
                node_outputs.get(node_key).cloned(),
                rows.iter()
                    .find(|row| &row.node_key == node_key)
                    .map(|row| row.attempt_count)
                    .unwrap_or(0),
            );
            repository
                .save_node_state(run.id, &update)
                .await
                .map_err(|error| error.to_string())?;
        }

        let context = serde_json::to_value(&execution.context).unwrap_or(Value::Null);
        repository
            .save_context(run.id, &context)
            .await
            .map_err(|error| error.to_string())?;

        // 6. Settle if the graph has run out of work.
        if execution.is_complete() {
            let failed = execution.has_failed();
            let reason = execution
                .node_states
                .values()
                .find_map(|state| match state {
                    NodeState::Failed { reason } => Some(reason.clone()),
                    _ => None,
                });
            repository
                .settle_run(
                    run.id,
                    if failed { "FAILED" } else { "SUCCEEDED" },
                    reason.as_deref(),
                )
                .await
                .map_err(|error| error.to_string())?;
            return Ok(RunOutcome::Settled);
        }

        let _ = dispatched;
        Ok(RunOutcome::Progressed)
    }

    /// Reads child execution outcomes back into their nodes.
    async fn report_children(
        &self,
        execution: &mut WorkflowExecution,
        children: &HashMap<String, Vec<Uuid>>,
    ) -> Result<(), String> {
        let repository = WorkflowRunRepository::new(self.pool);
        let all: Vec<Uuid> = children.values().flatten().copied().collect();
        let outcomes = repository
            .execution_outcomes(&all)
            .await
            .map_err(|error| error.to_string())?;
        let by_id: HashMap<Uuid, _> = outcomes.into_iter().map(|row| (row.id, row)).collect();

        for (node_key, child_ids) in children {
            // Only a node that is still waiting cares about its children.
            match execution.node_states.get(node_key) {
                Some(NodeState::Running) | Some(NodeState::Suspended { .. }) => {}
                _ => continue,
            }

            let mut all_terminal = true;
            let mut failure: Option<String> = None;
            let mut output: Option<Value> = None;

            for child_id in child_ids {
                let Some(outcome) = by_id.get(child_id) else {
                    all_terminal = false;
                    continue;
                };
                if !is_terminal_execution_status(&outcome.status) {
                    all_terminal = false;
                    continue;
                }
                if outcome.status == "SUCCEEDED" {
                    if output.is_none() {
                        output = outcome.output.clone();
                    }
                } else if failure.is_none() {
                    failure = Some(
                        outcome
                            .error_message
                            .clone()
                            .unwrap_or_else(|| format!("child execution {}", outcome.status)),
                    );
                }
            }

            if !all_terminal {
                continue;
            }

            match failure {
                Some(reason) => {
                    execution.fail_node(node_key, &reason);
                    // A map node's target *is* the fanned-out work, so the target
                    // shares the map's outcome rather than running a second time.
                    if let Some(target) = map_target(&execution.workflow, node_key) {
                        execution.fail_node(&target, &reason);
                    }
                }
                None => {
                    execution.complete_node(node_key, output);
                    if let Some(target) = map_target(&execution.workflow, node_key) {
                        execution.complete_node(&target, None);
                    }
                }
            }
        }

        Ok(())
    }

    /// Folds recorded approval decisions into the suspended nodes.
    async fn apply_approval_decisions(
        &self,
        execution: &mut WorkflowExecution,
        rows: &[NodeStateRow],
        run_id: Uuid,
    ) -> Result<(), String> {
        let repository = WorkflowRunRepository::new(self.pool);

        for row in rows {
            let awaiting = matches!(row.state.as_str(), "SUSPENDED")
                && row.suspension_reason.as_deref() == Some("AWAITING_APPROVAL");
            if !awaiting {
                continue;
            }
            let Some(node_id) = row.node_id else {
                continue;
            };
            match repository
                .approval_decision(run_id, node_id)
                .await
                .map_err(|error| error.to_string())?
                .as_deref()
            {
                Some("APPROVED") => {
                    execution.resume(&row.node_key);
                }
                Some("REJECTED") => {
                    execution.reject(&row.node_key, "rejected by an approver");
                }
                _ => {}
            }
        }

        Ok(())
    }

    /// Settles a cancelled run once its children have stopped.
    async fn settle_cancellation(
        &self,
        repository: &WorkflowRunRepository<'_>,
        run: &WorkflowRunRow,
        children: &HashMap<String, Vec<Uuid>>,
    ) -> Result<RunOutcome, String> {
        let all: Vec<Uuid> = children.values().flatten().copied().collect();
        let outcomes = repository
            .execution_outcomes(&all)
            .await
            .map_err(|error| error.to_string())?;

        let still_running = outcomes
            .iter()
            .any(|row| !is_terminal_execution_status(&row.status));

        if still_running {
            return Ok(RunOutcome::Progressed);
        }

        repository
            .settle_run(run.id, "CANCELLED", Some("cancelled by an operator"))
            .await
            .map_err(|error| error.to_string())?;
        Ok(RunOutcome::Settled)
    }

    /// Fails a run that outlived its workflow version's timeout.
    async fn time_out_run(&self, run: WorkflowRunRow, _now: DateTime<Utc>) -> Result<(), String> {
        let repository = WorkflowRunRepository::new(self.pool);
        let reason = "workflow exceeded its timeout";

        repository
            .fail_open_nodes(run.id, reason)
            .await
            .map_err(|error| error.to_string())?;
        repository
            .cancel_children(run.id)
            .await
            .map_err(|error| error.to_string())?;
        repository
            .settle_run(run.id, "TIMED_OUT", Some(reason))
            .await
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    /// Creates the child execution for one node's job.
    ///
    /// `suffix` distinguishes the child within the run. It matters because
    /// `executions (tenant_id, correlation_id)` is unique — that index is the
    /// manual-trigger idempotency guard (AT-API-005) — so a child must not
    /// simply inherit the run's correlation id. Deriving one keeps every child
    /// traceable back to the run while leaving the invariant intact.
    async fn dispatch_child(
        &self,
        run: &WorkflowRunRow,
        job_id: Uuid,
        input: Value,
        suffix: &str,
    ) -> Result<Uuid, String> {
        let repository = WorkflowRunRepository::new(self.pool);
        let tenant = TenantId::from_uuid(run.tenant_id);

        let target = repository
            .job_dispatch_target(tenant, job_id)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| {
                format!("job {job_id} does not exist, is archived, or has no published version")
            })?;

        repository
            .create_child_execution(NewChildExecution {
                tenant_id: tenant,
                job_id,
                job_version_id: target.job_version_id,
                queue_id: target.queue_id,
                parent_execution_id: run.id,
                correlation_id: run
                    .correlation_id
                    .as_ref()
                    .map(|base| format!("{base}:{suffix}")),
                input,
                priority: target.priority,
            })
            .await
            .map_err(|error| error.to_string())
    }

    /// Asks one child execution to stop.
    async fn request_child_cancel(&self, execution_id: Uuid) -> Result<(), String> {
        sqlx::query(
            "UPDATE executions SET status = 'CANCEL_REQUESTED', updated_at = NOW()
             WHERE id = $1 AND status IN ('SCHEDULED','QUEUED','DISPATCHED','RUNNING','RETRY_SCHEDULED')",
        )
        .bind(execution_id)
        .execute(self.pool)
        .await
        .map_err(|error| error.to_string())?;
        Ok(())
    }
}

/// Rebuilds a node's domain state from its stored row.
fn to_domain_state(row: &NodeStateRow) -> NodeState {
    match row.state.as_str() {
        "RUNNING" => NodeState::Running,
        "COMPLETED" => NodeState::Completed,
        "FAILED" => NodeState::Failed {
            reason: row
                .failure_reason
                .clone()
                .unwrap_or_else(|| "node failed".to_string()),
        },
        "SKIPPED" => NodeState::Skipped,
        "SUSPENDED" => NodeState::Suspended {
            reason: match row.suspension_reason.as_deref() {
                Some("AWAITING_APPROVAL") => SuspensionReason::AwaitingApproval {
                    required_role: row
                        .required_role
                        .clone()
                        .unwrap_or_else(|| "ADMIN".to_string()),
                },
                Some("AWAITING_FAN_OUT") => SuspensionReason::AwaitingFanOut,
                _ => SuspensionReason::Delay,
            },
            resume_at: row.resume_at,
        },
        _ => NodeState::Pending,
    }
}

/// Converts a domain node state into the row to write.
fn to_update(
    node_key: &str,
    state: &NodeState,
    child_execution_id: Option<Uuid>,
    output: Option<Value>,
    attempt_count: i32,
) -> NodeStateUpdate {
    let mut update = NodeStateUpdate {
        node_key: node_key.to_string(),
        state: String::new(),
        suspension_reason: None,
        resume_at: None,
        child_execution_id,
        required_role: None,
        output,
        failure_reason: None,
        attempt_count,
    };

    match state {
        NodeState::Pending => update.state = "PENDING".into(),
        NodeState::Running => update.state = "RUNNING".into(),
        NodeState::Completed => update.state = "COMPLETED".into(),
        NodeState::Skipped => update.state = "SKIPPED".into(),
        NodeState::Failed { reason } => {
            update.state = "FAILED".into();
            update.failure_reason = Some(reason.clone());
        }
        NodeState::Suspended { reason, resume_at } => {
            update.state = "SUSPENDED".into();
            update.resume_at = *resume_at;
            update.suspension_reason = Some(match reason {
                SuspensionReason::AwaitingApproval { required_role } => {
                    update.required_role = Some(required_role.clone());
                    "AWAITING_APPROVAL".to_string()
                }
                SuspensionReason::Delay => "DELAY".to_string(),
                SuspensionReason::AwaitingFanOut => "AWAITING_FAN_OUT".to_string(),
            });
        }
    }

    update
}

/// The node a `MAP` node fans out over, if `node_key` names one.
fn map_target(workflow: &forge_domain::workflow::Workflow, node_key: &str) -> Option<String> {
    match &workflow.nodes.get(node_key)?.node_type {
        NodeType::Map { target_node_id } => Some(target_node_id.clone()),
        _ => None,
    }
}

/// The child executions each node has dispatched, including fan-out children.
fn children_by_node(rows: &[NodeStateRow]) -> HashMap<String, Vec<Uuid>> {
    let mut children: HashMap<String, Vec<Uuid>> = HashMap::new();

    for row in rows {
        let mut ids = Vec::new();
        if let Some(id) = row.child_execution_id {
            ids.push(id);
        }
        if let Some(list) = row
            .output
            .as_ref()
            .and_then(|output| output.get("fan_out_children"))
            .and_then(Value::as_array)
        {
            ids.extend(
                list.iter()
                    .filter_map(Value::as_str)
                    .filter_map(|raw| Uuid::parse_str(raw).ok()),
            );
        }
        if !ids.is_empty() {
            children.insert(row.node_key.clone(), ids);
        }
    }

    children
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn row(state: &str, reason: Option<&str>, role: Option<&str>) -> NodeStateRow {
        NodeStateRow {
            node_key: "n1".into(),
            node_id: None,
            node_type: "APPROVAL".into(),
            state: state.into(),
            suspension_reason: reason.map(str::to_string),
            resume_at: None,
            child_execution_id: None,
            required_role: role.map(str::to_string),
            output: None,
            failure_reason: None,
            attempt_count: 0,
        }
    }

    #[test]
    fn a_suspended_approval_remembers_its_role() {
        let state = to_domain_state(&row("SUSPENDED", Some("AWAITING_APPROVAL"), Some("ADMIN")));
        assert_eq!(
            state,
            NodeState::Suspended {
                reason: SuspensionReason::AwaitingApproval {
                    required_role: "ADMIN".into()
                },
                resume_at: None,
            }
        );
    }

    #[test]
    fn an_unknown_suspension_is_treated_as_a_delay() {
        assert!(matches!(
            to_domain_state(&row("SUSPENDED", Some("SOMETHING_NEW"), None)),
            NodeState::Suspended {
                reason: SuspensionReason::Delay,
                ..
            }
        ));
    }

    #[test]
    fn a_failed_node_keeps_its_reason() {
        let mut stored = row("FAILED", None, None);
        stored.failure_reason = Some("upstream exploded".into());
        assert_eq!(
            to_domain_state(&stored),
            NodeState::Failed {
                reason: "upstream exploded".into()
            }
        );
    }

    /// The round trip has to be lossless, or a restart changes a run's meaning.
    #[test]
    fn a_state_round_trips_through_storage() {
        for original in [
            NodeState::Pending,
            NodeState::Running,
            NodeState::Completed,
            NodeState::Skipped,
            NodeState::Failed {
                reason: "boom".into(),
            },
            NodeState::Suspended {
                reason: SuspensionReason::Delay,
                resume_at: Some(Utc::now()),
            },
            NodeState::Suspended {
                reason: SuspensionReason::AwaitingFanOut,
                resume_at: None,
            },
            NodeState::Suspended {
                reason: SuspensionReason::AwaitingApproval {
                    required_role: "OPERATOR".into(),
                },
                resume_at: None,
            },
        ] {
            let update = to_update("n1", &original, None, None, 0);
            let stored = NodeStateRow {
                node_key: update.node_key,
                node_id: None,
                node_type: "ANY".into(),
                state: update.state,
                suspension_reason: update.suspension_reason,
                resume_at: update.resume_at,
                child_execution_id: update.child_execution_id,
                required_role: update.required_role,
                output: update.output,
                failure_reason: update.failure_reason,
                attempt_count: update.attempt_count,
            };
            assert_eq!(to_domain_state(&stored), original);
        }
    }

    #[test]
    fn fan_out_children_are_recovered_from_the_node_output() {
        let mut stored = row("SUSPENDED", Some("AWAITING_FAN_OUT"), None);
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        stored.output =
            Some(json!({ "fan_out_children": [first.to_string(), second.to_string()] }));

        let children = children_by_node(&[stored]);
        assert_eq!(children.get("n1").unwrap(), &vec![first, second]);
    }

    #[test]
    fn a_node_without_children_contributes_nothing() {
        assert!(children_by_node(&[row("PENDING", None, None)]).is_empty());
    }
}
