use std::env;

use anyhow::Result;
use swarms_rs::agent::rig_agent::RigAgentBuilder;
use swarms_rs::llm::provider::openai::OpenAI;
use swarms_rs::multi_agent_orchestrator::{self, MultiAgentOrchestrator};
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
        .description("Only answer questions about apple.")
        .system_prompt("You are a helpful agent.")
        .user_name("M4n5ter")
        .temperature(0.3)
        .build();

    let agent_2 = RigAgentBuilder::new_with_model(deepseek_chat.clone())
        .agent_name("Agent 2")
        .description("Only answer questions about banana.")
        .system_prompt("You are a helpful agent.")
        .user_name("M4n5ter")
        .max_loops(1)
        .temperature(0.3)
        .build();

    let agents = vec![agent_1, agent_2]
        .into_iter()
        .map(|a| Box::new(a) as _)
        .collect::<Vec<_>>();

    let base_url = env::var("DEEPSEEK_BASE_URL").unwrap();
    let api_key = env::var("DEEPSEEK_API_KEY").unwrap();
    let client = OpenAI::from_url(base_url, api_key).set_model("deepseek-chat");
    let boss = client
        .agent_builder()
        .system_prompt(multi_agent_orchestrator::create_boss_system_prompt(&agents).unwrap())
        .agent_name("MultiAgentOrchestrator")
        .user_name("M4n5ter")
        .enable_autosave()
        .max_loops(1)
        .save_sate_path("./temp")
        .build();

    let mao = MultiAgentOrchestrator::new(boss, agents, true)?;

    let result = mao
        .run("What are the benefits of eating bananas?".to_owned())
        .await?;

    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

// Output like:
//
// {
//     "id": "eab579a7-a16d-4f5c-8974-4e3ebdac6926",
//     "timestamp": 1741857412,
//     "task": {
//       "original": "What are the benefits of eating bananas?",
//       "modified": null
//     },
//     "boss_decision": {
//       "selected_agent": "Agent 2",
//       "reasoning": "The user's question is specifically about the benefits of eating bananas, which falls directly under the expertise of Agent 2, as it is specialized in answering questions about bananas."
//     },
//     "execution": {
//       "agent_id": "0860a600-55ef-470c-9ece-5c5dbc988e35",
//       "agent_name": "Agent 2",
//       "was_executed": true,
//       "response": "Eating bananas offers a variety of health benefits due to their rich nutrient content. Here are some of the key benefits:\n\n1. **Rich in Nutrients**: Bananas are a good source of essential vitamins and minerals, including vitamin C, vitamin B6, potassium, and dietary fiber.\n\n2. **Heart Health**: The high potassium content in bananas helps regulate blood pressure, which can reduce the risk of heart disease and stroke.\n\n3. **Digestive Health**: Bananas contain dietary fiber, which aids in digestion and helps prevent constipation. They also contain resistant starch, which acts as a prebiotic and promotes gut health.\n\n4. **Energy Boost**: Bananas are a great source of natural sugars (glucose, fructose, and sucrose) and carbohydrates, making them an excellent snack for a quick energy boost.\n\n5. **Weight Management**: The fiber content in bananas can help you feel full longer, which may aid in weight management by reducing overall calorie intake.\n\n6. **Antioxidants**: Bananas contain several types of antioxidants, including dopamine and catechins, which can help reduce oxidative stress and lower the risk of chronic diseases.\n\n7. **Mood and Mental Health**: Bananas contain tryptophan, an amino acid that the body converts into serotonin, a neurotransmitter that helps regulate mood and promote feelings of well-being.\n\n8. **Exercise Recovery**: The potassium and carbohydrates in bananas make them a good post-workout snack, helping to replenish electrolytes and glycogen stores.\n\n9. **Bone Health**: While bananas are not high in calcium, they contain fructooligosaccharides, which can enhance the body's ability to absorb calcium, thereby supporting bone health.\n\n10. **Skin Health**: The vitamins and antioxidants in bananas can contribute to healthier skin by reducing oxidative stress and promoting collagen production.\n\nIncorporating bananas into your diet can be a simple and effective way to enjoy these health benefits.",
//       "execution_time": 25
//     },
//     "total_time": 38
// }
