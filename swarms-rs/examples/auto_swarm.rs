use std::env;

use anyhow::Result;
use swarms_rs::auto_swarm::AutoSwarm;
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

    let boss = client
        .agent_builder()
        .agent_name("AutoSwarm_Boss")
        .user_name("M4n5ter")
        .enable_autosave()
        .max_loops(1)
        .save_sate_path("./temp")
        .build();

    // MultiAgentOrchestrator will set the system_prompt for boss automatically.
    let auto_swarm = AutoSwarm::new(
        "An AutoSwarm",
        "automatically builds and manages swarms of AI agents",
        boss,
        client,
    );

    let result = auto_swarm.run("如何学习Rust?".to_owned()).await?;

    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
