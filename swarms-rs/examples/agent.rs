use anyhow::Result;
use swarms_rs::agent::Agent;
use swarms_rs::agent::rig_agent::RigAgentBuilder;
use swarms_rs::rig::providers::deepseek;

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
        .system_prompt("You are a helpful assistant.")
        .agent_name("Agent 1")
        .user_name("M4n5ter")
        .enable_autosave()
        .max_loops(1)
        .save_sate_path("./temp/agent1_state.json") // or "./temp", we will ignore the base file.
        .enable_plan("Split the task into subtasks.".to_owned())
        .add_stop_word("<DONE>")
        .build();

    let result = agent.run("生命的意义是什么？".into()).await.unwrap();
    println!("{result}");
    Ok(())
}
