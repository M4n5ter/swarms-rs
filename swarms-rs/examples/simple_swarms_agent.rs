use std::env;

use anyhow::Result;
use swarms_rs::{agent::Agent, llm::provider::openai::OpenAI};

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
    let mut client = OpenAI::from_url(base_url, api_key);
    client.set_model("deepseek-chat");
    client.set_system_prompt("you are a helpful assistant");
    let agent = client.agent();
    let response = agent.run("How to learn Rust".to_owned()).await.unwrap();
    println!("{response}");

    Ok(())
}
