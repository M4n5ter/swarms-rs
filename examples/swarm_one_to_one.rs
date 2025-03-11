use anyhow::Result;
use swarms_rs::rig::providers::deepseek;
use swarms_rs::{
    agent::{
        AgentConfig,
        rig_agent::{NoMemory, RigAgent},
    },
    swarming_architectures::one_to_one,
};

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

    let mut agent_config_1 = AgentConfig::default()
        .with_agent_name("Agent 1".to_owned())
        .with_user_name("M4n5ter".to_owned())
        .with_max_loops(1)
        .enable_autosave()
        .with_save_sate_path("./temp/agent1_state.json".to_owned());
    agent_config_1.add_stop_word("<DONE>".to_owned());

    let agent_config_2 = agent_config_1
        .clone()
        .with_agent_name("Agent 2".to_owned())
        .with_user_name("M4n5ter".to_owned())
        .with_save_sate_path("./temp/agent2_state.json".to_owned());

    let agent_1 = RigAgent::<_, NoMemory>::new(
        deepseek_chat.clone(),
        agent_config_1,
        "You are Agent 1, responsible for planning, and execution is handed over to Agent 2."
            .to_owned(),
        None,
    );

    let agent_2 = RigAgent::<_, NoMemory>::new(
        deepseek_chat.clone(),
        agent_config_2,
        "You are Agent 1, responsible for planning, and execution is handed over to Agent 2."
            .to_owned(),
        None,
    );

    let result = one_to_one(
        agent_1,
        agent_2,
        "We need a Python code to implement a quick sort algorithm.".to_owned(),
        1,
    )
    .await?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
