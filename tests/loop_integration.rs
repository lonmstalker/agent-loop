use std::sync::{Arc, Mutex};

use anyhow::Result;

use agent_loop::config::{LoopProfile, RetainMode};
use agent_loop::contracts::{
    ProductStormOutput, ReviewOutput, ReviewStatus, RunOutcome, SpawnTaskCandidate, SpawnTaskKind,
    SpecOutput, Task, TaskStatus,
};
use agent_loop::loop_runner::{
    AgentLoop, BeadsClient, HindsightClient, LlmClient, MemoryBankClient, RunConfig,
};

#[derive(Default)]
struct BeadsState {
    ensure_calls: usize,
    bootstrap_calls: usize,
    claim_calls: usize,
    sync_calls: usize,
    add_dep_calls: usize,
    notes_calls: usize,
    created_ids: Vec<String>,
    noted_ids: Vec<String>,
    bootstrap_goals: Vec<String>,
    children: Vec<Task>,
    parent: Option<Task>,
}

struct MockBeads {
    state: Arc<Mutex<BeadsState>>,
    next_child: Arc<Mutex<u32>>,
}

impl MockBeads {
    fn new(parent: Option<Task>, children: Vec<Task>) -> Self {
        Self {
            state: Arc::new(Mutex::new(BeadsState {
                parent,
                children,
                ..Default::default()
            })),
            next_child: Arc::new(Mutex::new(1)),
        }
    }
}

impl BeadsClient for MockBeads {
    fn ensure_initialized(&self) -> Result<()> {
        self.state.lock().unwrap().ensure_calls += 1;
        Ok(())
    }

    fn create_bootstrap_parent_task(&self, goal: &str) -> Result<Task> {
        let mut state = self.state.lock().unwrap();
        state.bootstrap_calls += 1;
        state.bootstrap_goals.push(goal.to_string());
        Ok(Task {
            id: "bootstrap-1".to_string(),
            title: format!("[Bootstrap] Product storm: {goal}"),
            description: format!("Bootstrap goal: {goal}"),
            status: TaskStatus::Open,
            labels: vec!["loop:bootstrap".to_string()],
            parent_id: None,
        })
    }

    fn claim_parent_task(&self, _preferred_task: Option<&str>) -> Result<Option<Task>> {
        let mut state = self.state.lock().unwrap();
        state.claim_calls += 1;
        Ok(state.parent.take())
    }

    fn sync(&self) -> Result<()> {
        self.state.lock().unwrap().sync_calls += 1;
        Ok(())
    }

    fn search_duplicates(
        &self,
        parent_id: &str,
        candidate: &SpawnTaskCandidate,
    ) -> Result<Vec<Task>> {
        let state = self.state.lock().unwrap();
        let target = candidate.normalized_title();
        Ok(state
            .children
            .iter()
            .filter(|task| {
                task.parent_id.as_deref() == Some(parent_id)
                    && task.is_open()
                    && task.title.to_lowercase().trim() == target
            })
            .cloned()
            .collect())
    }

    fn create_child_task(&self, parent_id: &str, candidate: &SpawnTaskCandidate) -> Result<Task> {
        let mut id = self.next_child.lock().unwrap();
        let child_id = format!("child-{}", *id);
        *id += 1;

        let task = Task {
            id: child_id.clone(),
            title: candidate.title.clone(),
            description: candidate.description.clone(),
            status: TaskStatus::Open,
            labels: vec![],
            parent_id: Some(parent_id.to_string()),
        };

        let mut state = self.state.lock().unwrap();
        state.created_ids.push(child_id);
        state.children.push(task.clone());
        Ok(task)
    }

    fn add_blocking_dependency(&self, _child_id: &str, _parent_id: &str) -> Result<()> {
        self.state.lock().unwrap().add_dep_calls += 1;
        Ok(())
    }

    fn add_notes(&self, task_id: &str, _notes: &str) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        state.notes_calls += 1;
        state.noted_ids.push(task_id.to_string());
        Ok(())
    }
}

#[derive(Default)]
struct HindsightState {
    ensure_calls: usize,
    bank_calls: usize,
    recall_pref_calls: usize,
    recall_task_calls: usize,
    retain_calls: usize,
}

struct MockHindsight {
    state: Arc<Mutex<HindsightState>>,
}

impl MockHindsight {
    fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(HindsightState::default())),
        }
    }
}

impl HindsightClient for MockHindsight {
    fn ensure_available(&self) -> Result<()> {
        self.state.lock().unwrap().ensure_calls += 1;
        Ok(())
    }

    fn ensure_project_bank(&self, _bank: &str) -> Result<()> {
        self.state.lock().unwrap().bank_calls += 1;
        Ok(())
    }

    fn recall_preferences(&self) -> Result<()> {
        self.state.lock().unwrap().recall_pref_calls += 1;
        Ok(())
    }

    fn recall_task_context(&self, _bank: &str, _query: &str) -> Result<()> {
        self.state.lock().unwrap().recall_task_calls += 1;
        Ok(())
    }

    fn retain_summary(
        &self,
        _bank: &str,
        _doc_id: &str,
        _context: &str,
        _content: &str,
        _async_mode: bool,
    ) -> Result<()> {
        self.state.lock().unwrap().retain_calls += 1;
        Ok(())
    }
}

#[derive(Default)]
struct MemoryState {
    ensure_calls: usize,
    prime_calls: usize,
    prepare_calls: usize,
}

struct MockMemory {
    state: Arc<Mutex<MemoryState>>,
}

impl MockMemory {
    fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(MemoryState::default())),
        }
    }
}

impl MemoryBankClient for MockMemory {
    fn ensure_exists(&self) -> Result<()> {
        self.state.lock().unwrap().ensure_calls += 1;
        Ok(())
    }

    fn prime(&self) -> Result<()> {
        self.state.lock().unwrap().prime_calls += 1;
        Ok(())
    }

    fn prepare(&self, _topic: &str) -> Result<()> {
        self.state.lock().unwrap().prepare_calls += 1;
        Ok(())
    }
}

struct MockLlm {
    storm: ProductStormOutput,
}

impl MockLlm {
    fn new(storm: ProductStormOutput) -> Self {
        Self { storm }
    }
}

impl LlmClient for MockLlm {
    fn product_storm(&self, _task: &Task) -> Result<ProductStormOutput> {
        Ok(self.storm.clone())
    }

    fn spec_first(&self, _task: &Task, _storm: &ProductStormOutput) -> Result<SpecOutput> {
        Ok(SpecOutput {
            acceptance_criteria: vec!["Критерий 1".to_string(), "Критерий 2".to_string()],
            non_goals: vec![],
            requires_perf_gate: false,
        })
    }

    fn run_red_phase(&self, _task: &Task, _spec: &SpecOutput, _iteration: u32) -> Result<()> {
        Ok(())
    }

    fn run_green_phase(&self, _task: &Task, _spec: &SpecOutput, _iteration: u32) -> Result<()> {
        Ok(())
    }

    fn run_refactor_phase(&self, _task: &Task, _spec: &SpecOutput, _iteration: u32) -> Result<()> {
        Ok(())
    }

    fn review(&self, _task: &Task, _spec: &SpecOutput, _iteration: u32) -> Result<ReviewOutput> {
        Ok(ReviewOutput {
            status: ReviewStatus::Done,
            coverage_score: 1.0,
            missing_items: vec![],
            spawn_candidates: vec![],
            risk_flags: vec![],
        })
    }
}

fn parent_task() -> Task {
    Task {
        id: "parent-1".to_string(),
        title: "Implement loop".to_string(),
        description: "Main parent task".to_string(),
        status: TaskStatus::Open,
        labels: vec![],
        parent_id: None,
    }
}

fn child_parent_task() -> Task {
    Task {
        id: "parent-1.1".to_string(),
        title: "Child task".to_string(),
        description: "Nested task".to_string(),
        status: TaskStatus::Open,
        labels: vec![],
        parent_id: Some("parent-1".to_string()),
    }
}

fn run_config() -> RunConfig {
    RunConfig {
        task_id: None,
        bootstrap_goal: None,
        max_iterations: 3,
        timeout_minutes: 45,
        spawn_cap: 5,
        model: "gpt-5.3-codex".to_string(),
        hindsight_bank: "agent-loop".to_string(),
        profile: LoopProfile::Delivery,
        retain_mode: RetainMode::Sync,
        json_events: false,
        dry_run: false,
    }
}

fn candidate(title: &str, blocking: bool) -> SpawnTaskCandidate {
    SpawnTaskCandidate {
        title: title.to_string(),
        description: format!("{title} description"),
        kind: if blocking {
            SpawnTaskKind::Decomposition
        } else {
            SpawnTaskKind::Backlog
        },
        blocking,
        estimated_minutes: 30,
        can_inline: false,
        acceptance: vec!["ok".to_string()],
        priority: 2,
        labels: vec!["auto".to_string()],
    }
}

#[test]
fn no_ready_issue_returns_no_ready_work() {
    let beads = MockBeads::new(None, vec![]);
    let loop_engine = AgentLoop {
        config: run_config(),
        beads: Box::new(beads),
        hindsight: Box::new(MockHindsight::new()),
        memory_bank: Box::new(MockMemory::new()),
        llm: Box::new(MockLlm::new(ProductStormOutput::default())),
    };

    let outcome = loop_engine.run_once().unwrap();
    assert!(matches!(outcome, RunOutcome::NoReadyWork));
}

#[test]
fn bootstrap_creates_parent_task_when_no_tasks_exist() {
    let beads = MockBeads::new(None, vec![]);
    let state = beads.state.clone();
    let mut cfg = run_config();
    cfg.bootstrap_goal = Some("Собрать roadmap продукта".to_string());

    let outcome = AgentLoop {
        config: cfg,
        beads: Box::new(beads),
        hindsight: Box::new(MockHindsight::new()),
        memory_bank: Box::new(MockMemory::new()),
        llm: Box::new(MockLlm::new(ProductStormOutput::default())),
    }
    .run_once()
    .unwrap();

    match outcome {
        RunOutcome::AgentActionRequired { task_id, .. } => assert_eq!(task_id, "bootstrap-1"),
        _ => panic!("expected agent action required"),
    }

    let state = state.lock().unwrap();
    assert_eq!(state.bootstrap_calls, 1);
    assert_eq!(state.claim_calls, 0);
    assert_eq!(
        state.bootstrap_goals,
        vec!["Собрать roadmap продукта".to_string()]
    );
}

#[test]
fn emits_agent_commands_and_adds_notes() {
    let beads = MockBeads::new(Some(parent_task()), vec![]);
    let beads_state = beads.state.clone();

    let outcome = AgentLoop {
        config: run_config(),
        beads: Box::new(beads),
        hindsight: Box::new(MockHindsight::new()),
        memory_bank: Box::new(MockMemory::new()),
        llm: Box::new(MockLlm::new(ProductStormOutput::default())),
    }
    .run_once()
    .unwrap();

    match outcome {
        RunOutcome::AgentActionRequired {
            task_id, commands, ..
        } => {
            assert_eq!(task_id, "parent-1");
            assert!(!commands.is_empty());
            assert!(commands.iter().any(|x| x.contains("RED")));
            assert!(commands.iter().any(|x| x.contains("cargo test")));
        }
        _ => panic!("expected agent action required"),
    }

    let state = beads_state.lock().unwrap();
    assert_eq!(state.notes_calls, 1);
}

#[test]
fn enforces_spawn_cap_5() {
    let mut candidates = Vec::new();
    for i in 0..7 {
        candidates.push(candidate(&format!("task-{i}"), false));
    }

    let storm = ProductStormOutput {
        summary: "storm".to_string(),
        spawn_candidates: candidates,
    };

    let beads = MockBeads::new(Some(parent_task()), vec![]);
    let state = beads.state.clone();

    let outcome = AgentLoop {
        config: run_config(),
        beads: Box::new(beads),
        hindsight: Box::new(MockHindsight::new()),
        memory_bank: Box::new(MockMemory::new()),
        llm: Box::new(MockLlm::new(storm)),
    }
    .run_once()
    .unwrap();

    assert!(matches!(outcome, RunOutcome::AgentActionRequired { .. }));
    assert_eq!(state.lock().unwrap().created_ids.len(), 5);
}

#[test]
fn deduplicates_spawn_candidates() {
    let existing = Task {
        id: "child-existing".to_string(),
        title: "duplicate-title".to_string(),
        description: "existing".to_string(),
        status: TaskStatus::Open,
        labels: vec!["loop:followup".to_string()],
        parent_id: Some("parent-1".to_string()),
    };

    let storm = ProductStormOutput {
        summary: "storm".to_string(),
        spawn_candidates: vec![candidate("duplicate-title", false)],
    };

    let beads = MockBeads::new(Some(parent_task()), vec![existing]);
    let state = beads.state.clone();

    let outcome = AgentLoop {
        config: run_config(),
        beads: Box::new(beads),
        hindsight: Box::new(MockHindsight::new()),
        memory_bank: Box::new(MockMemory::new()),
        llm: Box::new(MockLlm::new(storm)),
    }
    .run_once()
    .unwrap();

    assert!(matches!(outcome, RunOutcome::AgentActionRequired { .. }));
    assert!(state.lock().unwrap().created_ids.is_empty());
}

#[test]
fn creates_blocking_dependency_for_blocking_child() {
    let storm = ProductStormOutput {
        summary: "storm".to_string(),
        spawn_candidates: vec![candidate("blocking-child", true)],
    };

    let beads = MockBeads::new(Some(parent_task()), vec![]);
    let state = beads.state.clone();

    let outcome = AgentLoop {
        config: run_config(),
        beads: Box::new(beads),
        hindsight: Box::new(MockHindsight::new()),
        memory_bank: Box::new(MockMemory::new()),
        llm: Box::new(MockLlm::new(storm)),
    }
    .run_once()
    .unwrap();

    assert!(matches!(outcome, RunOutcome::AgentActionRequired { .. }));
    assert_eq!(state.lock().unwrap().add_dep_calls, 1);
}

#[test]
fn child_tasks_do_not_spawn_more_children() {
    let storm = ProductStormOutput {
        summary: "storm".to_string(),
        spawn_candidates: vec![candidate("should-not-spawn", true)],
    };

    let beads = MockBeads::new(Some(child_parent_task()), vec![]);
    let state = beads.state.clone();

    let outcome = AgentLoop {
        config: run_config(),
        beads: Box::new(beads),
        hindsight: Box::new(MockHindsight::new()),
        memory_bank: Box::new(MockMemory::new()),
        llm: Box::new(MockLlm::new(storm)),
    }
    .run_once()
    .unwrap();

    assert!(matches!(outcome, RunOutcome::AgentActionRequired { .. }));
    assert!(state.lock().unwrap().created_ids.is_empty());
}

#[test]
fn retain_summary_happens_in_sync_mode() {
    let hindsight = MockHindsight::new();
    let state = hindsight.state.clone();

    let outcome = AgentLoop {
        config: run_config(),
        beads: Box::new(MockBeads::new(Some(parent_task()), vec![])),
        hindsight: Box::new(hindsight),
        memory_bank: Box::new(MockMemory::new()),
        llm: Box::new(MockLlm::new(ProductStormOutput::default())),
    }
    .run_once()
    .unwrap();

    assert!(matches!(outcome, RunOutcome::AgentActionRequired { .. }));
    assert_eq!(state.lock().unwrap().retain_calls, 1);
}
