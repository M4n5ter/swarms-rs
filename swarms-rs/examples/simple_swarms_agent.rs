use std::env;

use anyhow::Result;
use swarms_rs::{agent::Agent, llm::provider::openai::OpenAI};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .with(
            tracing_subscriber::fmt::layer()
                .with_line_number(true)
                .with_file(true),
        )
        .init();

    let base_url = env::var("DEEPSEEK_BASE_URL").unwrap();
    let api_key = env::var("DEEPSEEK_API_KEY").unwrap();
    let client = OpenAI::from_url(base_url, api_key).set_model("deepseek-chat");
    let agent = client
        .agent_builder()
        .system_prompt("You are a helpful assistant.")
        .agent_name("Agent 1")
        .user_name("M4n5ter")
        .enable_autosave()
        .max_loops(1)
        .save_sate_path("./temp/agent1_state.json") // or "./temp", we will ignore the base file.
        .enable_plan("Split the task into subtasks.".to_owned())
        .build();
    let response = agent.run("生命的意义是什么？".to_owned()).await.unwrap();
    println!("{response}");

    Ok(())
}
