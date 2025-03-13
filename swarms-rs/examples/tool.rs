use anyhow::Result;
use swarms_macro::tool;
use swarms_rs::agent::Agent;
use swarms_rs::agent::rig_agent::RigAgentBuilder;
use swarms_rs::rig::providers::deepseek;
use thiserror::Error;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();

    let subscriber = tracing_subscriber::fmt::Subscriber::builder()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_line_number(true)
        .with_file(true)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    // OPENAI_API_KEY=sk-xxxxxxxxxxxxxxxxxxxxx
    // let openai_client = openai::Client::from_env();
    // let o3_mini = openai_client.completion_model(openai::O3_MINI_2025_01_31);

    // ANTHROPIC_API_KEY=xxxxxxxxxxxxxxxxxxxxxx
    // let anthropic_client = anthropic::Client::from_env();
    // let claude35 = anthropic_client.completion_model("claude-3-7-sonnet-latest");
    // GEMINI_API_KEY=xxxxxxxxxxxxxxxx

    // DEEPSEEK_API_KEY=sk-xxxxxxxxxxxxxxxxxxxxx
    let deepseek_client = deepseek::Client::from_env();
    let deepseek_chat = deepseek_client.completion_model(deepseek::DEEPSEEK_CHAT);

    let agent = RigAgentBuilder::new_with_model(deepseek_chat)
        .system_prompt("You need to select the right tool to answer the question.")
        .agent_name("Agent 1")
        .user_name("M4n5ter")
        .enable_autosave()
        .save_sate_path("./temp/agent1_state.json") // or "./temp", we will ignore the base file.
        .add_stop_word("<DONE>")
        .add_tool(SubTool)
        .add_tool(Add) // or AddTool, Add is a pub static variable of AddTool
        .add_tool(MultiplyTool)
        .build();

    let mut result = agent.run("10 - 5".into()).await.unwrap();
    println!("{result}");

    result = agent.run(format!("{} + 5", result)).await.unwrap();
    println!("{result}");

    result = agent.run(format!("{} * 5", result)).await.unwrap();
    println!("{result}");
    Ok(())
}

#[tool(
    description = "Subtract y from x (i.e.: x - y)",
    arg(x, description = "The number to subtract from"),
    arg(y, description = "The number to subtract")
)]
fn sub(x: f64, y: f64) -> Result<f64, CalcError> {
    tracing::info!("Sub tool is called");
    Ok(x - y)
}

#[tool]
fn add(x: f64, y: f64) -> Result<f64, CalcError> {
    tracing::info!("Add tool is called");
    Ok(x + y)
}

#[tool(name = "Multiply", description = "Multiply x and y (i.e.: x * y)")]
fn mul(x: f64, y: f64) -> Result<f64, CalcError> {
    tracing::info!("Mul tool is called");
    Ok(x * y)
}

#[derive(Debug, Error)]
pub enum CalcError {}
