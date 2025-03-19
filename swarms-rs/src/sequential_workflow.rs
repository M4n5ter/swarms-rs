use crate::agent::Agent;

pub struct SequentialWorkflow {
    name: String,
    description: String,
    metadata_output_dir: String,
    agents: Vec<Box<dyn Agent>>,
}
