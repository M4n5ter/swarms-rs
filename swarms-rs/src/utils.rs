use chrono::Local;
use uuid::Uuid;

use crate::{
    agent::{Agent, AgentError},
    swarm::AgentOutputSchema,
};

pub async fn run_agent_with_output_schema(
    agent: &dyn Agent,
    task: String,
) -> Result<AgentOutputSchema, AgentError> {
    let start = Local::now();
    let output = agent.run(task.clone()).await?;

    let end = Local::now();
    let duration = end.signed_duration_since(start).num_seconds();

    let agent_output = AgentOutputSchema {
        run_id: Uuid::new_v4(),
        agent_name: agent.name(),
        task,
        output,
        start,
        end,
        duration,
    };

    Ok(agent_output)
}
