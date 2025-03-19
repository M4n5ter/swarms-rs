use std::env;
use std::sync::Arc;

use anyhow::Result;
use swarms_rs::agent::Agent;
use swarms_rs::graph_workflow::{AgentRearrange, Flow};
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

    let data_collection_agent = client
        .agent_builder()
        .agent_name("Data Collection Agent")
        .system_prompt(r#"
            You are a Data Collection Agent. Your primary function is to gather requested information from various sources.

            When given a query or topic, you will:
            1. Identify the key information requirements
            2. Collect relevant data points based on your knowledge
            3. Organize the collected information in a structured format
            4. List any relevant sources or additional context

            Your responses should be factual, comprehensive, and relevant to the query.
            Format your output with clear sections and bullet points when appropriate.
            Always end with "DATA_COLLECTION_COMPLETE" to signal that your data gathering is finished.
        "#)
        .user_name("M4n5ter")
        .max_loops(1) // default is 1
        .temperature(0.1)
        .enable_autosave()
        .save_sate_path("./temp")
        .build();

    let data_processing_agent = client
        .agent_builder()
        .agent_name("Data Processing Agent")
        .user_name("M4n5ter")
        .system_prompt(r#"
            You are a Data Processing Agent. Your role is to transform raw data into more useful structured information.

            When given input data, you will:
            1. Identify and parse the key components in the input
            2. Clean the data (remove duplicates, fix formatting issues, etc.)
            3. Categorize and tag information by type and relevance
            4. Extract key entities, metrics, and relationships
            5. Transform the data into a consistent JSON format

            Your output should always follow this structure:
            {
            "processed_data": {
                "entities": [...],
                "categories": {...},
                "metrics": {...},
                "relationships": [...]
            },
            "metadata": {
                "processing_steps": [...],
                "confidence_score": 0.0-1.0
            }
            }

            Always maintain factual accuracy while improving the structure and usability of the data.
        "#)
        .enable_autosave()
        .temperature(0.1)
        .save_sate_path("./temp")
        .build();

    let content_summarization_agent = client
        .agent_builder()
        .agent_name("Content Summarization Agent")
        .user_name("M4n5ter")
        .system_prompt(r#"
            You are a Summarization Agent. Your purpose is to condense information while preserving key insights.

            When given content to summarize, you will:
            1. Identify the main topic and core message
            2. Extract the most important points and supporting details
            3. Eliminate redundancies and non-essential information
            4. Create a concise summary in proportion to the input length
            5. Preserve the original tone and factual accuracy

            Your summary should include:
            - A one-sentence TL;DR
            - 3-5 key points in bullet form
            - A short paragraph that synthesizes the information

            For longer or complex inputs, organize your summary with appropriate headings.
            Always maintain objectivity and avoid introducing new information not present in the original content.
        "#)
        .enable_autosave()
        .temperature(1.0)
        .save_sate_path("./temp")
        .build();

    let mut workflow = AgentRearrange::new("Graph Swarm", "A graph swarm workflow");

    // register agents
    vec![
        data_collection_agent.clone(),
        data_processing_agent.clone(),
        content_summarization_agent.clone(),
    ]
    .into_iter()
    .map(|a| Box::new(a) as _)
    .collect::<Vec<_>>()
    .into_iter()
    .for_each(|a| workflow.register_agent(a));

    // connect agents
    let _edge_idx1 = workflow
        .connect_agents(
            &data_collection_agent.name(),
            &data_processing_agent.name(),
            Flow::default(),
        )
        .unwrap();

    // Add a conditional flow with transformation
    let conditional_flow = Flow {
        // Add a custom transformation function, this will change the output of the previous agent
        // to a new format that will be used as the input of the next agent.
        transform: Some(Arc::new(|output| format!("Summary request: {}", output))),
        // Add a condition, this will only trigger the next agent if the output of the previous agent
        // is longer than 100 characters. If the condition is not met, the workflow will continue
        // to the next agent in the graph. This is useful to avoid expensive computations if the
        // input is too short.
        condition: Some(Arc::new(|output| output.len() > 100)),
    };
    let _edge_idx2 = workflow
        .connect_agents(
            &data_processing_agent.name(),
            &content_summarization_agent.name(),
            conditional_flow,
        )
        .unwrap();

    // Execute the workflow
    let results = workflow
        .execute_workflow(
            &data_collection_agent.name(),
            "How to build a graph database?",
        )
        .await
        .unwrap();

    println!("{results:#?}");
    Ok(())
}
