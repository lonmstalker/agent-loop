use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    #[default]
    Open,
    InProgress,
    Blocked,
    Deferred,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub status: TaskStatus,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub parent_id: Option<String>,
}

impl Task {
    pub fn is_open(&self) -> bool {
        !matches!(self.status, TaskStatus::Closed)
    }

    pub fn is_blocking_child(&self) -> bool {
        self.labels.iter().any(|x| x == "loop:blocking")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpawnTaskKind {
    Decomposition,
    Backlog,
    Performance,
    Quality,
    Tests,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpawnTaskCandidate {
    pub title: String,
    pub description: String,
    pub kind: SpawnTaskKind,
    pub blocking: bool,
    pub estimated_minutes: u32,
    pub can_inline: bool,
    #[serde(default)]
    pub acceptance: Vec<String>,
    pub priority: u8,
    #[serde(default)]
    pub labels: Vec<String>,
}

impl SpawnTaskCandidate {
    pub fn normalized_title(&self) -> String {
        self.title.to_lowercase().trim().to_owned()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ProductStormOutput {
    pub summary: String,
    #[serde(default)]
    pub spawn_candidates: Vec<SpawnTaskCandidate>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpecOutput {
    #[serde(default)]
    pub acceptance_criteria: Vec<String>,
    #[serde(default)]
    pub non_goals: Vec<String>,
    #[serde(default)]
    pub requires_perf_gate: bool,
}

impl Default for SpecOutput {
    fn default() -> Self {
        Self {
            acceptance_criteria: vec!["Task requirements are satisfied".to_string()],
            non_goals: Vec::new(),
            requires_perf_gate: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewStatus {
    Done,
    Rework,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviewOutput {
    pub status: ReviewStatus,
    pub coverage_score: f64,
    #[serde(default)]
    pub missing_items: Vec<String>,
    #[serde(default)]
    pub spawn_candidates: Vec<SpawnTaskCandidate>,
    #[serde(default)]
    pub risk_flags: Vec<String>,
}

impl Default for ReviewOutput {
    fn default() -> Self {
        Self {
            status: ReviewStatus::Done,
            coverage_score: 1.0,
            missing_items: Vec::new(),
            spawn_candidates: Vec::new(),
            risk_flags: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateReport {
    pub fmt_ok: bool,
    pub clippy_ok: bool,
    pub tests_ok: bool,
    pub perf_ok: bool,
}

impl GateReport {
    pub fn all_passed(&self) -> bool {
        self.fmt_ok && self.clippy_ok && self.tests_ok && self.perf_ok
    }

    pub fn all_green() -> Self {
        Self {
            fmt_ok: true,
            clippy_ok: true,
            tests_ok: true,
            perf_ok: true,
        }
    }
}

impl Default for GateReport {
    fn default() -> Self {
        Self::all_green()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DoneDecision {
    Done,
    Rework,
    NeedsHuman,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunOutcome {
    NoReadyWork,
    Done {
        task_id: String,
        created_children: Vec<String>,
        closed_children: Vec<String>,
    },
    NeedsHuman {
        task_id: String,
        reason: String,
        created_children: Vec<String>,
        closed_children: Vec<String>,
    },
}
