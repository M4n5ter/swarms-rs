use anyhow::Result;
use swarms_rs::agent::{
    AgentConfig,
    rig_agent::{NoMemory, RigAgent},
};
use swarms_rs::concurrent_workflow::ConcurrentWorkflow;
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

    let agent_config_1_builder = AgentConfig::builder()
        .agent_name("Agent 1")
        .user_name("M4n5ter")
        .max_loops(1)
        .enable_autosave()
        .save_sate_path("./temp/agent1_state.json")
        .add_stop_word("<DONE>");

    let agent_config_2_builder = agent_config_1_builder
        .clone()
        .agent_name("Agent 2")
        .user_name("M4n5ter")
        .save_sate_path("./temp/agent2_state.json");

    let agent_1 = RigAgent::<_, NoMemory>::new(
        deepseek_chat.clone(),
        agent_config_1_builder.build(),
        "You are Agent 1, responsible for planning.",
        None,
    );

    let agent_2 = RigAgent::<_, NoMemory>::new(
        deepseek_chat.clone(),
        agent_config_2_builder.build(),
        "You are Agent 2, responsible for planning.".to_owned(),
        None,
    );

    let workflow = ConcurrentWorkflow::new(
        "Concurrent Workflow".to_owned(),
        "./temp/concurrent_workflow/metadata".to_owned(),
        "A Workflow to solve a problem with two agents.",
        vec![Box::new(agent_1), Box::new(agent_2)],
    );

    let tasks = vec![
        "How to learn Rust?".to_owned(),
        "How to learn Python?".to_owned(),
        "How to learn Go?".to_owned(),
    ];

    let results = workflow.run_batch(tasks).await?;

    println!("{}", serde_json::to_string_pretty(&results)?);
    Ok(())
}
