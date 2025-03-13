use anyhow::Result;
use swarms_rs::agent::rig_agent::RigAgentBuilder;
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

    let agent_1 = RigAgentBuilder::new_with_model(deepseek_chat.clone())
        .agent_name("Agent 1")
        .system_prompt("You are Agent 1, responsible for planning.")
        .user_name("M4n5ter")
        .max_loops(1)
        .temperature(0.3)
        .enable_autosave()
        .save_sate_path("./temp/agent1_state.json")
        .add_stop_word("<DONE>")
        .build();

    let agent_2 = RigAgentBuilder::new_with_model(deepseek_chat)
        .agent_name("Agent 2")
        .system_prompt("You are Agent 2, responsible for execution.")
        .user_name("M4n5ter")
        .max_loops(1)
        .temperature(0.3)
        .enable_autosave()
        .save_sate_path("./temp/agent2_state.json")
        .add_stop_word("<DONE>")
        .build();

    let workflow = ConcurrentWorkflow::builder()
        .name("ConcurrentWorkflow")
        .metadata_output_dir("./temp/concurrent_workflow/metadata")
        .description("A Workflow to solve a problem with two agents.")
        .add_agent(Box::new(agent_1))
        .agents(vec![Box::new(agent_2)])
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
