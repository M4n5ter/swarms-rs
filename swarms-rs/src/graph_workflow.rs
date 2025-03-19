use std::{
    collections::{HashMap, hash_map},
    sync::Arc,
};

use dashmap::DashMap;
use petgraph::{
    Direction,
    graph::{EdgeIndex, NodeIndex},
    prelude::StableGraph,
    visit::EdgeRef,
};
use thiserror::Error;
use tokio::sync::Mutex;

use crate::agent::Agent;

// The main orchestration structure
pub struct DAGWorkflow {
    name: String,
    description: String,
    // Store all registered agents
    agents: DashMap<String, Box<dyn Agent>>,
    // The workflow graph
    workflow: StableGraph<AgentNode, Flow>,
    // Map from agent name to node index for quick lookup
    name_to_node: HashMap<String, NodeIndex>,
}

impl DAGWorkflow {
    pub fn new<S: Into<String>>(name: S, description: S) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            agents: DashMap::new(),
            workflow: StableGraph::new(),
            name_to_node: HashMap::new(),
        }
    }

    // Register an agent with the orchestrator
    pub fn register_agent(&mut self, agent: Box<dyn Agent>) {
        let agent_name = agent.name();
        self.agents.insert(agent_name.clone(), agent);

        // If agent isn't already in the graph, add it
        if let hash_map::Entry::Vacant(e) = self.name_to_node.entry(agent_name.clone()) {
            let node_idx = self.workflow.add_node(AgentNode {
                name: agent_name.clone(),
                last_result: Mutex::new(None),
            });
            e.insert(node_idx);
        }
    }

    // Add a flow connection between two agents
    pub fn connect_agents(
        &mut self,
        from: &str,
        to: &str,
        flow: Flow,
    ) -> Result<EdgeIndex, GraphWorkflowError> {
        // Ensure both agents exist
        if !self.agents.contains_key(from) {
            return Err(GraphWorkflowError::AgentNotFound(format!(
                "Source agent '{}' not found",
                from
            )));
        }
        if !self.agents.contains_key(to) {
            return Err(GraphWorkflowError::AgentNotFound(format!(
                "Target agent '{}' not found",
                to
            )));
        }

        // Get node indices, creating nodes if necessary
        let from_entry = self.name_to_node.entry(from.to_string());
        let from_idx = *from_entry.or_insert_with(|| {
            self.workflow.add_node(AgentNode {
                name: from.to_string(),
                last_result: Mutex::new(None),
            })
        });

        let to_entry = self.name_to_node.entry(to.to_string());
        let to_idx = *to_entry.or_insert_with(|| {
            self.workflow.add_node(AgentNode {
                name: to.to_string(),
                last_result: Mutex::new(None),
            })
        });

        // Add the edge
        let edge_idx = self.workflow.add_edge(from_idx, to_idx, flow);

        // Check for cycles (optional but recommended)
        if self.has_cycle() {
            // Remove the edge we just added
            self.workflow.remove_edge(edge_idx);
            return Err(GraphWorkflowError::CycleDetected);
        }

        Ok(edge_idx)
    }

    // Check if the workflow has a cycle
    fn has_cycle(&self) -> bool {
        // Implementation using DFS to detect cycles
        let mut visited = vec![false; self.workflow.node_count()];
        let mut rec_stack = vec![false; self.workflow.node_count()];

        for node in self.workflow.node_indices() {
            if !visited[node.index()] && self.is_cyclic_util(node, &mut visited, &mut rec_stack) {
                return true;
            }
        }
        false
    }

    fn is_cyclic_util(
        &self,
        node: NodeIndex,
        visited: &mut [bool],
        rec_stack: &mut [bool],
    ) -> bool {
        visited[node.index()] = true;
        rec_stack[node.index()] = true;

        for neighbor in self.workflow.neighbors_directed(node, Direction::Outgoing) {
            if !visited[neighbor.index()] {
                if self.is_cyclic_util(neighbor, visited, rec_stack) {
                    return true;
                }
            } else if rec_stack[neighbor.index()] {
                return true;
            }
        }

        rec_stack[node.index()] = false;
        false
    }

    // Remove an agent connection
    pub fn disconnect_agents(&mut self, from: &str, to: &str) -> Result<(), GraphWorkflowError> {
        let from_idx = self.name_to_node.get(from).ok_or_else(|| {
            GraphWorkflowError::AgentNotFound(format!("Source agent '{}' not found", from))
        })?;
        let to_idx = self.name_to_node.get(to).ok_or_else(|| {
            GraphWorkflowError::AgentNotFound(format!("Target agent '{}' not found", to))
        })?;

        // Find and remove the edge
        if let Some(edge) = self.workflow.find_edge(*from_idx, *to_idx) {
            self.workflow.remove_edge(edge);
            Ok(())
        } else {
            Err(GraphWorkflowError::AgentNotFound(format!(
                "No connection from '{}' to '{}'",
                from, to
            )))
        }
    }

    // Remove an agent from the orchestrator
    pub fn remove_agent(&mut self, name: &str) -> Result<(), GraphWorkflowError> {
        if let Some(node_idx) = self.name_to_node.remove(name) {
            self.workflow.remove_node(node_idx);
            self.agents.remove(name);
            Ok(())
        } else {
            Err(GraphWorkflowError::AgentNotFound(format!(
                "Agent '{}' not found",
                name
            )))
        }
    }

    // Execute a specific agent
    pub async fn execute_agent(
        &self,
        name: &str,
        input: String,
    ) -> Result<String, GraphWorkflowError> {
        if let Some(agent) = self.agents.get(name) {
            agent
                .run(input)
                .await
                .map_err(|e| GraphWorkflowError::AgentError(e.to_string()))
        } else {
            Err(GraphWorkflowError::AgentNotFound(format!(
                "Agent '{}' not found",
                name
            )))
        }
    }

    // Execute the entire workflow starting from a specific agent
    pub async fn execute_workflow(
        &mut self,
        start_agent: &str,
        input: impl Into<String>,
    ) -> Result<DashMap<String, Result<String, GraphWorkflowError>>, GraphWorkflowError> {
        let input = input.into();

        let start_idx = self.name_to_node.get(start_agent).ok_or_else(|| {
            GraphWorkflowError::AgentNotFound(format!("Start agent '{}' not found", start_agent))
        })?;

        // Reset all results
        let node_idxs = self.workflow.node_indices().collect::<Vec<_>>();
        for idx in node_idxs {
            if let Some(node_weight) = self.workflow.node_weight_mut(idx) {
                let mut last_result = node_weight.last_result.lock().await;
                *last_result = None;
            }
        }

        // Create a shared results map for all agents to write to
        let results = Arc::new(DashMap::new());
        // Create a shared tracking state for the entire workflow
        let edge_tracker = Arc::new(DashMap::new());
        let processed_nodes = Arc::new(DashMap::new());
        // Execute the workflow
        self.execute_node(
            *start_idx,
            input,
            Arc::clone(&results),
            edge_tracker,
            processed_nodes,
        )
        .await?;
        Ok(Arc::into_inner(results).expect("Results should not be poisoned"))
    }

    async fn execute_node(
        &self,
        node_idx: NodeIndex,
        input: String,
        results: Arc<DashMap<String, Result<String, GraphWorkflowError>>>,
        edge_tracker: Arc<DashMap<(NodeIndex, NodeIndex), bool>>,
        processed_nodes: Arc<DashMap<NodeIndex, Vec<(NodeIndex, String)>>>,
    ) -> Result<String, GraphWorkflowError> {
        // Get the agent name from the node
        let agent_name = &self
            .workflow
            .node_weight(node_idx)
            .ok_or_else(|| {
                GraphWorkflowError::AgentNotFound("Node not found in graph".to_string())
            })?
            .name;

        // Execute the agent
        let result = self.execute_agent(agent_name, input).await;

        // Store the result
        results.entry(agent_name.clone()).or_insert(result.clone());

        // Update the node's last result
        if let Some(node_weight) = self.workflow.node_weight(node_idx) {
            let mut last_result = node_weight.last_result.lock().await;
            *last_result = Some(result.clone());
        }

        // If successful, propagate to connected agents
        match &result {
            Ok(output) => {
                // Find all outgoing edges that pass the condition (if any)
                let valid_edges = self
                    .workflow
                    .edges_directed(node_idx, Direction::Outgoing)
                    .filter(|edge| {
                        edge.weight()
                            .condition
                            .as_ref()
                            .map(|cond| cond(output))
                            .unwrap_or(true) // if no condition, always execute
                    })
                    .collect::<Vec<_>>();

                let mut futures = Vec::new();

                for edge in valid_edges {
                    let source_node = node_idx;
                    let target_node = edge.target();
                    let flow = edge.weight().clone();
                    let results_clone = Arc::clone(&results);
                    let processed_nodes_clone = Arc::clone(&processed_nodes);
                    let edge_tracker_clone = Arc::clone(&edge_tracker);

                    let future = async move {
                        // Apply transformation if any
                        let next_input = flow
                            .transform
                            .as_ref()
                            .map_or_else(|| output.clone(), |transform| transform(output.clone()));

                        // mark this edge as processed
                        edge_tracker_clone.insert((source_node, target_node), true);
                        // record the input for this node
                        processed_nodes_clone
                            .entry(target_node)
                            .or_default()
                            .push((source_node, next_input));

                        // check if all incoming edges have been processed
                        // if yes, then we can execute the target node
                        let incoming_edges_count = self
                            .workflow
                            .edges_directed(target_node, Direction::Incoming)
                            .count();

                        let processed_edges_count = self
                            .workflow
                            .edges_directed(target_node, Direction::Incoming)
                            .filter(|e| edge_tracker_clone.contains_key(&(e.source(), target_node)))
                            .count();

                        // only execute if all incoming edges have been processed
                        if processed_edges_count == incoming_edges_count {
                            let mut aggregated_input = String::new();
                            if let Some(inputs) = processed_nodes_clone.get(&target_node) {
                                for (source_idx, input) in inputs.value() {
                                    let source_name =
                                        &self.workflow.node_weight(*source_idx).unwrap().name;
                                    aggregated_input
                                        .push_str(&format!("[From {}] {}\n", source_name, input));
                                }
                            }

                            // execute the target node with the aggregated input
                            if let Err(e) = self
                                .execute_node(
                                    target_node,
                                    aggregated_input,
                                    results_clone,
                                    edge_tracker_clone,
                                    processed_nodes_clone,
                                )
                                .await
                            {
                                tracing::error!("Failed to execute node: {:?}", e);
                            }
                        }
                    };

                    futures.push(future);
                }

                // Execute connected agents concurrently
                futures::future::join_all(futures).await;
            }
            Err(e) => {
                tracing::error!("Agent '{}' execution failed: {:?}", agent_name, e);
            }
        }

        result
    }

    // Get the current workflow as a visualization-friendly format
    pub fn get_workflow_structure(&self) -> HashMap<String, Vec<(String, Option<String>)>> {
        let mut structure = HashMap::new();

        for node_idx in self.workflow.node_indices() {
            if let Some(node) = self.workflow.node_weight(node_idx) {
                let mut connections = Vec::new();

                for edge in self.workflow.edges_directed(node_idx, Direction::Outgoing) {
                    if let Some(target) = self.workflow.node_weight(edge.target()) {
                        // TODO: can add more edge metadata here if needed
                        let edge_label = if edge.weight().transform.is_some() {
                            Some("transform".to_string())
                        } else {
                            None
                        };

                        connections.push((target.name.clone(), edge_label));
                    }
                }

                structure.insert(node.name.clone(), connections);
            }
        }

        structure
    }

    // Export the workflow to a format that can be visualized (e.g., DOT format for Graphviz)
    pub fn export_workflow_dot(&self) -> String {
        // TODO: can use petgraph's built-in dot
        // let dot = Dot::with_config(&self.workflow, &[dot::Config::EdgeNoLabel]);

        let mut dot = String::from("digraph {\n");

        // Add nodes
        for node_idx in self.workflow.node_indices() {
            if let Some(node) = self.workflow.node_weight(node_idx) {
                dot.push_str(&format!(
                    "    \"{}\" [label=\"{}\"];\n",
                    node.name, node.name
                ));
            }
        }

        // Add edges
        for edge in self.workflow.edge_indices() {
            if let Some((source, target)) = self.workflow.edge_endpoints(edge) {
                if let (Some(source_node), Some(target_node)) = (
                    self.workflow.node_weight(source),
                    self.workflow.node_weight(target),
                ) {
                    dot.push_str(&format!(
                        "    \"{}\" -> \"{}\";\n",
                        source_node.name, target_node.name
                    ));
                }
            }
        }

        dot.push_str("}\n");
        dot
    }

    // Helper method to find all possible execution paths
    pub fn find_execution_paths(
        &self,
        start_agent: &str,
    ) -> Result<Vec<Vec<String>>, GraphWorkflowError> {
        let start_idx = self.name_to_node.get(start_agent).ok_or_else(|| {
            GraphWorkflowError::AgentNotFound(format!("Start agent '{}' not found", start_agent))
        })?;

        let mut paths = Vec::new();
        let mut current_path = Vec::new();

        self.dfs_paths(*start_idx, &mut current_path, &mut paths);

        Ok(paths)
    }

    fn dfs_paths(
        &self,
        node_idx: NodeIndex,
        current_path: &mut Vec<String>,
        all_paths: &mut Vec<Vec<String>>,
    ) {
        if let Some(node) = self.workflow.node_weight(node_idx) {
            // Add current node to path
            current_path.push(node.name.clone());

            // Check if this is a leaf node (no outgoing edges)
            let has_outgoing = self
                .workflow
                .neighbors_directed(node_idx, Direction::Outgoing)
                .count()
                > 0;

            if !has_outgoing {
                // We've reached a leaf node, save this path
                all_paths.push(current_path.clone());
            } else {
                // Continue DFS for all neighbors
                for neighbor in self
                    .workflow
                    .neighbors_directed(node_idx, Direction::Outgoing)
                {
                    self.dfs_paths(neighbor, current_path, all_paths);
                }
            }

            // Backtrack
            current_path.pop();
        }
    }
}

// Edge weight to represent the flow of data between agents
#[allow(clippy::type_complexity)]
#[derive(Clone, Default)]
pub struct Flow {
    // Optional transformation function to apply to the output before passing to the next agent
    pub transform: Option<Arc<dyn Fn(String) -> String + Send + Sync>>,
    // Optional condition to determine if this flow should be taken
    pub condition: Option<Arc<dyn Fn(&str) -> bool + Send + Sync>>,
}

// Node weight for the graph
#[derive(Debug)]
pub struct AgentNode {
    pub name: String,
    // Cache for execution results
    pub last_result: Mutex<Option<Result<String, GraphWorkflowError>>>,
}

#[derive(Clone, Debug, Error)]
pub enum GraphWorkflowError {
    #[error("Agent Error: {0}")]
    AgentError(String),
    #[error("Agent not found: {0}")]
    AgentNotFound(String),
    #[error("Cycle detected in workflow")]
    CycleDetected,
}
