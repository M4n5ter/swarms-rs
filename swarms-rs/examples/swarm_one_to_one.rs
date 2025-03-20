use std::env;

use anyhow::Result;
use swarms_rs::{llm::provider::openai::OpenAI, swarming_architectures::one_to_one};

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
        .system_prompt(
            "You are Agent 1, responsible for planning, and execution is handed over to Agent 2.",
        )
        .agent_name("Agent 1")
        .user_name("M4n5ter")
        .enable_autosave()
        .temperature(0.3)
        .max_loops(1)
        .save_sate_dir("./temp")
        .build();
    let agent_2 = client
        .agent_builder()
        .system_prompt("You are Agent 2, responsible for execution.")
        .agent_name("Agent 2")
        .user_name("M4n5ter")
        .enable_autosave()
        .temperature(0.3)
        .max_loops(1)
        .save_sate_dir("./temp")
        .build();

    let result = one_to_one(
        agent_1,
        agent_2,
        "We need a Python code to implement a quick sort algorithm.",
        1,
    )
    .await?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
