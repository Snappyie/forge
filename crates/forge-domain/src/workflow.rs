use serde::{Deserialize, Serialize};
use crate::error::DomainError;
use crate::id::{JobId, TenantId};
use std::collections::{BTreeMap, HashMap, VecDeque};
use uuid::Uuid;

/// Dependency condition on an edge (spec 02.11). `UpstreamSucceeded` is the
/// default when an edge declares none.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EdgeCondition {
    /// Every incoming edge's source succeeded. The default.
    #[default]
    AllSucceeded,
    /// At least one incoming edge's source succeeded.
    AnySucceeded,
    /// Every incoming edge's source reached any terminal state.
    AllCompleted,
    /// Fires regardless of upstream outcome.
    Always,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workflow {
    pub id: Uuid,
    pub tenant_id: TenantId,
    pub name: String,
    pub nodes: HashMap<String, Node>,
    pub edges: Vec<Edge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: String,
    pub node_type: NodeType,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeType {
    /// Executes a job asynchronously.
    Job { job_id: JobId },
    /// Requires manual human intervention to proceed.
    Approval { required_role: String },
    /// Delays execution by a specific duration.
    Delay { seconds: u64 },
    /// Evaluates a condition to determine which branch to take.
    Condition { expression: String },
    /// Dynamically spans multiple executions over an array of inputs.
    Map { target_node_id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub from_node: String,
    pub to_node: String,
    pub condition: Option<String>,
}

impl Workflow {
    pub fn new(tenant_id: TenantId, name: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            name,
            nodes: HashMap::new(),
            edges: Vec::new(),
        }
    }

    pub fn add_node(&mut self, node: Node) {
        self.nodes.insert(node.id.clone(), node);
    }

    pub fn add_edge(&mut self, from: String, to: String, condition: Option<String>) {
        self.edges.push(Edge {
            from_node: from,
            to_node: to,
            condition,
        });
    }

    /// Validates the graph per spec 02.11.
    ///
    /// Checks, in order: non-empty graph, edge endpoints resolve, no self
    /// loops, no duplicate edges, node configuration is sound, and the graph
    /// is acyclic. Cycle detection uses Kahn's algorithm; any node with
    /// remaining in-degree after the queue drains lies on or downstream of a
    /// cycle.
    pub fn validate(&self) -> Result<(), DomainError> {
        if self.nodes.is_empty() {
            return Err(DomainError::InvalidWorkflow(
                "a workflow must contain at least one node".to_string(),
            ));
        }

        // 1. Every edge endpoint must reference an existing node.
        for edge in &self.edges {
            if !self.nodes.contains_key(&edge.from_node) {
                return Err(DomainError::InvalidWorkflow(format!(
                    "edge references unknown source node `{}`",
                    edge.from_node
                )));
            }
            if !self.nodes.contains_key(&edge.to_node) {
                return Err(DomainError::InvalidWorkflow(format!(
                    "edge references unknown target node `{}`",
                    edge.to_node
                )));
            }
        }

        // 2. No self loops.
        for edge in &self.edges {
            if edge.from_node == edge.to_node {
                return Err(DomainError::InvalidWorkflow(format!(
                    "node `{}` cannot depend on itself",
                    edge.from_node
                )));
            }
        }

        // 3. No duplicate edges.
        let mut seen = std::collections::HashSet::new();
        for edge in &self.edges {
            let key = (&edge.from_node, &edge.to_node);
            if !seen.insert(key) {
                return Err(DomainError::InvalidWorkflow(format!(
                    "duplicate edge `{}` -> `{}`",
                    edge.from_node, edge.to_node
                )));
            }
        }

        // 4. Node-specific configuration.
        for node in self.nodes.values() {
            match &node.node_type {
                NodeType::Map { target_node_id } => {
                    if !self.nodes.contains_key(target_node_id) {
                        return Err(DomainError::InvalidWorkflow(format!(
                            "map node `{}` targets unknown node `{target_node_id}`",
                            node.id
                        )));
                    }
                    if target_node_id == &node.id {
                        return Err(DomainError::InvalidWorkflow(format!(
                            "map node `{}` cannot target itself",
                            node.id
                        )));
                    }
                }
                NodeType::Condition { expression } if expression.trim().is_empty() => {
                    return Err(DomainError::InvalidWorkflow(format!(
                        "condition node `{}` has an empty expression",
                        node.id
                    )));
                }
                _ => {}
            }
        }

        // 5. Acyclicity (spec 02.11: "The graph MUST be acyclic").
        self.assert_acyclic()?;

        Ok(())
    }

    /// Kahn's algorithm. `Ok(())` when the graph is acyclic.
    fn assert_acyclic(&self) -> Result<(), DomainError> {
        let mut in_degree: BTreeMap<&str, usize> =
            self.nodes.keys().map(|k| (k.as_str(), 0)).collect();
        let mut adjacency: BTreeMap<&str, Vec<&str>> = BTreeMap::new();

        for edge in &self.edges {
            *in_degree
                .get_mut(edge.to_node.as_str())
                .expect("endpoint validated above") += 1;
            adjacency
                .entry(edge.from_node.as_str())
                .or_default()
                .push(edge.to_node.as_str());
        }

        let mut queue: VecDeque<&str> = in_degree
            .iter()
            .filter(|(_, &d)| d == 0)
            .map(|(k, _)| *k)
            .collect();

        let mut visited = 0usize;
        while let Some(node) = queue.pop_front() {
            visited += 1;
            if let Some(children) = adjacency.get(node) {
                for child in children {
                    let degree = in_degree
                        .get_mut(child)
                        .expect("child is a known node");
                    *degree -= 1;
                    if *degree == 0 {
                        queue.push_back(child);
                    }
                }
            }
        }

        if visited != self.nodes.len() {
            let stuck: Vec<&str> = in_degree
                .iter()
                .filter(|(_, &d)| d > 0)
                .map(|(k, _)| *k)
                .collect();
            return Err(DomainError::InvalidWorkflow(format!(
                "workflow graph contains a cycle involving: {}",
                stuck.join(", ")
            )));
        }

        Ok(())
    }

    /// Nodes with no incoming edges — the entry points of the DAG.
    pub fn entry_nodes(&self) -> Vec<String> {
        let targets: std::collections::HashSet<&str> =
            self.edges.iter().map(|e| e.to_node.as_str()).collect();
        let mut entries: Vec<String> = self
            .nodes
            .keys()
            .filter(|k| !targets.contains(k.as_str()))
            .cloned()
            .collect();
        entries.sort();
        entries
    }

    /// Immediate successors of `node_id`.
    pub fn successors(&self, node_id: &str) -> Vec<&str> {
        self.edges
            .iter()
            .filter(|e| e.from_node == node_id)
            .map(|e| e.to_node.as_str())
            .collect()
    }

    /// Immediate predecessors of `node_id`.
    pub fn predecessors(&self, node_id: &str) -> Vec<&str> {
        self.edges
            .iter()
            .filter(|e| e.to_node == node_id)
            .map(|e| e.from_node.as_str())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job_node(id: &str) -> Node {
        Node {
            id: id.to_string(),
            node_type: NodeType::Job { job_id: JobId::new() },
            name: id.to_string(),
        }
    }

    fn linear_workflow() -> Workflow {
        let mut wf = Workflow::new(TenantId::new(), "linear".to_string());
        wf.add_node(job_node("a"));
        wf.add_node(job_node("b"));
        wf.add_node(job_node("c"));
        wf.add_edge("a".into(), "b".into(), None);
        wf.add_edge("b".into(), "c".into(), None);
        wf
    }

    #[test]
    fn test_workflow_creation() {
        let workflow = Workflow::new(TenantId::new(), "Onboarding".to_string());
        assert_eq!(workflow.name, "Onboarding");
        assert!(workflow.nodes.is_empty());
        assert!(workflow.edges.is_empty());
    }

    #[test]
    fn test_workflow_add_nodes_and_edges() {
        let workflow = linear_workflow();
        assert_eq!(workflow.nodes.len(), 3);
        assert_eq!(workflow.edges.len(), 2);
        assert_eq!(workflow.edges[0].from_node, "a");
        assert_eq!(workflow.edges[0].to_node, "b");
    }

    // AT-WF-001: a valid DAG validates
    #[test]
    fn valid_dag_passes_validation() {
        assert!(linear_workflow().validate().is_ok());
    }

    #[test]
    fn parallel_branches_validate() {
        let mut wf = Workflow::new(TenantId::new(), "fan-out".to_string());
        for id in ["start", "left", "right", "join"] {
            wf.add_node(job_node(id));
        }
        wf.add_edge("start".into(), "left".into(), None);
        wf.add_edge("start".into(), "right".into(), None);
        wf.add_edge("left".into(), "join".into(), None);
        wf.add_edge("right".into(), "join".into(), None);
        assert!(wf.validate().is_ok());
        assert_eq!(wf.entry_nodes(), vec!["start".to_string()]);
    }

    // AT-WF-002: a cycle is rejected
    #[test]
    fn cycle_is_rejected() {
        let mut wf = Workflow::new(TenantId::new(), "cyclic".to_string());
        for id in ["a", "b", "c"] {
            wf.add_node(job_node(id));
        }
        wf.add_edge("a".into(), "b".into(), None);
        wf.add_edge("b".into(), "c".into(), None);
        wf.add_edge("c".into(), "a".into(), None);

        let err = wf.validate().unwrap_err();
        assert!(
            matches!(err, DomainError::InvalidWorkflow(ref m) if m.contains("cycle")),
            "expected a cycle error, got: {err}"
        );
    }

    #[test]
    fn two_node_cycle_is_rejected() {
        let mut wf = Workflow::new(TenantId::new(), "cyclic".to_string());
        wf.add_node(job_node("a"));
        wf.add_node(job_node("b"));
        wf.add_edge("a".into(), "b".into(), None);
        wf.add_edge("b".into(), "a".into(), None);
        assert!(wf.validate().is_err());
    }

    /// A long chain must not be mistaken for a cycle.
    #[test]
    fn long_chain_is_not_a_cycle() {
        let mut wf = Workflow::new(TenantId::new(), "long".to_string());
        for i in 0..500 {
            wf.add_node(job_node(&format!("n{i}")));
        }
        for i in 0..499 {
            wf.add_edge(format!("n{i}"), format!("n{}", i + 1), None);
        }
        assert!(wf.validate().is_ok());
    }

    #[test]
    fn dangling_edge_is_rejected() {
        let mut wf = Workflow::new(TenantId::new(), "dangling".to_string());
        wf.add_node(job_node("a"));
        wf.add_edge("a".into(), "ghost".into(), None);
        assert!(wf.validate().is_err());
    }

    #[test]
    fn self_loop_is_rejected() {
        let mut wf = Workflow::new(TenantId::new(), "self".to_string());
        wf.add_node(job_node("a"));
        wf.add_edge("a".into(), "a".into(), None);
        assert!(wf.validate().is_err());
    }

    #[test]
    fn duplicate_edge_is_rejected() {
        let mut wf = Workflow::new(TenantId::new(), "dupe".to_string());
        wf.add_node(job_node("a"));
        wf.add_node(job_node("b"));
        wf.add_edge("a".into(), "b".into(), None);
        wf.add_edge("a".into(), "b".into(), None);
        assert!(wf.validate().is_err());
    }

    #[test]
    fn empty_graph_is_rejected() {
        let wf = Workflow::new(TenantId::new(), "empty".to_string());
        assert!(wf.validate().is_err());
    }

    #[test]
    fn map_node_pointing_at_unknown_node_is_rejected() {
        let mut wf = Workflow::new(TenantId::new(), "bad-map".to_string());
        wf.add_node(Node {
            id: "fanout".to_string(),
            node_type: NodeType::Map {
                target_node_id: "nowhere".to_string(),
            },
            name: "fanout".to_string(),
        });
        assert!(wf.validate().is_err());
    }

    #[test]
    fn map_node_pointing_at_a_real_node_is_accepted() {
        let mut wf = Workflow::new(TenantId::new(), "good-map".to_string());
        wf.add_node(job_node("target"));
        wf.add_node(Node {
            id: "fanout".to_string(),
            node_type: NodeType::Map {
                target_node_id: "target".to_string(),
            },
            name: "fanout".to_string(),
        });
        assert!(wf.validate().is_ok());
    }

    #[test]
    fn successors_and_predecessors_are_reported() {
        let wf = linear_workflow();
        assert_eq!(wf.successors("a"), vec!["b"]);
        assert_eq!(wf.predecessors("b"), vec!["a"]);
        assert!(wf.successors("c").is_empty());
    }

    #[test]
    fn default_edge_condition_is_upstream_succeeded() {
        assert_eq!(EdgeCondition::default(), EdgeCondition::AllSucceeded);
    }
}