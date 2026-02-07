use std::collections::HashSet;
use std::time::{Duration, Instant};

use anyhow::Result;
use serde_json::json;

use crate::config::{LoopProfile, RetainMode, RunCommand};
use crate::contracts::{
    DoneDecision, GateName, GateReport, ProductStormOutput, ReviewOutput, RunOutcome,
    SpawnTaskCandidate, SpawnTaskKind, SpecOutput, Task,
};
use crate::evaluator::{DoneEvaluator, ParentDecisionInput};

#[derive(Debug, Clone)]
pub struct RunConfig {
    pub task_id: Option<String>,
    pub bootstrap_goal: Option<String>,
    pub max_iterations: u32,
    pub timeout_minutes: u64,
    pub spawn_cap: usize,
    pub child_inline_timeout_minutes: u64,
    pub small_child_threshold_minutes: u32,
    pub model: String,
    pub hindsight_bank: String,
    pub profile: LoopProfile,
    pub non_blocking_gates: HashSet<GateName>,
    pub retain_mode: RetainMode,
    pub json_events: bool,
    pub dry_run: bool,
}

impl From<RunCommand> for RunConfig {
    fn from(value: RunCommand) -> Self {
        let task_id = value.task.clone();
        let bootstrap_goal = value.resolved_bootstrap_goal();
        let model = value.resolved_model();
        let profile = value.profile;
        let mut non_blocking_gates = value.resolved_non_blocking_gates();
        if matches!(profile, LoopProfile::Discovery) {
            non_blocking_gates.insert(GateName::Clippy);
        }
        if matches!(profile, LoopProfile::Hardening) {
            non_blocking_gates.clear();
        }
        Self {
            task_id,
            bootstrap_goal,
            max_iterations: value.max_iterations,
            timeout_minutes: value.timeout_minutes,
            spawn_cap: value.spawn_cap,
            child_inline_timeout_minutes: value.child_inline_timeout_minutes,
            small_child_threshold_minutes: value.small_child_threshold_minutes,
            model,
            hindsight_bank: value.hindsight_bank,
            profile,
            non_blocking_gates,
            retain_mode: value.retain_mode,
            json_events: value.json_events,
            dry_run: value.dry_run,
        }
    }
}

impl RunConfig {
    pub fn timeout(&self) -> Duration {
        Duration::from_secs(self.timeout_minutes.saturating_mul(60))
    }

    pub fn child_timeout(&self) -> Duration {
        Duration::from_secs(self.child_inline_timeout_minutes.saturating_mul(60))
    }
}

pub trait BeadsClient: Send + Sync {
    fn ensure_initialized(&self) -> Result<()>;
    fn create_bootstrap_parent_task(&self, goal: &str) -> Result<Task>;
    fn claim_parent_task(&self, preferred_task: Option<&str>) -> Result<Option<Task>>;
    fn sync(&self) -> Result<()>;
    fn search_duplicates(
        &self,
        parent_id: &str,
        candidate: &SpawnTaskCandidate,
    ) -> Result<Vec<Task>>;
    fn create_child_task(&self, parent_id: &str, candidate: &SpawnTaskCandidate) -> Result<Task>;
    fn add_blocking_dependency(&self, child_id: &str, parent_id: &str) -> Result<()>;
    fn list_open_children(&self, parent_id: &str) -> Result<Vec<Task>>;
    fn close_task(&self, task_id: &str, reason: &str) -> Result<()>;
    fn block_task(&self, task_id: &str, notes: &str) -> Result<()>;
    fn add_notes(&self, task_id: &str, notes: &str) -> Result<()>;
}

pub trait HindsightClient: Send + Sync {
    fn ensure_available(&self) -> Result<()>;
    fn ensure_project_bank(&self, bank: &str) -> Result<()>;
    fn recall_preferences(&self) -> Result<()>;
    fn recall_task_context(&self, bank: &str, query: &str) -> Result<()>;
    fn retain_summary(
        &self,
        bank: &str,
        doc_id: &str,
        context: &str,
        content: &str,
        async_mode: bool,
    ) -> Result<()>;
}

pub trait MemoryBankClient: Send + Sync {
    fn ensure_exists(&self) -> Result<()>;
    fn prime(&self) -> Result<()>;
    fn prepare(&self, topic: &str) -> Result<()>;
}

pub trait LlmClient: Send + Sync {
    fn product_storm(&self, task: &Task) -> Result<ProductStormOutput>;
    fn spec_first(&self, task: &Task, storm: &ProductStormOutput) -> Result<SpecOutput>;
    fn run_red_phase(&self, task: &Task, spec: &SpecOutput, iteration: u32) -> Result<()>;
    fn run_green_phase(&self, task: &Task, spec: &SpecOutput, iteration: u32) -> Result<()>;
    fn run_refactor_phase(&self, task: &Task, spec: &SpecOutput, iteration: u32) -> Result<()>;
    fn review(&self, task: &Task, spec: &SpecOutput, iteration: u32) -> Result<ReviewOutput>;
}

pub trait QualityGateRunner: Send + Sync {
    fn run_core_gates(&self, task: &Task, spec: &SpecOutput) -> Result<GateReport>;
}

pub struct AgentLoop {
    pub config: RunConfig,
    pub beads: Box<dyn BeadsClient>,
    pub hindsight: Box<dyn HindsightClient>,
    pub memory_bank: Box<dyn MemoryBankClient>,
    pub llm: Box<dyn LlmClient>,
    pub gates: Box<dyn QualityGateRunner>,
    pub evaluator: DoneEvaluator,
}

impl AgentLoop {
    fn emit_json_event(&self, phase: &str, detail: &str) {
        if !self.config.json_events {
            return;
        }
        let event = json!({
            "type": "agent_loop_event",
            "phase": phase,
            "detail": detail,
            "profile": format!("{:?}", self.config.profile).to_lowercase(),
        });
        println!("{event}");
    }

    fn step(&self, message: &str) {
        eprintln!("[agent-loop] {message}");
        self.emit_json_event("progress", message);
    }

    fn retain_summary_best_effort(&self, task: &Task, content: String) {
        if self.config.dry_run {
            self.step("dry-run: skipping hindsight retain_summary");
            return;
        }

        if matches!(self.config.retain_mode, RetainMode::Off) {
            self.step("retain-mode=off: skipping hindsight retain_summary");
            return;
        }

        let async_mode = matches!(self.config.retain_mode, RetainMode::Async);
        self.step(if async_mode {
            "retaining summary to hindsight (async)"
        } else {
            "retaining summary to hindsight"
        });
        if let Err(err) = self.hindsight.retain_summary(
            &self.config.hindsight_bank,
            &format!("task-{}", task.id),
            &task.title,
            &content,
            async_mode,
        ) {
            self.step(&format!("warning: failed to retain summary: {err}"));
            return;
        }
        self.step("hindsight summary retained");
    }

    fn format_gate_statuses(gates: &GateReport) -> String {
        format!(
            "fmt={}, clippy={}, tests={}, perf={}",
            gates.fmt_ok, gates.clippy_ok, gates.tests_ok, gates.perf_ok
        )
    }

    fn create_non_blocking_gate_debt(
        &self,
        parent: &Task,
        gates: &GateReport,
        spawn_budget: &mut usize,
        created_children: &mut Vec<String>,
    ) -> Result<()> {
        if parent.parent_id.is_some() {
            return Ok(());
        }

        let non_blocking_failures = gates
            .failing_gates()
            .into_iter()
            .filter(|gate| self.config.non_blocking_gates.contains(gate))
            .collect::<Vec<_>>();

        for gate in non_blocking_failures {
            if *spawn_budget == 0 {
                self.step("spawn budget exhausted while creating quality debt tasks");
                break;
            }

            let candidate = SpawnTaskCandidate {
                title: format!("[Quality] Fix {} gate for {}", gate, parent.id),
                description: format!(
                    "Auto-generated quality debt: non-blocking gate '{}' failed for parent task {}.\nGate statuses: {}",
                    gate,
                    parent.id,
                    Self::format_gate_statuses(gates)
                ),
                kind: SpawnTaskKind::Quality,
                blocking: false,
                estimated_minutes: 30,
                can_inline: false,
                acceptance: vec![format!("{} gate passes for task {}", gate, parent.id)],
                priority: 2,
                labels: vec!["auto-generated".to_string(), "quality-debt".to_string()],
            };

            let duplicates = self.beads.search_duplicates(&parent.id, &candidate)?;
            if !duplicates.is_empty() {
                continue;
            }

            if self.config.dry_run {
                *spawn_budget = spawn_budget.saturating_sub(1);
                continue;
            }

            let child = self.beads.create_child_task(&parent.id, &candidate)?;
            created_children.push(child.id.clone());
            self.beads.sync()?;
            *spawn_budget = spawn_budget.saturating_sub(1);
            self.step(&format!("created quality debt child: {}", child.id));
        }

        Ok(())
    }

    pub fn run_once(&self) -> Result<RunOutcome> {
        self.step("starting single-run loop");
        self.hindsight.ensure_available()?;
        self.step("hindsight available");
        self.hindsight.recall_preferences()?;
        self.step("hindsight preferences recalled");
        self.hindsight
            .ensure_project_bank(&self.config.hindsight_bank)?;
        self.step(&format!(
            "hindsight project bank ready: {}",
            self.config.hindsight_bank
        ));

        self.memory_bank.ensure_exists()?;
        self.memory_bank.prime()?;
        self.step("memory-bank PRIME completed");

        self.beads.ensure_initialized()?;
        self.step("beads initialized");

        let parent = if let Some(goal) = self.config.bootstrap_goal.as_deref() {
            self.step("creating bootstrap parent task");
            self.beads.create_bootstrap_parent_task(goal)?
        } else {
            self.step("claiming parent task");
            match self
                .beads
                .claim_parent_task(self.config.task_id.as_deref())?
            {
                Some(task) => task,
                None => return Ok(RunOutcome::NoReadyWork),
            }
        };
        self.step(&format!("parent task: {} ({})", parent.id, parent.title));

        self.beads.sync()?;
        self.step("beads synced");

        self.hindsight
            .recall_task_context(&self.config.hindsight_bank, &parent.title)?;
        self.step("hindsight task context recalled");
        self.memory_bank.prepare(&parent.title)?;
        self.step("memory-bank PREPARE completed");

        self.step("running product-storm");
        let storm = self.llm.product_storm(&parent)?;
        self.step("running spec-first");
        let spec = self.llm.spec_first(&parent, &storm)?;

        let mut spawn_budget = self.config.spawn_cap;
        let mut created_children = Vec::new();
        let mut closed_children = Vec::new();
        let can_spawn_children = parent.parent_id.is_none();

        if can_spawn_children {
            self.process_spawn_candidates(
                &parent,
                storm.spawn_candidates,
                &mut spawn_budget,
                &mut created_children,
                &mut closed_children,
            )?;
            self.step("initial spawn candidates processed");
        }

        let started_at = Instant::now();
        let timeout = self.config.timeout();

        for iteration in 1..=self.config.max_iterations {
            self.step(&format!(
                "iteration {iteration}/{}",
                self.config.max_iterations
            ));
            if started_at.elapsed() >= timeout {
                let reason = "timeout exceeded".to_string();
                self.beads.block_task(&parent.id, &reason)?;
                self.beads.sync()?;
                self.retain_summary_best_effort(
                    &parent,
                    format!("Task requires human intervention: {reason}"),
                );
                return Ok(RunOutcome::NeedsHuman {
                    task_id: parent.id,
                    reason,
                    created_children,
                    closed_children,
                });
            }

            self.llm.run_red_phase(&parent, &spec, iteration)?;
            self.llm.run_green_phase(&parent, &spec, iteration)?;
            self.llm.run_refactor_phase(&parent, &spec, iteration)?;
            self.step("tdd cycle completed");

            let gates = self.gates.run_core_gates(&parent, &spec)?;
            self.step(&format!(
                "quality gates: {}",
                Self::format_gate_statuses(&gates)
            ));

            self.create_non_blocking_gate_debt(
                &parent,
                &gates,
                &mut spawn_budget,
                &mut created_children,
            )?;

            let review = self.llm.review(&parent, &spec, iteration)?;
            self.step(&format!(
                "review: status={:?}, coverage={:.2}",
                review.status, review.coverage_score
            ));

            if can_spawn_children {
                self.process_spawn_candidates(
                    &parent,
                    review.spawn_candidates.clone(),
                    &mut spawn_budget,
                    &mut created_children,
                    &mut closed_children,
                )?;
                self.step("review spawn candidates processed");
            }

            let open_children = self.beads.list_open_children(&parent.id)?;
            let has_open_blocking_children =
                open_children.iter().any(|task| task.is_blocking_child());

            if self.should_close_as_decomposition_handoff(&parent, &gates, &open_children) {
                self.beads
                    .close_task(&parent.id, "agent-loop: decomposition handoff completed")?;
                self.beads.sync()?;
                self.retain_summary_best_effort(
                    &parent,
                    "Task closed as decomposition handoff with delegated child tasks".to_string(),
                );
                return Ok(RunOutcome::Done {
                    task_id: parent.id,
                    created_children,
                    closed_children,
                });
            }

            let decision = self.evaluator.decide_parent(&ParentDecisionInput {
                gates: &gates,
                non_blocking_gates: &self.config.non_blocking_gates,
                review: &review,
                has_open_blocking_children,
                parent_iteration: iteration,
                max_iterations: self.config.max_iterations,
                elapsed: started_at.elapsed(),
                timeout,
            });

            match decision {
                DoneDecision::Done => {
                    self.step("decision: DONE");
                    self.beads.close_task(&parent.id, "agent-loop: completed")?;
                    self.beads.sync()?;
                    self.retain_summary_best_effort(
                        &parent,
                        "Task completed with done decision".to_string(),
                    );
                    return Ok(RunOutcome::Done {
                        task_id: parent.id,
                        created_children,
                        closed_children,
                    });
                }
                DoneDecision::Rework => {
                    self.step("decision: REWORK");
                    continue;
                }
                DoneDecision::NeedsHuman => {
                    self.step("decision: NEEDS_HUMAN");
                    let reason = self.needs_human_reason(
                        &gates,
                        &review,
                        has_open_blocking_children,
                        started_at.elapsed() >= timeout,
                    );
                    self.beads.block_task(&parent.id, &reason)?;
                    self.beads.sync()?;
                    self.retain_summary_best_effort(
                        &parent,
                        format!("Task requires human intervention: {reason}"),
                    );
                    return Ok(RunOutcome::NeedsHuman {
                        task_id: parent.id,
                        reason,
                        created_children,
                        closed_children,
                    });
                }
            }
        }

        let reason = "parent iteration limit reached".to_string();
        self.beads.block_task(&parent.id, &reason)?;
        self.beads.sync()?;
        self.retain_summary_best_effort(
            &parent,
            format!("Task requires human intervention: {reason}"),
        );
        Ok(RunOutcome::NeedsHuman {
            task_id: parent.id,
            reason,
            created_children,
            closed_children,
        })
    }

    fn process_spawn_candidates(
        &self,
        parent: &Task,
        candidates: Vec<SpawnTaskCandidate>,
        spawn_budget: &mut usize,
        created_children: &mut Vec<String>,
        closed_children: &mut Vec<String>,
    ) -> Result<()> {
        for candidate in candidates {
            if *spawn_budget == 0 {
                break;
            }

            let duplicates = self.beads.search_duplicates(&parent.id, &candidate)?;
            if !duplicates.is_empty() {
                continue;
            }

            if self.config.dry_run {
                *spawn_budget = spawn_budget.saturating_sub(1);
                continue;
            }

            let child = self.beads.create_child_task(&parent.id, &candidate)?;
            created_children.push(child.id.clone());

            self.beads.sync()?;
            *spawn_budget = spawn_budget.saturating_sub(1);

            if self.should_inline_child(&candidate) {
                let closed = self.run_inline_child(&child)?;
                if closed {
                    self.beads
                        .close_task(&child.id, "agent-loop: inline child completed")?;
                    closed_children.push(child.id.clone());
                } else {
                    self.beads.add_notes(
                        &child.id,
                        "Inline execution incomplete; child left in backlog",
                    )?;
                }
                self.beads.sync()?;
            }
        }
        Ok(())
    }

    fn should_inline_child(&self, candidate: &SpawnTaskCandidate) -> bool {
        if !candidate.can_inline {
            return false;
        }
        if candidate.estimated_minutes > self.config.small_child_threshold_minutes {
            return false;
        }

        matches!(
            candidate.kind,
            SpawnTaskKind::Decomposition
                | SpawnTaskKind::Performance
                | SpawnTaskKind::Quality
                | SpawnTaskKind::Tests
        )
    }

    fn run_inline_child(&self, child: &Task) -> Result<bool> {
        let started_at = Instant::now();
        let timeout = self.config.child_timeout();

        if started_at.elapsed() >= timeout {
            return Ok(false);
        }

        let timed_out = |started: Instant| started.elapsed() >= timeout;

        let storm = self.llm.product_storm(child)?;
        if timed_out(started_at) {
            return Ok(false);
        }
        let spec = self.llm.spec_first(child, &storm)?;
        if timed_out(started_at) {
            return Ok(false);
        }
        self.llm.run_red_phase(child, &spec, 1)?;
        if timed_out(started_at) {
            return Ok(false);
        }
        self.llm.run_green_phase(child, &spec, 1)?;
        if timed_out(started_at) {
            return Ok(false);
        }
        self.llm.run_refactor_phase(child, &spec, 1)?;
        if timed_out(started_at) {
            return Ok(false);
        }
        let gates = self.gates.run_core_gates(child, &spec)?;
        if timed_out(started_at) {
            return Ok(false);
        }
        let review = self.llm.review(child, &spec, 1)?;

        Ok(matches!(
            self.evaluator
                .decide_child(&gates, &review, &self.config.non_blocking_gates),
            DoneDecision::Done
        ))
    }

    fn should_close_as_decomposition_handoff(
        &self,
        task: &Task,
        gates: &GateReport,
        open_children: &[Task],
    ) -> bool {
        if !gates.all_passed_with_non_blocking(&self.config.non_blocking_gates)
            || open_children.is_empty()
        {
            return false;
        }

        let title = task.title.to_lowercase();
        task.labels
            .iter()
            .any(|label| label == "loop:kind:decomposition")
            || title.contains("decomposition")
            || title.contains("decompose")
            || title.contains("декомпози")
            || title.contains("аудит")
            || title.contains("инвентаризац")
    }

    fn needs_human_reason(
        &self,
        gates: &GateReport,
        review: &ReviewOutput,
        has_open_blocking_children: bool,
        timed_out: bool,
    ) -> String {
        if timed_out {
            return "timeout exceeded".to_string();
        }
        let blocking_failures = gates.blocking_failures(&self.config.non_blocking_gates);
        if !blocking_failures.is_empty() {
            let failed = blocking_failures
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(",");
            return format!(
                "quality gates failed [{}] ({})",
                failed,
                Self::format_gate_statuses(gates)
            );
        }
        if has_open_blocking_children {
            return "open blocking child tasks".to_string();
        }
        if let Some(first) = review.missing_items.first() {
            return format!("review missing item: {first}");
        }
        if review.coverage_score < self.evaluator.config.coverage_threshold {
            return format!(
                "review coverage below threshold (coverage={}, threshold={})",
                review.coverage_score, self.evaluator.config.coverage_threshold
            );
        }

        "parent iteration limit reached".to_string()
    }
}
