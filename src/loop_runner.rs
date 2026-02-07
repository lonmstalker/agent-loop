use std::time::Duration;

use anyhow::Result;
use serde_json::json;

use crate::config::{LoopProfile, RetainMode, RunCommand};
use crate::contracts::{
    ProductStormOutput, ReviewOutput, RunOutcome, SpawnTaskCandidate, SpecOutput, Task,
};

#[derive(Debug, Clone)]
pub struct RunConfig {
    pub task_id: Option<String>,
    pub bootstrap_goal: Option<String>,
    pub max_iterations: u32,
    pub timeout_minutes: u64,
    pub spawn_cap: usize,
    pub model: String,
    pub hindsight_bank: String,
    pub profile: LoopProfile,
    pub retain_mode: RetainMode,
    pub json_events: bool,
    pub dry_run: bool,
}

impl From<RunCommand> for RunConfig {
    fn from(value: RunCommand) -> Self {
        Self {
            task_id: value.task.clone(),
            bootstrap_goal: value.resolved_bootstrap_goal(),
            max_iterations: value.max_iterations,
            timeout_minutes: value.timeout_minutes,
            spawn_cap: value.spawn_cap,
            model: value.resolved_model(),
            hindsight_bank: value.hindsight_bank,
            profile: value.profile,
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
}

pub trait BeadsClient: Send + Sync {
    fn ensure_initialized(&self) -> Result<()>;
    fn create_bootstrap_parent_task(&self, goal: &str) -> Result<Task>;
    fn claim_parent_task(&self, preferred_task: Option<&str>, claim: bool) -> Result<Option<Task>>;
    fn sync(&self) -> Result<()>;
    fn search_duplicates(
        &self,
        parent_id: &str,
        candidate: &SpawnTaskCandidate,
    ) -> Result<Vec<Task>>;
    fn create_child_task(&self, parent_id: &str, candidate: &SpawnTaskCandidate) -> Result<Task>;
    fn add_blocking_dependency(&self, child_id: &str, parent_id: &str) -> Result<()>;
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

pub struct AgentLoop {
    pub config: RunConfig,
    pub beads: Box<dyn BeadsClient>,
    pub hindsight: Box<dyn HindsightClient>,
    pub memory_bank: Box<dyn MemoryBankClient>,
    pub llm: Box<dyn LlmClient>,
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

    fn build_agent_commands(&self, task: &Task, spec: &SpecOutput) -> Vec<String> {
        let mut commands = Vec::new();
        let acceptance = if spec.acceptance_criteria.is_empty() {
            "- Validate task behavior against expected outcomes".to_string()
        } else {
            spec.acceptance_criteria
                .iter()
                .map(|x| format!("- {x}"))
                .collect::<Vec<_>>()
                .join("\n")
        };

        commands.push(format!(
            "Task: {} ({})\nAcceptance criteria:\n{}",
            task.id, task.title, acceptance
        ));

        for iteration in 1..=self.config.max_iterations {
            commands.push(format!(
                "Iteration {iteration}/{max}: RED -> write/update failing tests for one acceptance criterion",
                max = self.config.max_iterations
            ));
            commands.push("Run: cargo test -- --nocapture".to_string());
            commands
                .push("GREEN -> implement minimal code to pass the new failing tests".to_string());
            commands.push("Run: cargo test".to_string());
            commands.push("REFACTOR -> clean code while preserving behavior".to_string());
            commands.push("Run: cargo fmt -- --check".to_string());
            commands.push("Run: cargo clippy -- -D warnings".to_string());
            commands.push("Run: cargo test".to_string());
        }

        commands.push(format!(
            "After completing commands, re-run: agent-loop run --task {} --hindsight-bank {}",
            task.id, self.config.hindsight_bank
        ));

        commands
    }

    fn format_agent_note(commands: &[String]) -> String {
        let mut out = String::from("agent-loop generated iterative command plan:\n");
        for (idx, cmd) in commands.iter().enumerate() {
            out.push_str(&format!("{}. {}\n", idx + 1, cmd));
        }
        out
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
                .claim_parent_task(self.config.task_id.as_deref(), !self.config.dry_run)?
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
        let closed_children = Vec::new();
        if parent.parent_id.is_none() {
            self.process_spawn_candidates(
                &parent,
                storm.spawn_candidates,
                &mut spawn_budget,
                &mut created_children,
            )?;
            self.step("spawn candidates processed");
        }

        let commands = self.build_agent_commands(&parent, &spec);
        if !self.config.dry_run {
            self.beads
                .add_notes(&parent.id, &Self::format_agent_note(&commands))?;
            self.beads.sync()?;
        }
        self.retain_summary_best_effort(
            &parent,
            "Agent command plan generated; waiting for agent execution feedback".to_string(),
        );
        Ok(RunOutcome::AgentActionRequired {
            task_id: parent.id,
            commands,
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
            if candidate.blocking {
                self.beads.add_blocking_dependency(&child.id, &parent.id)?;
            }
            self.beads.sync()?;
            *spawn_budget = spawn_budget.saturating_sub(1);
            created_children.push(child.id);
        }
        Ok(())
    }
}
