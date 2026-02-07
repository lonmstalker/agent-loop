use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use anyhow::Result;

use agent_loop::config::{LoopProfile, RetainMode};
use agent_loop::contracts::{
    GateName, GateReport, ProductStormOutput, ReviewOutput, ReviewStatus, RunOutcome,
    SpawnTaskCandidate, SpawnTaskKind, SpecOutput, Task, TaskStatus,
};
use agent_loop::evaluator::DoneEvaluator;
use agent_loop::loop_runner::{
    AgentLoop, BeadsClient, HindsightClient, LlmClient, MemoryBankClient, QualityGateRunner,
    RunConfig,
};

#[derive(Default)]
struct BeadsState {
    ensure_calls: usize,
    bootstrap_calls: usize,
    claim_calls: usize,
    sync_calls: usize,
    add_dep_calls: usize,
    close_calls: usize,
    block_calls: usize,
    notes_calls: usize,
    created_ids: Vec<String>,
    closed_ids: Vec<String>,
    blocked_ids: Vec<String>,
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

        let mut labels = candidate.labels.clone();
        labels.push(if candidate.blocking {
            "loop:blocking".to_string()
        } else {
            "loop:followup".to_string()
        });

        let task = Task {
            id: child_id.clone(),
            title: candidate.title.clone(),
            description: candidate.description.clone(),
            status: TaskStatus::Open,
            labels,
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

    fn list_open_children(&self, parent_id: &str) -> Result<Vec<Task>> {
        let state = self.state.lock().unwrap();
        Ok(state
            .children
            .iter()
            .filter(|task| task.parent_id.as_deref() == Some(parent_id) && task.is_open())
            .cloned()
            .collect())
    }

    fn close_task(&self, task_id: &str, _reason: &str) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        state.close_calls += 1;
        state.closed_ids.push(task_id.to_string());
        if let Some(task) = state.children.iter_mut().find(|x| x.id == task_id) {
            task.status = TaskStatus::Closed;
        }
        Ok(())
    }

    fn block_task(&self, task_id: &str, _notes: &str) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        state.block_calls += 1;
        state.blocked_ids.push(task_id.to_string());
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

#[derive(Default)]
struct LlmState {
    red_calls: HashMap<String, usize>,
    green_calls: HashMap<String, usize>,
    refactor_calls: HashMap<String, usize>,
    review_calls: HashMap<String, usize>,
}

struct MockLlm {
    parent_storm: ProductStormOutput,
    child_storm: ProductStormOutput,
    parent_spec: SpecOutput,
    child_spec: SpecOutput,
    parent_reviews: Arc<Mutex<Vec<ReviewOutput>>>,
    child_review: ReviewOutput,
    state: Arc<Mutex<LlmState>>,
}

impl MockLlm {
    fn new(parent_storm: ProductStormOutput, parent_reviews: Vec<ReviewOutput>) -> Self {
        Self {
            parent_storm,
            child_storm: ProductStormOutput::default(),
            parent_spec: SpecOutput::default(),
            child_spec: SpecOutput::default(),
            parent_reviews: Arc::new(Mutex::new(parent_reviews)),
            child_review: ReviewOutput {
                status: ReviewStatus::Done,
                coverage_score: 1.0,
                missing_items: vec![],
                spawn_candidates: vec![],
                risk_flags: vec![],
            },
            state: Arc::new(Mutex::new(LlmState::default())),
        }
    }

    fn inc(map: &mut HashMap<String, usize>, task_id: &str) {
        *map.entry(task_id.to_string()).or_insert(0) += 1;
    }
}

impl LlmClient for MockLlm {
    fn product_storm(&self, task: &Task) -> Result<ProductStormOutput> {
        if task.parent_id.is_none() {
            Ok(self.parent_storm.clone())
        } else {
            Ok(self.child_storm.clone())
        }
    }

    fn spec_first(&self, task: &Task, _storm: &ProductStormOutput) -> Result<SpecOutput> {
        if task.parent_id.is_none() {
            Ok(self.parent_spec.clone())
        } else {
            Ok(self.child_spec.clone())
        }
    }

    fn run_red_phase(&self, task: &Task, _spec: &SpecOutput, _iteration: u32) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        Self::inc(&mut state.red_calls, &task.id);
        Ok(())
    }

    fn run_green_phase(&self, task: &Task, _spec: &SpecOutput, _iteration: u32) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        Self::inc(&mut state.green_calls, &task.id);
        Ok(())
    }

    fn run_refactor_phase(&self, task: &Task, _spec: &SpecOutput, _iteration: u32) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        Self::inc(&mut state.refactor_calls, &task.id);
        Ok(())
    }

    fn review(&self, task: &Task, _spec: &SpecOutput, _iteration: u32) -> Result<ReviewOutput> {
        let mut state = self.state.lock().unwrap();
        Self::inc(&mut state.review_calls, &task.id);
        drop(state);

        if task.parent_id.is_none() {
            let mut reviews = self.parent_reviews.lock().unwrap();
            if reviews.is_empty() {
                return Ok(ReviewOutput {
                    status: ReviewStatus::Done,
                    coverage_score: 1.0,
                    missing_items: vec![],
                    spawn_candidates: vec![],
                    risk_flags: vec![],
                });
            }
            Ok(reviews.remove(0))
        } else {
            Ok(self.child_review.clone())
        }
    }
}

struct MockGates {
    parent_reports: Arc<Mutex<Vec<GateReport>>>,
    child_report: GateReport,
}

impl MockGates {
    fn new(parent_reports: Vec<GateReport>) -> Self {
        Self {
            parent_reports: Arc::new(Mutex::new(parent_reports)),
            child_report: GateReport::all_green(),
        }
    }
}

impl QualityGateRunner for MockGates {
    fn run_core_gates(&self, task: &Task, _spec: &SpecOutput) -> Result<GateReport> {
        if task.parent_id.is_none() {
            let mut reports = self.parent_reports.lock().unwrap();
            if reports.is_empty() {
                return Ok(GateReport::all_green());
            }
            Ok(reports.remove(0))
        } else {
            Ok(self.child_report.clone())
        }
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
        labels: vec!["loop:blocking".to_string()],
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
        child_inline_timeout_minutes: 10,
        small_child_threshold_minutes: 20,
        model: "gpt-5.3-codex".to_string(),
        hindsight_bank: "agent-loop".to_string(),
        profile: LoopProfile::Delivery,
        non_blocking_gates: HashSet::new(),
        retain_mode: RetainMode::Sync,
        json_events: false,
        dry_run: false,
    }
}

fn candidate(
    title: &str,
    blocking: bool,
    can_inline: bool,
    minutes: u32,
    kind: SpawnTaskKind,
) -> SpawnTaskCandidate {
    SpawnTaskCandidate {
        title: title.to_string(),
        description: format!("{title} description"),
        kind,
        blocking,
        estimated_minutes: minutes,
        can_inline,
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
        llm: Box::new(MockLlm::new(ProductStormOutput::default(), vec![])),
        gates: Box::new(MockGates::new(vec![GateReport::all_green()])),
        evaluator: DoneEvaluator::default(),
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
        llm: Box::new(MockLlm::new(
            ProductStormOutput::default(),
            vec![ReviewOutput {
                status: ReviewStatus::Done,
                coverage_score: 1.0,
                missing_items: vec![],
                spawn_candidates: vec![],
                risk_flags: vec![],
            }],
        )),
        gates: Box::new(MockGates::new(vec![GateReport::all_green()])),
        evaluator: DoneEvaluator::default(),
    }
    .run_once()
    .unwrap();

    match outcome {
        RunOutcome::Done { task_id, .. } => assert_eq!(task_id, "bootstrap-1"),
        _ => panic!("expected done"),
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
fn happy_path_done_first_iteration() {
    let beads = MockBeads::new(Some(parent_task()), vec![]);
    let hindsight = MockHindsight::new();
    let memory = MockMemory::new();
    let llm = MockLlm::new(
        ProductStormOutput::default(),
        vec![ReviewOutput {
            status: ReviewStatus::Done,
            coverage_score: 0.95,
            missing_items: vec![],
            spawn_candidates: vec![],
            risk_flags: vec![],
        }],
    );

    let outcome = AgentLoop {
        config: run_config(),
        beads: Box::new(MockBeads {
            state: beads.state.clone(),
            next_child: beads.next_child.clone(),
        }),
        hindsight: Box::new(MockHindsight {
            state: hindsight.state.clone(),
        }),
        memory_bank: Box::new(MockMemory {
            state: memory.state.clone(),
        }),
        llm: Box::new(MockLlm {
            parent_storm: llm.parent_storm.clone(),
            child_storm: llm.child_storm.clone(),
            parent_spec: llm.parent_spec.clone(),
            child_spec: llm.child_spec.clone(),
            parent_reviews: llm.parent_reviews.clone(),
            child_review: llm.child_review.clone(),
            state: llm.state.clone(),
        }),
        gates: Box::new(MockGates::new(vec![GateReport::all_green()])),
        evaluator: DoneEvaluator::default(),
    }
    .run_once()
    .unwrap();

    match outcome {
        RunOutcome::Done { task_id, .. } => assert_eq!(task_id, "parent-1"),
        _ => panic!("expected done outcome"),
    }

    assert_eq!(beads.state.lock().unwrap().ensure_calls, 1);
    assert_eq!(memory.state.lock().unwrap().ensure_calls, 1);
    assert_eq!(hindsight.state.lock().unwrap().retain_calls, 1);
}

#[test]
fn rework_then_done() {
    let reviews = vec![
        ReviewOutput {
            status: ReviewStatus::Rework,
            coverage_score: 0.5,
            missing_items: vec!["missing".to_string()],
            spawn_candidates: vec![],
            risk_flags: vec![],
        },
        ReviewOutput {
            status: ReviewStatus::Done,
            coverage_score: 0.95,
            missing_items: vec![],
            spawn_candidates: vec![],
            risk_flags: vec![],
        },
    ];

    let llm = MockLlm::new(ProductStormOutput::default(), reviews);
    let state = llm.state.clone();
    let outcome = AgentLoop {
        config: run_config(),
        beads: Box::new(MockBeads::new(Some(parent_task()), vec![])),
        hindsight: Box::new(MockHindsight::new()),
        memory_bank: Box::new(MockMemory::new()),
        llm: Box::new(llm),
        gates: Box::new(MockGates::new(vec![
            GateReport::all_green(),
            GateReport::all_green(),
        ])),
        evaluator: DoneEvaluator::default(),
    }
    .run_once()
    .unwrap();

    assert!(matches!(outcome, RunOutcome::Done { .. }));
    let review_calls = state
        .lock()
        .unwrap()
        .review_calls
        .get("parent-1")
        .copied()
        .unwrap_or(0);
    assert_eq!(review_calls, 2);
}

#[test]
fn gate_failure_forces_rework_then_needs_human_on_limit() {
    let mut cfg = run_config();
    cfg.max_iterations = 1;

    let outcome = AgentLoop {
        config: cfg,
        beads: Box::new(MockBeads::new(Some(parent_task()), vec![])),
        hindsight: Box::new(MockHindsight::new()),
        memory_bank: Box::new(MockMemory::new()),
        llm: Box::new(MockLlm::new(
            ProductStormOutput::default(),
            vec![ReviewOutput {
                status: ReviewStatus::Done,
                coverage_score: 1.0,
                missing_items: vec![],
                spawn_candidates: vec![],
                risk_flags: vec![],
            }],
        )),
        gates: Box::new(MockGates::new(vec![GateReport {
            fmt_ok: true,
            clippy_ok: false,
            tests_ok: true,
            perf_ok: true,
        }])),
        evaluator: DoneEvaluator::default(),
    }
    .run_once()
    .unwrap();

    match outcome {
        RunOutcome::NeedsHuman { reason, .. } => {
            assert!(reason.contains("quality gates failed"));
        }
        _ => panic!("expected needs human"),
    }
}

#[test]
fn non_blocking_clippy_creates_debt_child_and_allows_done() {
    let mut cfg = run_config();
    cfg.non_blocking_gates.insert(GateName::Clippy);

    let beads = MockBeads::new(Some(parent_task()), vec![]);
    let beads_state = beads.state.clone();
    let outcome = AgentLoop {
        config: cfg,
        beads: Box::new(beads),
        hindsight: Box::new(MockHindsight::new()),
        memory_bank: Box::new(MockMemory::new()),
        llm: Box::new(MockLlm::new(
            ProductStormOutput::default(),
            vec![ReviewOutput {
                status: ReviewStatus::Done,
                coverage_score: 1.0,
                missing_items: vec![],
                spawn_candidates: vec![],
                risk_flags: vec![],
            }],
        )),
        gates: Box::new(MockGates::new(vec![GateReport {
            fmt_ok: true,
            clippy_ok: false,
            tests_ok: true,
            perf_ok: true,
        }])),
        evaluator: DoneEvaluator::default(),
    }
    .run_once()
    .unwrap();

    assert!(matches!(outcome, RunOutcome::Done { .. }));
    let state = beads_state.lock().unwrap();
    assert!(
        state
            .children
            .iter()
            .any(|task| task.title.contains("Fix clippy gate")),
        "expected clippy debt child task"
    );
}

#[test]
fn timeout_exceeded_returns_needs_human() {
    let mut cfg = run_config();
    cfg.timeout_minutes = 0;

    let outcome = AgentLoop {
        config: cfg,
        beads: Box::new(MockBeads::new(Some(parent_task()), vec![])),
        hindsight: Box::new(MockHindsight::new()),
        memory_bank: Box::new(MockMemory::new()),
        llm: Box::new(MockLlm::new(
            ProductStormOutput::default(),
            vec![ReviewOutput {
                status: ReviewStatus::Done,
                coverage_score: 1.0,
                missing_items: vec![],
                spawn_candidates: vec![],
                risk_flags: vec![],
            }],
        )),
        gates: Box::new(MockGates::new(vec![GateReport::all_green()])),
        evaluator: DoneEvaluator::default(),
    }
    .run_once()
    .unwrap();

    match outcome {
        RunOutcome::NeedsHuman { reason, .. } => assert!(reason.contains("timeout")),
        _ => panic!("expected timeout needs human"),
    }
}

#[test]
fn enforces_spawn_cap_5() {
    let mut candidates = Vec::new();
    for i in 0..7 {
        candidates.push(candidate(
            &format!("task-{i}"),
            false,
            false,
            30,
            SpawnTaskKind::Backlog,
        ));
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
        llm: Box::new(MockLlm::new(
            storm,
            vec![ReviewOutput {
                status: ReviewStatus::Done,
                coverage_score: 1.0,
                missing_items: vec![],
                spawn_candidates: vec![],
                risk_flags: vec![],
            }],
        )),
        gates: Box::new(MockGates::new(vec![GateReport::all_green()])),
        evaluator: DoneEvaluator::default(),
    }
    .run_once()
    .unwrap();

    assert!(matches!(outcome, RunOutcome::Done { .. }));
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
        spawn_candidates: vec![candidate(
            "duplicate-title",
            false,
            false,
            30,
            SpawnTaskKind::Backlog,
        )],
    };

    let beads = MockBeads::new(Some(parent_task()), vec![existing]);
    let state = beads.state.clone();

    let outcome = AgentLoop {
        config: run_config(),
        beads: Box::new(beads),
        hindsight: Box::new(MockHindsight::new()),
        memory_bank: Box::new(MockMemory::new()),
        llm: Box::new(MockLlm::new(
            storm,
            vec![ReviewOutput {
                status: ReviewStatus::Done,
                coverage_score: 1.0,
                missing_items: vec![],
                spawn_candidates: vec![],
                risk_flags: vec![],
            }],
        )),
        gates: Box::new(MockGates::new(vec![GateReport::all_green()])),
        evaluator: DoneEvaluator::default(),
    }
    .run_once()
    .unwrap();

    assert!(matches!(outcome, RunOutcome::Done { .. }));
    assert!(state.lock().unwrap().created_ids.is_empty());
}

#[test]
fn parent_not_done_with_open_blocking_children() {
    let storm = ProductStormOutput {
        summary: "storm".to_string(),
        spawn_candidates: vec![candidate(
            "blocking-child",
            true,
            false,
            15,
            SpawnTaskKind::Decomposition,
        )],
    };

    let mut cfg = run_config();
    cfg.max_iterations = 1;

    let beads = MockBeads::new(Some(parent_task()), vec![]);
    let beads_state = beads.state.clone();

    let outcome = AgentLoop {
        config: cfg,
        beads: Box::new(beads),
        hindsight: Box::new(MockHindsight::new()),
        memory_bank: Box::new(MockMemory::new()),
        llm: Box::new(MockLlm::new(
            storm,
            vec![ReviewOutput {
                status: ReviewStatus::Done,
                coverage_score: 1.0,
                missing_items: vec![],
                spawn_candidates: vec![],
                risk_flags: vec![],
            }],
        )),
        gates: Box::new(MockGates::new(vec![GateReport::all_green()])),
        evaluator: DoneEvaluator::default(),
    }
    .run_once()
    .unwrap();

    assert!(matches!(outcome, RunOutcome::NeedsHuman { .. }));
    assert_eq!(beads_state.lock().unwrap().add_dep_calls, 0);
}

#[test]
fn parent_done_with_open_non_blocking_children() {
    let storm = ProductStormOutput {
        summary: "storm".to_string(),
        spawn_candidates: vec![candidate(
            "followup-child",
            false,
            false,
            15,
            SpawnTaskKind::Backlog,
        )],
    };

    let outcome = AgentLoop {
        config: run_config(),
        beads: Box::new(MockBeads::new(Some(parent_task()), vec![])),
        hindsight: Box::new(MockHindsight::new()),
        memory_bank: Box::new(MockMemory::new()),
        llm: Box::new(MockLlm::new(
            storm,
            vec![ReviewOutput {
                status: ReviewStatus::Done,
                coverage_score: 1.0,
                missing_items: vec![],
                spawn_candidates: vec![],
                risk_flags: vec![],
            }],
        )),
        gates: Box::new(MockGates::new(vec![GateReport::all_green()])),
        evaluator: DoneEvaluator::default(),
    }
    .run_once()
    .unwrap();

    assert!(matches!(outcome, RunOutcome::Done { .. }));
}

#[test]
fn executes_small_child_inline_and_does_not_consume_parent_iterations() {
    let storm = ProductStormOutput {
        summary: "storm".to_string(),
        spawn_candidates: vec![candidate(
            "inline-child",
            false,
            true,
            15,
            SpawnTaskKind::Quality,
        )],
    };

    let reviews = vec![
        ReviewOutput {
            status: ReviewStatus::Rework,
            coverage_score: 0.5,
            missing_items: vec!["missing".to_string()],
            spawn_candidates: vec![],
            risk_flags: vec![],
        },
        ReviewOutput {
            status: ReviewStatus::Done,
            coverage_score: 0.95,
            missing_items: vec![],
            spawn_candidates: vec![],
            risk_flags: vec![],
        },
    ];
    let llm = MockLlm::new(storm, reviews);
    let llm_state = llm.state.clone();

    let beads = MockBeads::new(Some(parent_task()), vec![]);
    let beads_state = beads.state.clone();

    let mut cfg = run_config();
    cfg.max_iterations = 2;

    let outcome = AgentLoop {
        config: cfg,
        beads: Box::new(beads),
        hindsight: Box::new(MockHindsight::new()),
        memory_bank: Box::new(MockMemory::new()),
        llm: Box::new(llm),
        gates: Box::new(MockGates::new(vec![
            GateReport::all_green(),
            GateReport::all_green(),
        ])),
        evaluator: DoneEvaluator::default(),
    }
    .run_once()
    .unwrap();

    assert!(matches!(outcome, RunOutcome::Done { .. }));

    let reviews_map = &llm_state.lock().unwrap().review_calls;
    let parent_reviews = reviews_map.get("parent-1").copied().unwrap_or(0);
    assert_eq!(parent_reviews, 2);

    let child_review_calls = reviews_map
        .keys()
        .filter(|k| k.starts_with("child-"))
        .count();
    assert!(child_review_calls >= 1);

    let closed: HashSet<_> = beads_state
        .lock()
        .unwrap()
        .closed_ids
        .iter()
        .cloned()
        .collect();
    assert!(closed.contains("child-1"));
}

#[test]
fn retain_summary_happens_on_needs_human() {
    let hindsight = MockHindsight::new();
    let state = hindsight.state.clone();

    let mut cfg = run_config();
    cfg.max_iterations = 1;

    let outcome = AgentLoop {
        config: cfg,
        beads: Box::new(MockBeads::new(Some(parent_task()), vec![])),
        hindsight: Box::new(hindsight),
        memory_bank: Box::new(MockMemory::new()),
        llm: Box::new(MockLlm::new(
            ProductStormOutput::default(),
            vec![ReviewOutput {
                status: ReviewStatus::Rework,
                coverage_score: 0.1,
                missing_items: vec!["miss".to_string()],
                spawn_candidates: vec![],
                risk_flags: vec![],
            }],
        )),
        gates: Box::new(MockGates::new(vec![GateReport::all_green()])),
        evaluator: DoneEvaluator::default(),
    }
    .run_once()
    .unwrap();

    assert!(matches!(outcome, RunOutcome::NeedsHuman { .. }));
    assert_eq!(state.lock().unwrap().retain_calls, 1);
}

#[test]
fn child_tasks_do_not_spawn_more_children() {
    let storm = ProductStormOutput {
        summary: "storm".to_string(),
        spawn_candidates: vec![candidate(
            "should-not-spawn",
            true,
            false,
            15,
            SpawnTaskKind::Decomposition,
        )],
    };

    let beads = MockBeads::new(Some(child_parent_task()), vec![]);
    let state = beads.state.clone();

    let outcome = AgentLoop {
        config: run_config(),
        beads: Box::new(beads),
        hindsight: Box::new(MockHindsight::new()),
        memory_bank: Box::new(MockMemory::new()),
        llm: Box::new(MockLlm::new(
            storm,
            vec![ReviewOutput {
                status: ReviewStatus::Done,
                coverage_score: 1.0,
                missing_items: vec![],
                spawn_candidates: vec![],
                risk_flags: vec![],
            }],
        )),
        gates: Box::new(MockGates::new(vec![GateReport::all_green()])),
        evaluator: DoneEvaluator::default(),
    }
    .run_once()
    .unwrap();

    assert!(matches!(outcome, RunOutcome::Done { .. }));
    assert!(state.lock().unwrap().created_ids.is_empty());
}

#[test]
fn decomposition_task_closes_as_handoff_when_children_exist() {
    let mut task = child_parent_task();
    task.labels.push("loop:kind:decomposition".to_string());
    task.title = "Аудит прод-готовности".to_string();

    let existing_child = Task {
        id: "child-existing-1".to_string(),
        title: "Follow-up".to_string(),
        description: "child".to_string(),
        status: TaskStatus::Open,
        labels: vec!["loop:blocking".to_string()],
        parent_id: Some(task.id.clone()),
    };

    let beads = MockBeads::new(Some(task), vec![existing_child]);
    let state = beads.state.clone();

    let outcome = AgentLoop {
        config: run_config(),
        beads: Box::new(beads),
        hindsight: Box::new(MockHindsight::new()),
        memory_bank: Box::new(MockMemory::new()),
        llm: Box::new(MockLlm::new(
            ProductStormOutput::default(),
            vec![ReviewOutput {
                status: ReviewStatus::Rework,
                coverage_score: 0.0,
                missing_items: vec!["delegate".to_string()],
                spawn_candidates: vec![],
                risk_flags: vec![],
            }],
        )),
        gates: Box::new(MockGates::new(vec![GateReport::all_green()])),
        evaluator: DoneEvaluator::default(),
    }
    .run_once()
    .unwrap();

    assert!(matches!(outcome, RunOutcome::Done { .. }));
    let state = state.lock().unwrap();
    assert_eq!(state.close_calls, 1);
}
