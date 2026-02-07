use agent_loop::adapters::openai::OpenAiResponsesClient;
use agent_loop::contracts::{ReviewStatus, SpecOutput, Task, TaskStatus};
use agent_loop::loop_runner::LlmClient;

fn sample_task() -> Task {
    Task {
        id: "t-1".to_string(),
        title: "sample task".to_string(),
        description: "description".to_string(),
        status: TaskStatus::Open,
        labels: vec![],
        parent_id: None,
    }
}

#[test]
fn fallback_review_is_never_done_when_api_key_missing() {
    let client = OpenAiResponsesClient::new("gpt-5.3-codex".to_string(), None, None);
    let task = sample_task();
    let spec = SpecOutput::default();

    let review = client.review(&task, &spec, 1).unwrap();

    assert_eq!(review.status, ReviewStatus::Rework);
    assert_eq!(review.coverage_score, 0.0);
    assert!(!review.missing_items.is_empty());
    assert!(review.risk_flags.iter().any(|x| x == "fallback-review"));
}

#[test]
fn fallback_storm_and_spec_work_without_api_key() {
    let client = OpenAiResponsesClient::new("gpt-5.3-codex".to_string(), None, None);
    let task = sample_task();

    let storm = client.product_storm(&task).unwrap();
    let spec = client.spec_first(&task, &storm).unwrap();

    assert!(!spec.acceptance_criteria.is_empty());
}
