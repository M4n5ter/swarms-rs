use criterion::{Criterion, black_box, criterion_group, criterion_main};
use futures::future::{self, BoxFuture};
use std::sync::Arc;
use tokio::runtime::Runtime;

use swarms_rs::{
    agent::{Agent, AgentError},
    graph_workflow::{DAGWorkflow, Flow},
};

fn run_async<F: std::future::Future<Output = T>, T>(future: F) -> T {
    let rt = Runtime::new().unwrap();
    rt.block_on(future)
}

fn create_mock_agent(id: &str, name: &str, response: &str) -> Box<dyn Agent> {
    let agent = MockAgent::new(id, name, response);
    Box::new(agent)
}

struct MockAgent {
    id: String,
    name: String,
    response: String,
}

impl MockAgent {
    fn new(id: &str, name: &str, response: &str) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            response: response.to_string(),
        }
    }
}

impl Agent for MockAgent {
    fn run(&self, _task: String) -> BoxFuture<'static, Result<String, AgentError>> {
        let response = self.response.clone();
        Box::pin(future::ready(Ok(response)))
    }

    fn run_multiple_tasks(
        &mut self,
        tasks: Vec<String>,
    ) -> BoxFuture<'static, Result<Vec<String>, AgentError>> {
        let response = self.response.clone();
        let responses = vec![response; tasks.len()];
        Box::pin(future::ready(Ok(responses)))
    }

    fn plan(&self, _task: String) -> BoxFuture<'static, Result<(), AgentError>> {
        Box::pin(future::ready(Ok(())))
    }

    fn query_long_term_memory(&self, _task: String) -> BoxFuture<'static, Result<(), AgentError>> {
        Box::pin(future::ready(Ok(())))
    }

    fn save_task_state(&self, _task: String) -> BoxFuture<'static, Result<(), AgentError>> {
        Box::pin(future::ready(Ok(())))
    }

    fn is_response_complete(&self, _response: String) -> bool {
        true
    }

    fn id(&self) -> String {
        self.id.clone()
    }

    fn name(&self) -> String {
        self.name.clone()
    }

    fn description(&self) -> String {
        format!("Mock agent: {}", self.name)
    }

    fn clone_box(&self) -> Box<dyn Agent> {
        Box::new(MockAgent {
            id: self.id.clone(),
            name: self.name.clone(),
            response: self.response.clone(),
        })
    }
}

fn bench_linear_workflow(c: &mut Criterion) {
    c.bench_function("linear_workflow_3_agents", |b| {
        b.iter(|| {
            run_async(async {
                let mut workflow = DAGWorkflow::new("bench", "Benchmark workflow");

                workflow.register_agent(create_mock_agent("1", "agent1", "response1"));
                workflow.register_agent(create_mock_agent("2", "agent2", "response2"));
                workflow.register_agent(create_mock_agent("3", "agent3", "response3"));

                workflow
                    .connect_agents("agent1", "agent2", Flow::default())
                    .unwrap();
                workflow
                    .connect_agents("agent2", "agent3", Flow::default())
                    .unwrap();

                let _ = workflow
                    .execute_workflow("agent1", black_box("input data"))
                    .await
                    .unwrap();
            })
        });
    });
}

fn bench_branching_workflow(c: &mut Criterion) {
    c.bench_function("branching_workflow_5_agents", |b| {
        b.iter(|| {
            run_async(async {
                let mut workflow = DAGWorkflow::new("bench", "Benchmark workflow");

                workflow.register_agent(create_mock_agent("1", "agent1", "response1"));
                workflow.register_agent(create_mock_agent("2", "agent2", "response2"));
                workflow.register_agent(create_mock_agent("3", "agent3", "response3"));
                workflow.register_agent(create_mock_agent("4", "agent4", "response4"));
                workflow.register_agent(create_mock_agent("5", "agent5", "response5"));

                workflow
                    .connect_agents("agent1", "agent2", Flow::default())
                    .unwrap();
                workflow
                    .connect_agents("agent1", "agent3", Flow::default())
                    .unwrap();
                workflow
                    .connect_agents("agent2", "agent4", Flow::default())
                    .unwrap();
                workflow
                    .connect_agents("agent3", "agent5", Flow::default())
                    .unwrap();

                let _ = workflow
                    .execute_workflow("agent1", black_box("input data"))
                    .await
                    .unwrap();
            })
        });
    });
}

fn bench_workflow_with_transform(c: &mut Criterion) {
    c.bench_function("workflow_with_transform", |b| {
        b.iter(|| {
            run_async(async {
                let mut workflow = DAGWorkflow::new("bench", "Benchmark workflow");

                workflow.register_agent(create_mock_agent("1", "agent1", "response1"));
                workflow.register_agent(create_mock_agent("2", "agent2", "response2"));
                workflow.register_agent(create_mock_agent("3", "agent3", "response3"));

                let transform_fn = Arc::new(|input: String| {
                    let mut result = String::with_capacity(input.len() + 20);
                    result.push_str("Transformed: ");
                    result.push_str(&input);
                    result
                });

                workflow
                    .connect_agents(
                        "agent1",
                        "agent2",
                        Flow {
                            transform: Some(transform_fn.clone()),
                            condition: None,
                        },
                    )
                    .unwrap();

                workflow
                    .connect_agents(
                        "agent2",
                        "agent3",
                        Flow {
                            transform: Some(transform_fn),
                            condition: None,
                        },
                    )
                    .unwrap();

                let _ = workflow
                    .execute_workflow("agent1", black_box("input data"))
                    .await
                    .unwrap();
            })
        });
    });
}

fn bench_workflow_with_condition(c: &mut Criterion) {
    c.bench_function("workflow_with_condition", |b| {
        b.iter(|| {
            run_async(async {
                let mut workflow = DAGWorkflow::new("bench", "Benchmark workflow");

                workflow.register_agent(create_mock_agent("1", "agent1", "response1"));
                workflow.register_agent(create_mock_agent("2", "agent2", "response2"));
                workflow.register_agent(create_mock_agent("3", "agent3", "response3"));

                let true_condition = Arc::new(|_: &str| true);
                let false_condition = Arc::new(|_: &str| false);

                workflow
                    .connect_agents(
                        "agent1",
                        "agent2",
                        Flow {
                            transform: None,
                            condition: Some(true_condition),
                        },
                    )
                    .unwrap();

                workflow
                    .connect_agents(
                        "agent1",
                        "agent3",
                        Flow {
                            transform: None,
                            condition: Some(false_condition),
                        },
                    )
                    .unwrap();

                let _ = workflow
                    .execute_workflow("agent1", black_box("input data"))
                    .await
                    .unwrap();
            })
        });
    });
}

fn bench_large_workflow(c: &mut Criterion) {
    c.bench_function("large_workflow_10_agents", |b| {
        b.iter(|| {
            run_async(async {
                let mut workflow = DAGWorkflow::new("bench", "Benchmark workflow");

                for i in 1..=10 {
                    workflow.register_agent(create_mock_agent(
                        &i.to_string(),
                        &format!("agent{}", i),
                        &format!("response{}", i),
                    ));
                }

                for i in 1..10 {
                    for j in (i + 1)..=10 {
                        if j % (i + 1) == 0 {
                            workflow
                                .connect_agents(
                                    &format!("agent{}", i),
                                    &format!("agent{}", j),
                                    Flow::default(),
                                )
                                .unwrap();
                        }
                    }
                }

                let _ = workflow
                    .execute_workflow("agent1", black_box("input data"))
                    .await
                    .unwrap();
            })
        });
    });
}

criterion_group!(
    benches,
    bench_linear_workflow,
    bench_branching_workflow,
    bench_workflow_with_transform,
    bench_workflow_with_condition,
    bench_large_workflow,
);
criterion_main!(benches);
