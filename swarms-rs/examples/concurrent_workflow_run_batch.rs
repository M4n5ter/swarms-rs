use std::env;

use anyhow::Result;
use swarms_rs::concurrent_workflow::ConcurrentWorkflow;
use swarms_rs::llm::provider::openai::OpenAI;

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
        .agent_name("Agent1")
        .system_prompt("You are a helpful assistant.")
        .user_name("M4n5ter")
        .max_loops(1)
        .temperature(0.3)
        .enable_autosave()
        .save_sate_path("./temp/rig_agent_state.json") // or "./temp", we will ignore the base file.
        .build();

    let agent_2 = client
        .agent_builder()
        .system_prompt("You are a helpful assistant.")
        .agent_name("Agent2")
        .user_name("M4n5ter")
        .enable_autosave()
        .max_loops(1)
        .temperature(0.7)
        .save_sate_path("./temp/swarms_agent_state.json") // or "./temp", we will ignore the base file.
        .build();

    // Concurrent Workflow
    let workflow = ConcurrentWorkflow::builder()
        .name("ConcurrentWorkflow")
        .metadata_output_dir("./temp/concurrent_workflow/metadata")
        .description("A Workflow to solve a problem with two agents.")
        .add_agent(Box::new(agent_2))
        .agents(vec![Box::new(agent_1)]) // also support Vec<Box<dyn Agent>>
        .build();

    let tasks = vec![
        "How to learn Rust?",
        "How to learn Python?",
        "How to learn Go?",
    ]
    .into_iter()
    .map(String::from)
    .collect();

    let results = workflow.run_batch(tasks).await?;

    println!("{}", serde_json::to_string_pretty(&results)?);
    Ok(())
}
