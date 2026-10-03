use serde::{Deserialize, Serialize};
use crate::id::{JobId, TenantId};
use std::collections::HashMap;
use uuid::Uuid;

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
    pub id: String, // E.g., "step_1"
    pub node_type: NodeType,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeType {
    /// Executes a job asynchronously
    Job { job_id: JobId },
    /// Requires manual human intervention to proceed
    Approval { required_role: String },
    /// Delays execution by a specific duration
    Delay { seconds: u64 },
    /// Evaluates a condition to determine which branch to take
    Condition { expression: String },
    /// Dynamically spans multiple executions over an array of inputs
    Map { target_node_id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub from_node: String,
    pub to_node: String,
    pub condition: Option<String>, // E.g., for conditional branching (if true, if false)
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
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::JobId;

    #[test]
    fn test_workflow_creation() {
        let tenant = TenantId::new();
        let workflow = Workflow::new(tenant, "Onboarding".to_string());
        assert_eq!(workflow.name, "Onboarding");
        assert!(workflow.nodes.is_empty());
        assert!(workflow.edges.is_empty());
    }

    #[test]
    fn test_workflow_add_nodes_and_edges() {
        let tenant = TenantId::new();
        let mut workflow = Workflow::new(tenant, "Data Pipeline".to_string());
        
        let node1 = Node {
            id: "extract".to_string(),
            node_type: NodeType::Job { job_id: JobId::new() },
            name: "Extract Data".to_string(),
        };
        
        let node2 = Node {
            id: "transform".to_string(),
            node_type: NodeType::Delay { seconds: 60 },
            name: "Wait and Transform".to_string(),
        };
        
        workflow.add_node(node1.clone());
        workflow.add_node(node2.clone());
        workflow.add_edge("extract".to_string(), "transform".to_string(), None);
        
        assert_eq!(workflow.nodes.len(), 2);
        assert_eq!(workflow.edges.len(), 1);
        
        let edge = &workflow.edges[0];
        assert_eq!(edge.from_node, "extract");
        assert_eq!(edge.to_node, "transform");
        assert_eq!(edge.condition, None);
    }
}
