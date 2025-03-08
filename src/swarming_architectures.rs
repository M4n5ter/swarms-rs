use anyhow::Result;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::agent_trait::Agent;
use crate::base::Structure;
use crate::swarm::{BaseSwarm, SwarmConfig};
use crate::swarm_trait::Swarm;

/// TreeSwarm configuration
#[derive(Debug)]
pub struct TreeSwarmConfig {
    pub swarm_config: SwarmConfig,
    pub max_depth: usize,
    pub branching_factor: usize,
}

impl Default for TreeSwarmConfig {
    fn default() -> Self {
        Self {
            swarm_config: SwarmConfig::default(),
            max_depth: 3,
            branching_factor: 2,
        }
    }
}

/// Tree node structure
#[derive(Debug)]
pub struct TreeNode {
    pub id: String,
    pub agent: Box<dyn Agent>,
    pub children: Vec<String>,  // IDs of child nodes
    pub parent: Option<String>, // ID of parent node
    pub depth: usize,
}

/// TreeSwarm implementation
pub struct TreeSwarm {
    base_swarm: BaseSwarm,
    config: TreeSwarmConfig,
    nodes: Arc<Mutex<HashMap<String, TreeNode>>>,
    root_id: Option<String>,
}

impl TreeSwarm {
    pub fn new(config: TreeSwarmConfig, root_agent: Box<dyn Agent>) -> Self {
        let swarm = BaseSwarm::new(config.swarm_config.clone(), vec![root_agent.clone()]);

        // We'll initialize the tree in a separate async function
        Self {
            base_swarm: swarm,
            config,
            nodes: Arc::new(Mutex::new(HashMap::new())),
            root_id: None,
        }
    }

    /// Initialize the tree with the root agent
    pub async fn initialize(&mut self) -> Result<()> {
        // Initialize the base swarm
        self.base_swarm.initialize().await?;

        // Get the root agent
        let agents = self.base_swarm.agents.lock().await;
        if let Some(root_agent) = agents.first() {
            let root_id = root_agent.id().to_owned();

            // Create the root node
            let root_node = TreeNode {
                id: root_id.clone(),
                agent: root_agent.clone(),
                children: Vec::new(),
                parent: None,
                depth: 0,
            };

            // Add the root node to the nodes map
            let mut nodes = self.nodes.lock().await;
            nodes.insert(root_id.clone(), root_node);

            // Set the root ID
            self.root_id = Some(root_id);
        }

        Ok(())
    }

    /// Add a child agent to a parent node
    pub async fn add_child(&mut self, parent_id: &str, child_agent: Box<dyn Agent>) -> Result<()> {
        let mut nodes = self.nodes.lock().await;

        // Check if the parent exists
        if let Some(parent_node) = nodes.get_mut(parent_id) {
            // Check if we've reached the maximum depth
            if parent_node.depth >= self.config.max_depth {
                return Err(anyhow::anyhow!("Maximum tree depth reached"));
            }

            // Check if we've reached the maximum branching factor
            if parent_node.children.len() >= self.config.branching_factor {
                return Err(anyhow::anyhow!("Maximum branching factor reached"));
            }

            let child_id = child_agent.id().to_owned();

            // Create the child node
            let child_node = TreeNode {
                id: child_id.clone(),
                agent: child_agent.clone(),
                children: Vec::new(),
                parent: Some(parent_id.to_string()),
                depth: parent_node.depth + 1,
            };

            // Add the child to the parent's children
            parent_node.children.push(child_id.clone());

            // Add the child node to the nodes map
            nodes.insert(child_id, child_node);

            // Add the agent to the base swarm
            drop(nodes); // Release the lock before calling add_agent
            self.base_swarm.add_agent(child_agent).await?;

            Ok(())
        } else {
            Err(anyhow::anyhow!("Parent node not found"))
        }
    }

    /// Traverse the tree in breadth-first order
    pub async fn bfs_traverse<F>(&self, mut callback: F) -> Result<()>
    where
        F: AsyncFnMut(&TreeNode) -> Result<()>,
    {
        let nodes = self.nodes.lock().await;

        if let Some(root_id) = &self.root_id {
            let mut queue = VecDeque::new();
            queue.push_back(root_id.clone());

            while let Some(node_id) = queue.pop_front() {
                if let Some(node) = nodes.get(&node_id) {
                    // Call the callback on this node
                    callback(node).await?;

                    // Add children to the queue
                    for child_id in &node.children {
                        queue.push_back(child_id.clone());
                    }
                }
            }
        }

        Ok(())
    }

    /// Traverse the tree in depth-first order
    pub async fn dfs_traverse<F>(&self, mut callback: F) -> Result<()>
    where
        F: FnMut(&TreeNode) -> Result<()>,
    {
        let nodes = self.nodes.lock().await;

        if let Some(root_id) = &self.root_id {
            self.dfs_traverse_recursive(root_id, &nodes, &mut callback)?;
        }

        Ok(())
    }

    /// Helper function for recursive DFS traversal
    fn dfs_traverse_recursive<F>(
        &self,
        node_id: &str,
        nodes: &HashMap<String, TreeNode>,
        callback: &mut F,
    ) -> Result<()>
    where
        F: FnMut(&TreeNode) -> Result<()>,
    {
        if let Some(node) = nodes.get(node_id) {
            // Call the callback on this node
            callback(node)?;

            // Recursively traverse children
            for child_id in &node.children {
                self.dfs_traverse_recursive(child_id, nodes, callback)?;
            }
        }

        Ok(())
    }
}

impl Structure for TreeSwarm {
    async fn run(&self) -> Result<()> {
        // Default implementation - run the base swarm
        Swarm::run(&self.base_swarm).await
    }

    async fn save_to_file(&self, data: &[u8], path: std::path::PathBuf) -> Result<()> {
        self.base_swarm.save_to_file(data, path).await
    }

    async fn load_from_file(&self, path: std::path::PathBuf) -> Result<Vec<u8>> {
        self.base_swarm.load_from_file(path).await
    }

    async fn save_metadata(&self, metadata: HashMap<String, String>) -> Result<()> {
        self.base_swarm.save_metadata(metadata).await
    }

    async fn load_metadata(&self) -> Result<HashMap<String, String>> {
        self.base_swarm.load_metadata().await
    }

    async fn log_error(&self, error: anyhow::Error) -> Result<()> {
        self.base_swarm.log_error(error).await
    }

    async fn save_artifact(&self, artifact: Vec<u8>) -> Result<()> {
        self.base_swarm.save_artifact(artifact).await
    }

    async fn load_artifact(&self, path: std::path::PathBuf) -> Result<Vec<u8>> {
        self.base_swarm.load_artifact(path).await
    }

    async fn log_event(&self, event: String) -> Result<()> {
        self.base_swarm.log_event(event).await
    }
}

impl Swarm for TreeSwarm {
    async fn add_agent(&mut self, agent: Box<dyn Agent>) -> Result<()> {
        // In TreeSwarm, agents must be added as children of existing nodes
        // This is a fallback that adds the agent to the root if no parent is specified
        if let Some(root_id) = &self.root_id.clone() {
            self.add_child(root_id, agent).await
        } else {
            Err(anyhow::anyhow!("Tree not initialized"))
        }
    }

    async fn remove_agent(&mut self, agent_id: String) -> Result<()> {
        let mut nodes = self.nodes.lock().await;

        // Check if the agent exists in the tree
        if let Some(node) = nodes.get(&agent_id) {
            // Cannot remove the root node
            if node.parent.is_none() {
                return Err(anyhow::anyhow!("Cannot remove the root node"));
            }

            // Get the parent node
            if let Some(parent_id) = &node.parent.clone() {
                if let Some(parent_node) = nodes.get_mut(parent_id) {
                    // Remove the agent from the parent's children
                    parent_node.children.retain(|id| id != &agent_id);
                }
            }

            // Remove the node and all its descendants
            self.remove_subtree(&agent_id, &mut nodes);

            // Remove the agent from the base swarm
            drop(nodes); // Release the lock before calling remove_agent
            self.base_swarm.remove_agent(agent_id).await?;

            Ok(())
        } else {
            Err(anyhow::anyhow!("Agent not found in the tree"))
        }
    }

    async fn run(&self) -> Result<()> {
        // Run the swarm in a breadth-first manner
        self.bfs_traverse(async |node| {
            // Run the agent
            node.agent.run().await
        })
        .await
    }

    async fn broadcast(&self, message: String) -> Result<()> {
        // Use the base swarm's broadcast method
        self.base_swarm.broadcast(message).await
    }
}

impl TreeSwarm {
    /// Helper function to remove a subtree
    fn remove_subtree(&self, node_id: &str, nodes: &mut HashMap<String, TreeNode>) {
        if let Some(node) = nodes.get(node_id) {
            // Clone the children vector to avoid borrowing issues
            let children = node.children.clone();

            // Recursively remove all children
            for child_id in children {
                self.remove_subtree(&child_id, nodes);
            }

            // Remove this node
            nodes.remove(node_id);
        }
    }
}

/// MatrixSwarm configuration
#[derive(Debug)]
pub struct MatrixSwarmConfig {
    pub swarm_config: SwarmConfig,
    pub rows: usize,
    pub cols: usize,
}

impl Default for MatrixSwarmConfig {
    fn default() -> Self {
        Self {
            swarm_config: SwarmConfig::default(),
            rows: 3,
            cols: 3,
        }
    }
}

/// Matrix cell structure
#[derive(Clone, Debug)]
pub struct MatrixCell {
    pub id: String,
    pub agent: Box<dyn Agent>,
    pub row: usize,
    pub col: usize,
    pub neighbors: Vec<(usize, usize)>, // Indices of neighboring cells
}

/// MatrixSwarm implementation
pub struct MatrixSwarm {
    base_swarm: BaseSwarm,
    config: MatrixSwarmConfig,
    cells: Arc<Mutex<Vec<Vec<Option<MatrixCell>>>>>,
}

impl MatrixSwarm {
    pub fn new(config: MatrixSwarmConfig) -> Self {
        let swarm = BaseSwarm::new(config.swarm_config.clone(), vec![]);

        // Initialize the matrix with empty cells
        let mut matrix = Vec::with_capacity(config.rows);
        for _ in 0..config.rows {
            let row = vec![None; config.cols];
            matrix.push(row);
        }

        Self {
            base_swarm: swarm,
            config,
            cells: Arc::new(Mutex::new(matrix)),
        }
    }

    /// Initialize the matrix swarm
    pub async fn initialize(&mut self) -> Result<()> {
        // Initialize the base swarm
        self.base_swarm.initialize().await
    }

    /// Add an agent to a specific cell
    pub async fn add_agent_at(
        &mut self,
        row: usize,
        col: usize,
        agent: Box<dyn Agent>,
    ) -> Result<()> {
        let mut cells = self.cells.lock().await;

        // Check if the cell is within bounds
        if row >= self.config.rows || col >= self.config.cols {
            return Err(anyhow::anyhow!("Cell coordinates out of bounds"));
        }

        // Check if the cell is already occupied
        if cells[row][col].is_some() {
            return Err(anyhow::anyhow!("Cell already occupied"));
        }

        let agent_id = agent.id().to_owned();

        // Calculate neighboring cells
        let mut neighbors = Vec::new();
        for r in row.saturating_sub(1)..=usize::min(row + 1, self.config.rows - 1) {
            for c in col.saturating_sub(1)..=usize::min(col + 1, self.config.cols - 1) {
                // Skip the cell itself
                if r == row && c == col {
                    continue;
                }

                neighbors.push((r, c));
            }
        }

        // Create the cell
        let cell = MatrixCell {
            id: agent_id.clone(),
            agent: agent.clone(),
            row,
            col,
            neighbors,
        };

        // Add the cell to the matrix
        cells[row][col] = Some(cell);

        // Add the agent to the base swarm
        drop(cells); // Release the lock before calling add_agent
        self.base_swarm.add_agent(agent).await?;

        Ok(())
    }

    /// Get the agent at a specific cell
    pub async fn get_agent_at(&self, row: usize, col: usize) -> Option<Box<dyn Agent>> {
        let cells = self.cells.lock().await;

        // Check if the cell is within bounds
        if row >= self.config.rows || col >= self.config.cols {
            return None;
        }

        // Check if the cell is occupied
        if let Some(cell) = &cells[row][col] {
            return Some(cell.agent.clone());
        }

        None
    }

    /// Get the neighbors of a specific cell
    pub async fn get_neighbors(&self, row: usize, col: usize) -> Vec<(usize, usize)> {
        let cells = self.cells.lock().await;

        // Check if the cell is within bounds
        if row >= self.config.rows || col >= self.config.cols {
            return Vec::new();
        }

        // Check if the cell is occupied
        if let Some(cell) = &cells[row][col] {
            cell.neighbors.clone()
        } else {
            Vec::new()
        }
    }

    /// Send a message to a specific cell
    pub async fn send_message_to(&self, row: usize, col: usize, message: String) -> Result<()> {
        let cells = self.cells.lock().await;

        // Check if the cell is within bounds
        if row >= self.config.rows || col >= self.config.cols {
            return Err(anyhow::anyhow!("Cell coordinates out of bounds"));
        }

        // Check if the cell is occupied
        if let Some(cell) = &cells[row][col] {
            // Send the message to the agent
            cell.agent.send_message(message).await
        } else {
            Err(anyhow::anyhow!("Cell is not occupied"))
        }
    }

    /// Send a message to all neighbors of a specific cell
    pub async fn send_message_to_neighbors(
        &self,
        row: usize,
        col: usize,
        message: String,
    ) -> Result<()> {
        let cells = self.cells.lock().await;

        // Check if the cell is within bounds
        if row >= self.config.rows || col >= self.config.cols {
            return Err(anyhow::anyhow!("Cell coordinates out of bounds"));
        }

        // Check if the cell is occupied
        if let Some(cell) = &cells[row][col] {
            // Send the message to all neighbors
            for (r, c) in &cell.neighbors {
                if let Some(neighbor) = &cells[*r][*c] {
                    neighbor.agent.send_message(message.clone()).await?;
                }
            }

            Ok(())
        } else {
            Err(anyhow::anyhow!("Cell is not occupied"))
        }
    }
}

impl Structure for MatrixSwarm {
    async fn run(&self) -> Result<()> {
        // Default implementation - run the base swarm
        Swarm::run(&self.base_swarm).await
    }

    async fn save_to_file(&self, data: &[u8], path: std::path::PathBuf) -> Result<()> {
        self.base_swarm.save_to_file(data, path).await
    }

    async fn load_from_file(&self, path: std::path::PathBuf) -> Result<Vec<u8>> {
        self.base_swarm.load_from_file(path).await
    }

    async fn save_metadata(&self, metadata: HashMap<String, String>) -> Result<()> {
        self.base_swarm.save_metadata(metadata).await
    }

    async fn load_metadata(&self) -> Result<HashMap<String, String>> {
        self.base_swarm.load_metadata().await
    }

    async fn log_error(&self, error: anyhow::Error) -> Result<()> {
        self.base_swarm.log_error(error).await
    }

    async fn save_artifact(&self, artifact: Vec<u8>) -> Result<()> {
        self.base_swarm.save_artifact(artifact).await
    }

    async fn load_artifact(&self, path: std::path::PathBuf) -> Result<Vec<u8>> {
        self.base_swarm.load_artifact(path).await
    }

    async fn log_event(&self, event: String) -> Result<()> {
        self.base_swarm.log_event(event).await
    }
}

impl Swarm for MatrixSwarm {
    async fn add_agent(&mut self, agent: Box<dyn Agent>) -> Result<()> {
        // Find an empty cell to add the agent
        let cells = self.cells.lock().await;

        for row in 0..self.config.rows {
            for col in 0..self.config.cols {
                if cells[row][col].is_none() {
                    // Found an empty cell
                    drop(cells); // Release the lock before calling add_agent_at
                    return self.add_agent_at(row, col, agent).await;
                }
            }
        }

        // No empty cells found
        Err(anyhow::anyhow!("No empty cells available"))
    }

    async fn remove_agent(&mut self, agent_id: String) -> Result<()> {
        let mut cells = self.cells.lock().await;

        // Find the cell containing the agent
        for row in 0..self.config.rows {
            for col in 0..self.config.cols {
                if let Some(cell) = &cells[row][col] {
                    if cell.id == agent_id {
                        // Found the agent, remove it
                        cells[row][col] = None;

                        // Remove the agent from the base swarm
                        drop(cells); // Release the lock before calling remove_agent
                        self.base_swarm.remove_agent(agent_id).await?;

                        return Ok(());
                    }
                }
            }
        }

        // Agent not found
        Err(anyhow::anyhow!("Agent not found in the matrix"))
    }

    async fn run(&self) -> Result<()> {
        // Run all agents in the matrix
        let cells = self.cells.lock().await;

        for row in 0..self.config.rows {
            for col in 0..self.config.cols {
                if let Some(cell) = &cells[row][col] {
                    cell.agent.run().await?;
                }
            }
        }

        Ok(())
    }

    async fn broadcast(&self, message: String) -> Result<()> {
        // Use the base swarm's broadcast method
        self.base_swarm.broadcast(message).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::AgentConfig;
    use crate::agent::BaseAgent;

    #[test]
    fn test_matrix_swarm_config_default() {
        let config = MatrixSwarmConfig::default();
        assert_eq!(config.rows, 3);
        assert_eq!(config.cols, 3);
    }

    #[tokio::test]
    async fn test_matrix_swarm_add_agent_at() {
        let mut swarm = MatrixSwarm::new(MatrixSwarmConfig::default());
        let agent = Box::new(BaseAgent::new(AgentConfig::default())) as _;

        swarm.add_agent_at(1, 1, agent).await.unwrap();

        let cells = swarm.cells.lock().await;
        assert!(cells[1][1].is_some());
    }
}
