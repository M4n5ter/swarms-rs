use anyhow::Result;
use swarms_rs::agent::{
    Agent, AgentConfig,
    rig_agent::{NoMemory, RigAgent},
};
use swarms_rs::rig::providers::deepseek;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();

    let subscriber = tracing_subscriber::fmt::Subscriber::builder()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
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

    let mut agent_config = AgentConfig::default()
        .with_agent_name("Agent 1".to_owned())
        .with_user_name("M4n5ter".to_owned())
        .enable_autosave()
        .with_save_sate_path("./temp/agent1_state.json".to_owned())
        .enable_plan()
        .with_planning_prompt("将用户的问题分解为多个步骤".to_owned());
    agent_config.add_stop_word("<DONE>".to_owned());

    let mut agent = RigAgent::<_, NoMemory>::new(
        deepseek_chat,
        agent_config,
        "You are a helpful assistant, when you think you complete the task, you must add <DONE> to the end of the response.".to_owned(),
        None,
    );

    let result = agent.run("生命的意义是什么？".into()).await.unwrap();
    println!("{result}");
    Ok(())
}
