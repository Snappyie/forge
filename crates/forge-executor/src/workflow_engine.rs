use forge_domain::workflow::{Workflow, Node, NodeType, Edge};
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Debug)]
pub enum NodeState {
    Pending,
    Running,
    Suspended(String), // e.g. "Waiting for Approval", "Delaying for N seconds"
    Completed,
    Failed(String),
}

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

    /// Evaluates the workflow, making progress on any nodes whose dependencies are met.
    pub fn advance(&mut self) -> Vec<String> {
        let mut actions = Vec::new();
        let mut ready_nodes = self.get_ready_nodes();

        while let Some(node_id) = ready_nodes.pop_front() {
            let node = self.workflow.nodes.get(&node_id).unwrap().clone();
            
            // Mark as running
            self.node_states.insert(node_id.clone(), NodeState::Running);
            
            match &node.node_type {
                NodeType::Job { job_id } => {
                    actions.push(format!("DISPATCH_JOB:{}:{}", node_id, job_id));
                    // In a real system, the state transitions to Suspended or remains Running 
                    // until the external job completes and calls back.
                    self.node_states.insert(node_id.clone(), NodeState::Completed);
                }
                NodeType::Delay { seconds } => {
                    actions.push(format!("SCHEDULE_DELAY:{}:{}", node_id, seconds));
                    self.node_states.insert(node_id.clone(), NodeState::Suspended("Delay".to_string()));
                }
                NodeType::Approval { required_role } => {
                    actions.push(format!("REQUEST_APPROVAL:{}:{}", node_id, required_role));
                    self.node_states.insert(node_id.clone(), NodeState::Suspended("Waiting Approval".to_string()));
                }
                NodeType::Condition { expression } => {
                    // Evaluate mock expression. If true, continue on true edge.
                    let result = self.evaluate_expression(expression);
                    actions.push(format!("EVALUATED_CONDITION:{}:{}", node_id, result));
                    self.context.insert(format!("{}_result", node_id), serde_json::Value::Bool(result));
                    self.node_states.insert(node_id.clone(), NodeState::Completed);
                }
                NodeType::Map { target_node_id } => {
                    actions.push(format!("FAN_OUT:{}:{}", node_id, target_node_id));
                    self.node_states.insert(node_id.clone(), NodeState::Completed);
                }
            }
        }
        
        actions
    }

    /// Returns nodes that have no incomplete dependencies
    fn get_ready_nodes(&self) -> VecDeque<String> {
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        
        for edge in &self.workflow.edges {
            *in_degree.entry(edge.to_node.clone()).or_insert(0) += 1;
        }

        let mut ready = VecDeque::new();
        for (node_id, state) in &self.node_states {
            if matches!(state, NodeState::Pending) {
                // If it has no incoming edges, or all incoming edges are from completed nodes
                let incoming_edges: Vec<&Edge> = self.workflow.edges.iter()
                    .filter(|e| e.to_node == *node_id)
                    .collect();
                
                let all_deps_met = incoming_edges.iter().all(|e| {
                    if let Some(parent_state) = self.node_states.get(&e.from_node) {
                        if !matches!(parent_state, NodeState::Completed) {
                            return false;
                        }
                        // If it's a conditional edge, check if the condition matched
                        if let Some(cond) = &e.condition {
                            let result_key = format!("{}_result", e.from_node);
                            if let Some(serde_json::Value::Bool(res)) = self.context.get(&result_key) {
                                // E.g. condition string is "true" or "false"
                                return cond == &res.to_string();
                            }
                            return false;
                        }
                        true
                    } else {
                        false
                    }
                });

                if incoming_edges.is_empty() || all_deps_met {
                    ready.push_back(node_id.clone());
                }
            }
        }
        
        ready
    }

    fn evaluate_expression(&self, _expression: &str) -> bool {
        // Mock expression evaluator
        true
    }
    
    pub fn complete_node(&mut self, node_id: &str) {
        if self.node_states.contains_key(node_id) {
            self.node_states.insert(node_id.to_string(), NodeState::Completed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_domain::id::{JobId, TenantId};

    #[test]
    fn test_dag_execution_and_dependencies() {
        let tenant = TenantId::new();
        let mut workflow = Workflow::new(tenant, "Onboarding".to_string());
        
        workflow.add_node(Node {
            id: "step1".to_string(),
            node_type: NodeType::Job { job_id: JobId::new() },
            name: "Initial".to_string(),
        });
        
        workflow.add_node(Node {
            id: "step2".to_string(),
            node_type: NodeType::Approval { required_role: "admin".to_string() },
            name: "Approval".to_string(),
        });
        
        workflow.add_edge("step1".to_string(), "step2".to_string(), None);
        
        let mut exec = WorkflowExecution::new(workflow);
        
        // step1 has no dependencies, so it should run first.
        let actions = exec.advance();
        assert_eq!(actions.len(), 1);
        assert!(actions[0].starts_with("DISPATCH_JOB:step1"));
        
        // step1 was completed automatically in our mock implementation
        // so step2 should be ready in the next advance
        let actions2 = exec.advance();
        assert_eq!(actions2.len(), 1);
        assert!(actions2[0].starts_with("REQUEST_APPROVAL:step2"));
        
        // step2 is Suspended, so advancing again does nothing
        let actions3 = exec.advance();
        assert_eq!(actions3.len(), 0);
        
        // Manually complete step2
        exec.complete_node("step2");
        let actions4 = exec.advance();
        assert_eq!(actions4.len(), 0); // No more nodes
    }
    
    #[test]
    fn test_conditional_routing() {
        let tenant = TenantId::new();
        let mut workflow = Workflow::new(tenant, "Condition Test".to_string());
        
        workflow.add_node(Node {
            id: "cond".to_string(),
            node_type: NodeType::Condition { expression: "x > 5".to_string() },
            name: "Check".to_string(),
        });
        
        workflow.add_node(Node {
            id: "path_true".to_string(),
            node_type: NodeType::Delay { seconds: 10 },
            name: "True Path".to_string(),
        });
        
        workflow.add_node(Node {
            id: "path_false".to_string(),
            node_type: NodeType::Delay { seconds: 10 },
            name: "False Path".to_string(),
        });
        
        // condition evaluates to true in our mock, so only true path should trigger
        workflow.add_edge("cond".to_string(), "path_true".to_string(), Some("true".to_string()));
        workflow.add_edge("cond".to_string(), "path_false".to_string(), Some("false".to_string()));
        
        let mut exec = WorkflowExecution::new(workflow);
        let actions = exec.advance();
        assert_eq!(actions.len(), 1);
        assert!(actions[0].starts_with("EVALUATED_CONDITION:cond"));
        
        let actions2 = exec.advance();
        // Because cond evaluated to true, only path_true receives an action
        assert_eq!(actions2.len(), 1);
        assert!(actions2[0].starts_with("SCHEDULE_DELAY:path_true"));
    }
}
