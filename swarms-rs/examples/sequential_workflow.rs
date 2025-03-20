use std::env;

use anyhow::Result;
use swarms_rs::llm::provider::openai::OpenAI;
use swarms_rs::sequential_workflow::SequentialWorkflow;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();

    let subscriber = tracing_subscriber::fmt::Subscriber::builder()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_line_number(true)
        .with_file(true)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    let base_url = env::var("DEEPSEEK_BASE_URL").unwrap();
    let api_key = env::var("DEEPSEEK_API_KEY").unwrap();
    let client = OpenAI::from_url(base_url, api_key).set_model("deepseek-chat");

    let agent_1 = client
        .agent_builder()
        .agent_name("Agent 1")
        .system_prompt("You are Agent 1, responsible for planning.")
        .user_name("M4n5ter")
        .max_loops(1)
        .temperature(0.3)
        .enable_autosave()
        .save_sate_dir("./temp")
        .build();

    let agent_2 = client
        .agent_builder()
        .agent_name("Agent 2")
        .system_prompt("You are Agent 2, responsible for solving the problem.")
        .user_name("M4n5ter")
        .max_loops(1)
        .temperature(0.3)
        .enable_autosave()
        .save_sate_dir("./temp")
        .build();

    let agents = vec![agent_1, agent_2]
        .into_iter()
        .map(|a| Box::new(a) as _)
        .collect::<Vec<_>>();

    let workflow = SequentialWorkflow::builder()
        .name("SequentialWorkflow")
        .metadata_output_dir("./temp/sequential_workflow/metadata")
        .description("A Workflow to solve a problem sequentially")
        .agents(agents)
        .build();

    let result = workflow.run("How to learn Rust?").await?;

    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
