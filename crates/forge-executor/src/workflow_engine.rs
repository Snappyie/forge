//! Workflow DAG progression (spec 02.11, spec 19 Phase 7).
//!
//! The engine decides *what* to run and *when*; dispatching the resulting work
//! is the server's job. [`WorkflowExecution::advance`] returns typed actions
//! rather than strings, so a caller cannot misparse them.

use forge_domain::workflow::{EdgeCondition, NodeType, Workflow};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Progress of one node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NodeState {
    Pending,
    Running,
    /// Waiting on an external event: an approval, a delay, or a fan-out.
    Suspended {
        reason: SuspensionReason,
        /// For a delay, when the node may resume.
        resume_at: Option<chrono::DateTime<chrono::Utc>>,
    },
    Completed,
    Failed {
        reason: String,
    },
    /// An edge condition excluded this node, so it will never run.
    Skipped,
}

/// Why a node is suspended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SuspensionReason {
    AwaitingApproval {
        required_role: String,
    },
    Delay,
    /// A map node waiting for its fan-out instances to finish.
    AwaitingFanOut,
}

/// What the caller must do next.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WorkflowAction {
    /// Dispatch the referenced job for this node.
    DispatchJob { node_id: String, job_id: uuid::Uuid },
    /// Resume after the given wall-clock instant.
    ScheduleDelay {
        node_id: String,
        resume_at: chrono::DateTime<chrono::Utc>,
    },
    /// Ask a human; resume via the approval endpoint.
    RequestApproval {
        node_id: String,
        required_role: String,
    },
    /// Run the named task once per element of the mapped array.
    FanOut {
        node_id: String,
        target_node_id: String,
        items: Vec<serde_json::Value>,
    },
    /// The node finished unsuccessfully.
    NodeFailed { node_id: String, reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ExpressionError {
    #[error("expression is empty")]
    Empty,
    #[error("unknown identifier `{0}` in expression")]
    UnknownIdentifier(String),
}

/// Evaluates condition expressions against the workflow context.
///
/// The grammar is deliberately small and total: comparison, boolean
/// combination, dotted-path lookup, and truthiness. An expression that cannot be
/// evaluated is an *error* rather than a silent `false`, because a condition
/// that never fires would otherwise strand a workflow with no explanation.
pub struct ExpressionEvaluator<'a> {
    context: &'a HashMap<String, serde_json::Value>,
}

impl<'a> ExpressionEvaluator<'a> {
    pub fn new(context: &'a HashMap<String, serde_json::Value>) -> Self {
        Self { context }
    }

    /// Resolves a path such as `nodes.extract.output.count` or `items[0]`.
    ///
    /// Supports dotted member access and bracketed integer indexes.
    pub fn lookup(&self, path: &str) -> Option<&serde_json::Value> {
        let first = path.split(['.', '[']).next()?;
        // A bare name checks the context first, then node output.
        let mut current: &serde_json::Value = self
            .context
            .get(first)
            .or_else(|| self.context.get(&format!("nodes.{first}")))?;

        // Walk the remaining segments, each either `.name` or `[index]`.
        let mut rest = &path[first.len()..];
        while !rest.is_empty() {
            if let Some(stripped) = rest.strip_prefix('.') {
                let (name, remainder) = match stripped.find(['.', '[']) {
                    Some(i) => stripped.split_at(i),
                    None => (stripped, ""),
                };
                current = match current {
                    serde_json::Value::Object(map) => map.get(name)?,
                    _ => return None,
                };
                rest = remainder;
            } else if let Some(stripped) = rest.strip_prefix('[') {
                let close = stripped.find(']')?;
                let index: usize = stripped[..close].trim().parse().ok()?;
                current = match current {
                    serde_json::Value::Array(items) => items.get(index)?,
                    _ => return None,
                };
                rest = &stripped[close + 1..];
            } else {
                return None;
            }
        }

        Some(current)
    }

    /// Evaluates an expression to a boolean.
    pub fn evaluate(&self, expression: &str) -> Result<bool, ExpressionError> {
        let expr = expression.trim();
        if expr.is_empty() {
            return Err(ExpressionError::Empty);
        }

        // `or` before `and`, so `a or b and c` groups as expected.
        if let Some(idx) = find_top_level(expr, " or ") {
            let (l, r) = expr.split_at(idx);
            return Ok(self.evaluate(l)? || self.evaluate(r.trim_start_matches(" or "))?);
        }
        if let Some(idx) = find_top_level(expr, " and ") {
            let (l, r) = expr.split_at(idx);
            return Ok(self.evaluate(l)? && self.evaluate(r.trim_start_matches(" and "))?);
        }

        // Two-character comparison forms first, so `<=` is not read as `<`
        // followed by a stray `=`.
        for op in ["==", "!=", ">=", "<=", ">", "<"] {
            if let Some(idx) = find_top_level(expr, op) {
                let (l, r) = expr.split_at(idx);
                let left = self.value(l.trim())?;
                let right = self.value(r.trim_start_matches(op).trim())?;
                return Ok(compare(&left, &right, op));
            }
        }

        self.value(expr)?.truthy()
    }

    /// Resolves a literal or a context reference.
    fn value(&self, token: &str) -> Result<serde_json::Value, ExpressionError> {
        let token = token.trim();
        match token {
            "true" => return Ok(serde_json::Value::Bool(true)),
            "false" => return Ok(serde_json::Value::Bool(false)),
            "null" | "" => return Ok(serde_json::Value::Null),
            _ => {}
        }
        if let Ok(n) = token.parse::<i64>() {
            return Ok(serde_json::Value::from(n));
        }
        if let Ok(f) = token.parse::<f64>() {
            return Ok(serde_json::json!(f));
        }
        let quoted = (token.starts_with('"') && token.ends_with('"') && token.len() >= 2)
            || (token.starts_with('\'') && token.ends_with('\'') && token.len() >= 2);
        if quoted {
            return Ok(serde_json::Value::String(
                token[1..token.len() - 1].to_string(),
            ));
        }

        self.lookup(token)
            .cloned()
            .ok_or_else(|| ExpressionError::UnknownIdentifier(token.to_string()))
    }
}

/// Finds a top-level occurrence of `needle`, ignoring any inside brackets or
/// quotes.
fn find_top_level(haystack: &str, needle: &str) -> Option<usize> {
    let mut depth = 0usize;
    let mut in_quotes: Option<char> = None;

    for (i, ch) in haystack.char_indices() {
        if let Some(q) = in_quotes {
            if ch == q {
                in_quotes = None;
            }
            continue;
        }
        match ch {
            '"' | '\'' => in_quotes = Some(ch),
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            _ => {}
        }
        if depth == 0 && haystack[i..].starts_with(needle) {
            return Some(i);
        }
    }
    None
}

fn compare(left: &serde_json::Value, right: &serde_json::Value, op: &str) -> bool {
    match op {
        "==" => left == right,
        "!=" => left != right,
        _ => {
            // Ordering is numeric only; comparing unrelated types is false rather
            // than an error, so a stray type cannot wedge a workflow.
            let (Some(l), Some(r)) = (as_f64(left), as_f64(right)) else {
                return false;
            };
            match op {
                ">" => l > r,
                "<" => l < r,
                ">=" => l >= r,
                "<=" => l <= r,
                _ => false,
            }
        }
    }
}

fn as_f64(v: &serde_json::Value) -> Option<f64> {
    match v {
        serde_json::Value::Number(n) => n.as_f64(),
        _ => None,
    }
}

trait Truthy {
    fn truthy(&self) -> Result<bool, ExpressionError>;
}

impl Truthy for serde_json::Value {
    fn truthy(&self) -> Result<bool, ExpressionError> {
        match self {
            serde_json::Value::Bool(b) => Ok(*b),
            serde_json::Value::Null => Ok(false),
            serde_json::Value::Number(n) => Ok(n.as_f64().unwrap_or(0.0) != 0.0),
            serde_json::Value::String(s) => Ok(!s.is_empty() && s != "false"),
            serde_json::Value::Array(a) => Ok(!a.is_empty()),
            serde_json::Value::Object(o) => Ok(!o.is_empty()),
        }
    }
}

/// A workflow run in progress.
pub struct WorkflowExecution {
    pub workflow: Workflow,
    pub node_states: HashMap<String, NodeState>,
    pub context: HashMap<String, serde_json::Value>,
}

impl WorkflowExecution {
    pub fn new(workflow: Workflow) -> Self {
        let mut states = HashMap::new();
        for node_id in workflow.nodes.keys() {
            states.insert(node_id.clone(), NodeState::Pending);
        }
        Self {
            workflow,
            node_states: states,
            context: HashMap::new(),
        }
    }

    /// Evaluates the workflow once, returning the work the caller must perform.
    pub fn advance(&mut self) -> Result<Vec<WorkflowAction>, ExpressionError> {
        self.advance_at(chrono::Utc::now())
    }

    /// As [`WorkflowExecution::advance`], with an explicit clock so a delay can be
    /// released deterministically in tests.
    pub fn advance_at(
        &mut self,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<WorkflowAction>, ExpressionError> {
        let mut actions = Vec::new();
        let mut progressed = true;

        // One pass can unblock further nodes — a resolved condition releases a
        // downstream branch — so keep going until nothing more can start.
        while progressed {
            progressed = false;
            for node_id in self.ready_nodes(now) {
                if let Some(action) = self.start_node(&node_id, now)? {
                    actions.push(action);
                }
                progressed = true;
            }
        }

        Ok(actions)
    }

    /// Begins a node, returning the action its type requires.
    fn start_node(
        &mut self,
        node_id: &str,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<Option<WorkflowAction>, ExpressionError> {
        let node = match self.workflow.nodes.get(node_id) {
            Some(n) => n.clone(),
            None => return Ok(None),
        };

        let action = match &node.node_type {
            NodeType::Job { job_id } => {
                // A job node stays Running until the dispatcher reports back.
                // Marking it Completed here would let downstream nodes start
                // before the work had actually finished.
                self.node_states
                    .insert(node_id.to_string(), NodeState::Running);
                Some(WorkflowAction::DispatchJob {
                    node_id: node_id.to_string(),
                    job_id: job_id.into_uuid(),
                })
            }

            NodeType::Delay { seconds } => {
                // An elapsed delay is being released: complete it rather than
                // scheduling another wait.
                if matches!(
                    self.node_states.get(node_id),
                    Some(NodeState::Suspended {
                        reason: SuspensionReason::Delay,
                        resume_at: Some(at),
                    }) if *at <= now
                ) {
                    self.node_states
                        .insert(node_id.to_string(), NodeState::Completed);
                    None
                } else {
                    let resume_at = now + chrono::Duration::seconds(*seconds as i64);
                    self.node_states.insert(
                        node_id.to_string(),
                        NodeState::Suspended {
                            reason: SuspensionReason::Delay,
                            resume_at: Some(resume_at),
                        },
                    );
                    Some(WorkflowAction::ScheduleDelay {
                        node_id: node_id.to_string(),
                        resume_at,
                    })
                }
            }

            NodeType::Approval { required_role } => {
                self.node_states.insert(
                    node_id.to_string(),
                    NodeState::Suspended {
                        reason: SuspensionReason::AwaitingApproval {
                            required_role: required_role.clone(),
                        },
                        resume_at: None,
                    },
                );
                Some(WorkflowAction::RequestApproval {
                    node_id: node_id.to_string(),
                    required_role: required_role.clone(),
                })
            }

            NodeType::Condition { expression } => {
                let evaluator = ExpressionEvaluator::new(&self.context);
                let result = evaluator.evaluate(expression)?;
                // Publish the result so downstream conditional edges can read it.
                self.context
                    .insert(format!("{node_id}_result"), serde_json::json!(result));
                self.node_states
                    .insert(node_id.to_string(), NodeState::Completed);
                None
            }

            NodeType::Map { target_node_id } => {
                let items = self
                    .context
                    .get("map_items")
                    .or_else(|| self.context.get(target_node_id))
                    .and_then(|v| v.as_array().cloned())
                    .unwrap_or_default();

                if items.is_empty() {
                    // Nothing to fan out over: complete rather than stall forever.
                    self.node_states
                        .insert(node_id.to_string(), NodeState::Completed);
                    None
                } else {
                    self.node_states.insert(
                        node_id.to_string(),
                        NodeState::Suspended {
                            reason: SuspensionReason::AwaitingFanOut,
                            resume_at: None,
                        },
                    );
                    Some(WorkflowAction::FanOut {
                        node_id: node_id.to_string(),
                        target_node_id: target_node_id.clone(),
                        items,
                    })
                }
            }
        };

        Ok(action)
    }

    /// Nodes whose incoming edges are satisfied and that have not yet run.
    fn ready_nodes(&self, now: chrono::DateTime<chrono::Utc>) -> Vec<String> {
        let mut ready = Vec::new();

        for (node_id, state) in &self.node_states {
            match state {
                // A delay whose time has come resumes itself.
                NodeState::Suspended {
                    reason: SuspensionReason::Delay,
                    resume_at: Some(at),
                } if *at <= now => {
                    ready.push(node_id.clone());
                    continue;
                }
                NodeState::Pending => {}
                _ => continue,
            }

            if self.dependencies_satisfied(node_id) {
                ready.push(node_id.clone());
            }
        }

        // Deterministic order, so a run is reproducible.
        ready.sort();
        ready
    }

    /// Whether `node_id`'s incoming edges admit it.
    ///
    /// Under the default `ALL_SUCCEEDED` — which spec 02.11 makes the default
    /// edge condition — every incoming edge must be satisfied, so a fan-in waits
    /// for all of its branches (AT-WF-005) and a single failed parent keeps the
    /// node from running (AT-WF-006). `ANY_SUCCEEDED` relaxes that to "at least
    /// one".
    fn dependencies_satisfied(&self, node_id: &str) -> bool {
        let incoming: Vec<&forge_domain::workflow::Edge> = self
            .workflow
            .edges
            .iter()
            .filter(|e| e.to_node == node_id)
            .collect();

        if incoming.is_empty() {
            return true;
        }

        let require_all = incoming
            .iter()
            .all(|e| parse_edge_condition(e.condition.as_deref()) != EdgeCondition::AnySucceeded);

        let satisfied = |edge: &forge_domain::workflow::Edge| -> bool {
            let parent_state = self.node_states.get(&edge.from_node);
            let parent_completed = matches!(parent_state, Some(NodeState::Completed));
            let parent_finished = matches!(
                parent_state,
                Some(NodeState::Completed) | Some(NodeState::Failed { .. })
            );

            match parse_edge_condition(edge.condition.as_deref()) {
                EdgeCondition::Always => true,
                EdgeCondition::AllSucceeded | EdgeCondition::AnySucceeded => {
                    // A `true`/`false` edge label is a *branch selector*: it
                    // only fires when the parent condition node produced that
                    // result. Treating it as a plain dependency would run both
                    // branches of an if/else.
                    if let Some(result) = self.condition_result(&edge.from_node) {
                        return branch_label_matches(edge.condition.as_deref(), result);
                    }
                    parent_completed
                }
                EdgeCondition::AllCompleted => parent_finished,
            }
        };

        if require_all {
            // Every parent must have finished successfully, unless an edge opts
            // into a looser condition such as ALL_COMPLETED.
            incoming.iter().all(|e| satisfied(e))
        } else {
            incoming.iter().any(|e| satisfied(e))
        }
    }

    /// The boolean a condition node most recently produced, if it is one.
    fn condition_result(&self, node_id: &str) -> Option<bool> {
        // A node only publishes a result if it is a condition node.
        let is_condition = matches!(
            self.workflow.nodes.get(node_id).map(|n| &n.node_type),
            Some(NodeType::Condition { .. })
        );
        if !is_condition {
            return None;
        }
        match self.context.get(&format!("{node_id}_result")) {
            Some(serde_json::Value::Bool(b)) => Some(*b),
            _ => None,
        }
    }

    /// Records a node's successful completion, publishing its output.
    ///
    /// Output is stored under `context["nodes"][<id>]["output"]` as a nested
    /// structure, so a later condition can address it as
    /// `nodes.<id>.output.<field>`. A flattened key would not be walkable by
    /// the dotted-path resolver.
    pub fn complete_node(&mut self, node_id: &str, output: Option<serde_json::Value>) {
        if self.node_states.contains_key(node_id) {
            self.node_states
                .insert(node_id.to_string(), NodeState::Completed);
            if let Some(output) = output {
                let nodes = self
                    .context
                    .entry("nodes".to_string())
                    .or_insert_with(|| serde_json::Value::Object(Default::default()));
                if let serde_json::Value::Object(map) = nodes {
                    map.insert(node_id.to_string(), serde_json::json!({ "output": output }));
                }
            }
        }
    }

    /// Records a node failure.
    ///
    /// AT-WF-006: an upstream failure propagates according to the dependency
    /// policy, so downstream successors do not proceed.
    pub fn fail_node(&mut self, node_id: &str, reason: &str) {
        if self.node_states.contains_key(node_id) {
            self.node_states.insert(
                node_id.to_string(),
                NodeState::Failed {
                    reason: reason.to_string(),
                },
            );
        }
    }

    /// Resumes a suspended node — an approval decision, or a finished fan-out.
    pub fn resume(&mut self, node_id: &str) -> bool {
        if matches!(
            self.node_states.get(node_id),
            Some(NodeState::Suspended { .. })
        ) {
            self.node_states
                .insert(node_id.to_string(), NodeState::Completed);
            true
        } else {
            false
        }
    }

    /// Rejects a suspended approval node.
    pub fn reject(&mut self, node_id: &str, reason: &str) -> bool {
        if matches!(
            self.node_states.get(node_id),
            Some(NodeState::Suspended { .. })
        ) {
            self.fail_node(node_id, reason);
            true
        } else {
            false
        }
    }

    /// Whether the whole run has settled.
    pub fn is_complete(&self) -> bool {
        !self.node_states.is_empty()
            && self.node_states.values().all(|s| {
                matches!(
                    s,
                    NodeState::Completed | NodeState::Failed { .. } | NodeState::Skipped
                )
            })
    }

    /// Whether the run ended with a failure.
    pub fn has_failed(&self) -> bool {
        self.node_states
            .values()
            .any(|s| matches!(s, NodeState::Failed { .. }))
    }
}

/// Whether a `true`/`false` edge label matches the result a condition node
/// produced. An unrecognised label matches either result.
fn branch_label_matches(label: Option<&str>, result: bool) -> bool {
    match label.map(str::trim).map(str::to_ascii_lowercase) {
        Some(ref l) if l == "true" || l == "yes" || l == "on" => result,
        Some(ref l) if l == "false" || l == "no" || l == "off" => !result,
        _ => true,
    }
}

fn parse_edge_condition(raw: Option<&str>) -> EdgeCondition {
    match raw {
        None => EdgeCondition::AllSucceeded,
        Some(text) => match text
            .trim()
            .to_ascii_uppercase()
            .replace([' ', '-'], "_")
            .as_str()
        {
            "ANY_SUCCEEDED" => EdgeCondition::AnySucceeded,
            "ALL_COMPLETED" => EdgeCondition::AllCompleted,
            "ALWAYS" => EdgeCondition::Always,
            // `true`/`false` labels on a condition node's branches both map to
            // the default; the branch itself is chosen by the condition result.
            _ => EdgeCondition::AllSucceeded,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_domain::id::{JobId, TenantId};
    use forge_domain::workflow::Node;
    use serde_json::json;

    fn job_node(id: &str) -> Node {
        Node {
            id: id.to_string(),
            node_type: NodeType::Job {
                job_id: JobId::new(),
            },
            name: id.to_string(),
        }
    }

    fn linear() -> Workflow {
        let mut wf = Workflow::new(TenantId::new(), "linear".into());
        for id in ["a", "b", "c"] {
            wf.add_node(job_node(id));
        }
        wf.add_edge("a".into(), "b".into(), None);
        wf.add_edge("b".into(), "c".into(), None);
        wf
    }

    fn at(s: &str) -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::parse_from_rfc3339(s)
            .unwrap()
            .with_timezone(&chrono::Utc)
    }

    // AT-WF-003: a job node waits for the dispatcher rather than self-completing.
    #[test]
    fn a_job_node_stays_running_until_reported() {
        let mut run = WorkflowExecution::new(linear());
        let actions = run.advance().unwrap();

        assert_eq!(actions.len(), 1, "only the entry node starts");
        assert!(matches!(actions[0], WorkflowAction::DispatchJob { .. }));
        assert_eq!(run.node_states["a"], NodeState::Running);
        assert_eq!(run.node_states["b"], NodeState::Pending);
    }

    #[test]
    fn completing_a_node_releases_its_successor() {
        let mut run = WorkflowExecution::new(linear());
        run.advance().unwrap();
        run.complete_node("a", Some(json!({ "count": 1 })));

        let actions = run.advance().unwrap();
        assert_eq!(actions.len(), 1);
        assert!(
            matches!(&actions[0], WorkflowAction::DispatchJob { node_id, .. } if node_id == "b")
        );
    }

    // AT-WF-004: parallel branches both start.
    #[test]
    fn parallel_branches_both_run() {
        let mut wf = Workflow::new(TenantId::new(), "fan".into());
        for id in ["start", "left", "right"] {
            wf.add_node(job_node(id));
        }
        wf.add_edge("start".into(), "left".into(), None);
        wf.add_edge("start".into(), "right".into(), None);

        let mut run = WorkflowExecution::new(wf);
        run.advance().unwrap();
        run.complete_node("start", None);

        assert_eq!(run.advance().unwrap().len(), 2, "both branches start");
    }

    // AT-WF-005: fan-in waits for every required branch.
    #[test]
    fn fan_in_waits_for_all_branches() {
        let mut wf = Workflow::new(TenantId::new(), "join".into());
        for id in ["start", "left", "right", "join"] {
            wf.add_node(job_node(id));
        }
        wf.add_edge("start".into(), "left".into(), None);
        wf.add_edge("start".into(), "right".into(), None);
        wf.add_edge("left".into(), "join".into(), None);
        wf.add_edge("right".into(), "join".into(), None);

        let mut run = WorkflowExecution::new(wf);
        run.advance().unwrap();
        run.complete_node("start", None);
        run.advance().unwrap();

        run.complete_node("left", None);
        assert!(
            run.advance().unwrap().is_empty(),
            "the join must wait for the right branch"
        );

        run.complete_node("right", None);
        assert_eq!(run.advance().unwrap().len(), 1, "now the join can run");
    }

    // AT-WF-006: an upstream failure blocks downstream nodes.
    #[test]
    fn a_failure_blocks_downstream_nodes() {
        let mut run = WorkflowExecution::new(linear());
        run.advance().unwrap();
        run.fail_node("a", "job exited non-zero");
        assert!(run.has_failed());

        assert!(
            run.advance().unwrap().is_empty(),
            "downstream must not proceed after a failure"
        );
        assert_eq!(run.node_states["b"], NodeState::Pending);
    }

    #[test]
    fn an_all_completed_edge_admits_a_failed_parent() {
        let mut wf = Workflow::new(TenantId::new(), "collect".into());
        for id in ["try", "join"] {
            wf.add_node(job_node(id));
        }
        wf.add_edge("try".into(), "join".into(), Some("ALL_COMPLETED".into()));

        let mut run = WorkflowExecution::new(wf);
        run.advance().unwrap();
        run.fail_node("try", "boom");

        assert_eq!(
            run.advance().unwrap().len(),
            1,
            "ALL_COMPLETED admits a failed parent"
        );
    }

    #[test]
    fn delay_nodes_suspend_then_resume_at_their_instant() {
        let now = at("2026-10-03T00:00:00Z");
        let mut wf = Workflow::new(TenantId::new(), "wait".into());
        wf.add_node(Node {
            id: "hold".into(),
            node_type: NodeType::Delay { seconds: 60 },
            name: "hold".into(),
        });
        wf.add_node(job_node("after"));
        wf.add_edge("hold".into(), "after".into(), None);

        let mut run = WorkflowExecution::new(wf);
        let actions = run.advance_at(now).unwrap();
        assert!(matches!(actions[0], WorkflowAction::ScheduleDelay { .. }));
        assert!(matches!(
            run.node_states["hold"],
            NodeState::Suspended {
                reason: SuspensionReason::Delay,
                ..
            }
        ));

        // Before it elapses, nothing else runs.
        assert!(run
            .advance_at(now + chrono::Duration::seconds(30))
            .unwrap()
            .is_empty());

        // After it, the delay completes and the successor starts.
        let later = run.advance_at(now + chrono::Duration::seconds(61)).unwrap();
        assert_eq!(run.node_states["hold"], NodeState::Completed);
        assert_eq!(
            later.len(),
            1,
            "the successor starts once the delay elapses"
        );
    }

    #[test]
    fn approval_nodes_suspend_and_resume_on_decision() {
        let mut wf = Workflow::new(TenantId::new(), "approve".into());
        wf.add_node(Node {
            id: "gate".into(),
            node_type: NodeType::Approval {
                required_role: "ADMIN".into(),
            },
            name: "gate".into(),
        });
        wf.add_node(job_node("after"));
        wf.add_edge("gate".into(), "after".into(), None);

        let mut run = WorkflowExecution::new(wf);
        let actions = run.advance().unwrap();
        assert_eq!(
            actions[0],
            WorkflowAction::RequestApproval {
                node_id: "gate".into(),
                required_role: "ADMIN".into(),
            }
        );
        assert!(
            run.advance().unwrap().is_empty(),
            "nothing proceeds while waiting for approval"
        );

        assert!(run.resume("gate"));
        assert_eq!(run.advance().unwrap().len(), 1);
    }

    #[test]
    fn a_rejected_approval_fails_the_branch() {
        let mut wf = Workflow::new(TenantId::new(), "approve".into());
        wf.add_node(Node {
            id: "gate".into(),
            node_type: NodeType::Approval {
                required_role: "ADMIN".into(),
            },
            name: "gate".into(),
        });
        wf.add_node(job_node("after"));
        wf.add_edge("gate".into(), "after".into(), None);

        let mut run = WorkflowExecution::new(wf);
        run.advance().unwrap();
        assert!(run.reject("gate", "not authorised"));
        assert!(run.has_failed());
        assert!(run.advance().unwrap().is_empty());
    }

    // --- expressions ---

    fn ctx(pairs: &[(&str, serde_json::Value)]) -> HashMap<String, serde_json::Value> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }

    #[test]
    fn expressions_compare_literals_and_references() {
        let context = ctx(&[("count", json!(5))]);
        let ev = ExpressionEvaluator::new(&context);

        assert!(ev.evaluate("count > 3").unwrap());
        assert!(!ev.evaluate("count > 10").unwrap());
        assert!(ev.evaluate("count == 5").unwrap());
        assert!(ev.evaluate("count != 4").unwrap());
        assert!(ev.evaluate("count >= 5 and count <= 5").unwrap());
        assert!(ev.evaluate("count > 10 or count == 5").unwrap());
        assert!(ev.evaluate("true").unwrap());
        assert!(!ev.evaluate("false").unwrap());
    }

    #[test]
    fn expressions_resolve_dotted_paths_and_indexes() {
        // The engine publishes node output under `nodes.<id>.output`, so the
        // path `nodes.extract.output.items[0]` walks that nested structure.
        let context = ctx(&[(
            "nodes",
            json!({ "extract": { "output": { "items": [1, 2, 3] } } }),
        )]);
        let ev = ExpressionEvaluator::new(&context);
        assert!(ev.evaluate("nodes.extract.output.items[0] == 1").unwrap());
        assert!(ev.evaluate("nodes.extract.output.items[2] == 3").unwrap());
    }

    #[test]
    fn node_output_published_by_the_engine_is_addressable() {
        // `complete_node` writes `nodes.<id>.output`; a later condition must be
        // able to read it back.
        let mut wf = Workflow::new(TenantId::new(), "publish".into());
        wf.add_node(job_node("extract"));
        wf.add_node(Node {
            id: "check".into(),
            node_type: NodeType::Condition {
                expression: "nodes.extract.output.count > 5".into(),
            },
            name: "check".into(),
        });
        wf.add_edge("extract".into(), "check".into(), None);

        let mut run = WorkflowExecution::new(wf);
        run.advance().unwrap();
        run.complete_node("extract", Some(json!({ "count": 10 })));
        run.advance().unwrap();

        assert_eq!(
            run.node_states["check"],
            NodeState::Completed,
            "the condition must have resolved from the published output"
        );
        assert_eq!(run.context.get("check_result"), Some(&json!(true)));
    }

    #[test]
    fn strings_compare_as_strings() {
        let context = ctx(&[("status", json!("SETTLED"))]);
        let ev = ExpressionEvaluator::new(&context);
        assert!(ev.evaluate(r#"status == "SETTLED""#).unwrap());
        assert!(!ev.evaluate(r#"status == "FAILED""#).unwrap());
    }

    /// The previous engine returned `true` for every expression; an unknown
    /// identifier is now an error rather than a silent pass.
    #[test]
    fn unknown_identifiers_are_errors_not_false() {
        // Bind the context so the borrow outlives the evaluator.
        let context = ctx(&[]);
        let ev = ExpressionEvaluator::new(&context);
        assert!(matches!(
            ev.evaluate("missing > 1").unwrap_err(),
            ExpressionError::UnknownIdentifier(_)
        ));
        assert!(ev.evaluate("").is_err());
    }

    #[test]
    fn a_condition_node_publishes_its_result_for_downstream_edges() {
        let mut wf = Workflow::new(TenantId::new(), "branch".into());
        wf.add_node(Node {
            id: "check".into(),
            node_type: NodeType::Condition {
                expression: "count > 3".into(),
            },
            name: "check".into(),
        });
        for id in ["when_true", "when_false"] {
            wf.add_node(job_node(id));
        }
        wf.add_edge("check".into(), "when_true".into(), Some("true".into()));
        wf.add_edge("check".into(), "when_false".into(), Some("false".into()));

        let mut run = WorkflowExecution::new(wf);
        run.context.insert("count".into(), json!(10));

        // A single advance walks the graph as far as it can: the condition
        // resolves and the matching branch starts in the same pass.
        let actions = run.advance().unwrap();
        assert_eq!(run.node_states["check"], NodeState::Completed);
        assert_eq!(actions.len(), 1, "only the true branch runs: {actions:?}");
        assert!(
            matches!(&actions[0], WorkflowAction::DispatchJob { node_id, .. } if node_id == "when_true")
        );
    }

    #[test]
    fn the_false_branch_runs_when_the_condition_is_false() {
        let mut wf = Workflow::new(TenantId::new(), "branch".into());
        wf.add_node(Node {
            id: "check".into(),
            node_type: NodeType::Condition {
                expression: "count > 3".into(),
            },
            name: "check".into(),
        });
        for id in ["when_true", "when_false"] {
            wf.add_node(job_node(id));
        }
        wf.add_edge("check".into(), "when_true".into(), Some("true".into()));
        wf.add_edge("check".into(), "when_false".into(), Some("false".into()));

        let mut run = WorkflowExecution::new(wf);
        run.context.insert("count".into(), json!(1));

        let actions = run.advance().unwrap();
        assert_eq!(actions.len(), 1, "only the false branch runs: {actions:?}");
        assert!(
            matches!(&actions[0], WorkflowAction::DispatchJob { node_id, .. } if node_id == "when_false")
        );
    }

    #[test]
    fn a_condition_that_cannot_be_evaluated_is_an_error() {
        let mut wf = Workflow::new(TenantId::new(), "branch".into());
        wf.add_node(Node {
            id: "check".into(),
            node_type: NodeType::Condition {
                expression: "missing_field > 1".into(),
            },
            name: "check".into(),
        });

        let mut run = WorkflowExecution::new(wf);
        // The old engine returned `true` for anything; an unresolvable
        // identifier must surface rather than silently pick a branch.
        assert!(run.advance().is_err());
    }

    #[test]
    fn a_map_node_fans_out_over_an_array() {
        let mut wf = Workflow::new(TenantId::new(), "fan".into());
        wf.add_node(Node {
            id: "spread".into(),
            node_type: NodeType::Map {
                target_node_id: "target".into(),
            },
            name: "spread".into(),
        });
        wf.add_node(job_node("target"));
        wf.add_edge("spread".into(), "target".into(), None);

        let mut run = WorkflowExecution::new(wf);
        run.context
            .insert("map_items".into(), json!(["a", "b", "c"]));

        let actions = run.advance().unwrap();
        assert_eq!(
            actions[0],
            WorkflowAction::FanOut {
                node_id: "spread".into(),
                target_node_id: "target".into(),
                items: vec![json!("a"), json!("b"), json!("c")],
            }
        );
        assert!(matches!(
            run.node_states["spread"],
            NodeState::Suspended {
                reason: SuspensionReason::AwaitingFanOut,
                ..
            }
        ));

        // The fan-out finishing releases the node.
        run.resume("spread");
        assert_eq!(run.node_states["spread"], NodeState::Completed);
    }

    #[test]
    fn a_map_node_over_an_empty_array_completes_without_work() {
        let mut wf = Workflow::new(TenantId::new(), "fan".into());
        wf.add_node(Node {
            id: "spread".into(),
            node_type: NodeType::Map {
                target_node_id: "target".into(),
            },
            name: "spread".into(),
        });
        wf.add_node(job_node("target"));
        wf.add_edge("spread".into(), "target".into(), None);

        let mut run = WorkflowExecution::new(wf);
        // With no items there is no fan-out, so the map node completes and the
        // successor runs in the same pass — the workflow must not stall.
        let actions = run.advance().unwrap();
        assert_eq!(run.node_states["spread"], NodeState::Completed);
        assert_eq!(actions.len(), 1, "only the successor runs: {actions:?}");
    }

    #[test]
    fn completion_is_reported_once_everything_settles() {
        let mut run = WorkflowExecution::new(linear());
        run.advance().unwrap();
        assert!(!run.is_complete());
        for node in ["a", "b", "c"] {
            run.complete_node(node, None);
        }
        assert!(run.is_complete());
        assert!(!run.has_failed());
    }

    #[test]
    fn edge_conditions_parse_from_their_stored_text() {
        assert_eq!(parse_edge_condition(None), EdgeCondition::AllSucceeded);
        assert_eq!(
            parse_edge_condition(Some("ANY_SUCCEEDED")),
            EdgeCondition::AnySucceeded
        );
        assert_eq!(
            parse_edge_condition(Some("all_completed")),
            EdgeCondition::AllCompleted
        );
        assert_eq!(parse_edge_condition(Some("ALWAYS")), EdgeCondition::Always);
        assert_eq!(
            parse_edge_condition(Some("nonsense")),
            EdgeCondition::AllSucceeded
        );
    }
}
