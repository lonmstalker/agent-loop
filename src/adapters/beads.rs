use anyhow::{Context, Result, anyhow, bail};
use serde_json::Value;

use crate::adapters::shell::ShellCommandRunner;
use crate::contracts::{SpawnTaskCandidate, SpawnTaskKind, Task, TaskStatus};
use crate::loop_runner::BeadsClient;

pub struct BeadsCli {
    shell: Box<dyn ShellCommandRunner>,
    default_prefix: String,
}

impl BeadsCli {
    pub fn new(shell: Box<dyn ShellCommandRunner>, default_prefix: impl Into<String>) -> Self {
        Self {
            shell,
            default_prefix: default_prefix.into(),
        }
    }

    fn run_checked(&self, args: &[&str]) -> Result<String> {
        let out = self.shell.run("bd", args)?;
        if !out.success() {
            bail!("bd {} failed: {}", args.join(" "), out.stderr.trim());
        }
        Ok(out.stdout)
    }

    fn run_json(&self, args: &[&str]) -> Result<Value> {
        let stdout = self.run_checked(args)?;
        let trimmed = stdout.trim();
        if trimmed.is_empty() {
            return Ok(Value::Null);
        }
        let value = serde_json::from_str(trimmed)
            .with_context(|| format!("failed to parse JSON for: bd {}", args.join(" ")))?;
        Ok(value)
    }

    fn map_status(raw: &str) -> TaskStatus {
        match raw {
            "open" => TaskStatus::Open,
            "in_progress" => TaskStatus::InProgress,
            "blocked" => TaskStatus::Blocked,
            "deferred" => TaskStatus::Deferred,
            "closed" => TaskStatus::Closed,
            _ => TaskStatus::Open,
        }
    }

    fn parse_task(value: &Value) -> Option<Task> {
        let id = value
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_default();

        if id.is_empty() {
            return None;
        }

        let title = value
            .get("title")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| id.clone());

        let description = value
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_default();

        let status = value
            .get("status")
            .and_then(Value::as_str)
            .map(Self::map_status)
            .unwrap_or(TaskStatus::Open);

        let labels = value
            .get("labels")
            .and_then(Value::as_array)
            .map(|xs| {
                xs.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let parent_id = value
            .get("parent")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| {
                value
                    .get("parent_id")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            });

        Some(Task {
            id,
            title,
            description,
            status,
            labels,
            parent_id,
        })
    }

    fn parse_tasks(value: &Value) -> Vec<Task> {
        match value {
            Value::Array(items) => items.iter().filter_map(Self::parse_task).collect(),
            Value::Object(map) => {
                if let Some(items) = map.get("issues").and_then(Value::as_array) {
                    return items.iter().filter_map(Self::parse_task).collect();
                }
                Self::parse_task(value).into_iter().collect()
            }
            _ => Vec::new(),
        }
    }

    fn show_task(&self, task_id: &str) -> Result<Task> {
        let value = self.run_json(&["show", task_id, "--json"])?;
        let mut tasks = Self::parse_tasks(&value);
        tasks
            .pop()
            .ok_or_else(|| anyhow!("bd show returned no task for {task_id}"))
    }

    fn candidate_labels(candidate: &SpawnTaskCandidate) -> Vec<String> {
        let mut labels = candidate.labels.clone();
        labels.push(format!(
            "loop:kind:{}",
            match candidate.kind {
                SpawnTaskKind::Decomposition => "decomposition",
                SpawnTaskKind::Backlog => "backlog",
                SpawnTaskKind::Performance => "performance",
                SpawnTaskKind::Quality => "quality",
                SpawnTaskKind::Tests => "tests",
            }
        ));
        labels.push(if candidate.blocking {
            "loop:blocking".to_string()
        } else {
            "loop:followup".to_string()
        });
        labels
    }

    fn list_open_children(&self, parent_id: &str) -> Result<Vec<Task>> {
        let value = self.run_json(&["children", parent_id, "--json"])?;
        Ok(Self::parse_tasks(&value)
            .into_iter()
            .filter(|task| task.is_open())
            .collect())
    }
}

impl BeadsClient for BeadsCli {
    fn ensure_initialized(&self) -> Result<()> {
        let check = self.shell.run("bd", &["where"])?;
        if check.success() {
            return Ok(());
        }
        let _ = self.run_checked(&["init", "--prefix", &self.default_prefix])?;
        Ok(())
    }

    fn create_bootstrap_parent_task(&self, goal: &str) -> Result<Task> {
        let goal = goal.trim();
        if goal.is_empty() {
            bail!("bootstrap goal is empty");
        }

        let title = format!("[Bootstrap] Product storm: {goal}");
        let description =
            format!("Auto-created bootstrap task for agent-loop.\nProduct storm goal: {goal}");

        let id = self
            .run_checked(&[
                "create",
                "--title",
                title.as_str(),
                "--description",
                description.as_str(),
                "--type",
                "feature",
                "--priority",
                "2",
                "--labels",
                "loop:bootstrap",
                "--silent",
            ])?
            .trim()
            .to_string();

        if id.is_empty() {
            bail!("bd create did not return issue id for bootstrap task");
        }

        let _ = self.run_checked(&["update", &id, "--claim"])?;
        self.show_task(&id)
    }

    fn claim_parent_task(&self, preferred_task: Option<&str>) -> Result<Option<Task>> {
        let task = if let Some(task_id) = preferred_task {
            self.show_task(task_id)?
        } else {
            let value = self.run_json(&["ready", "--json", "--limit", "1"])?;
            let mut tasks = Self::parse_tasks(&value);
            let Some(task) = tasks.pop() else {
                return Ok(None);
            };
            self.show_task(&task.id)?
        };

        if !task.is_open() {
            return Ok(None);
        }

        let _ = self.run_checked(&["update", &task.id, "--claim"])?;
        Ok(Some(task))
    }

    fn sync(&self) -> Result<()> {
        let _ = self.run_checked(&["sync", "--flush-only"])?;
        Ok(())
    }

    fn search_duplicates(
        &self,
        parent_id: &str,
        candidate: &SpawnTaskCandidate,
    ) -> Result<Vec<Task>> {
        let target = candidate.normalized_title();
        let from_children = self
            .list_open_children(parent_id)?
            .into_iter()
            .filter(|task| task.title.to_lowercase().trim() == target)
            .collect::<Vec<_>>();

        let from_search = self
            .run_json(&[
                "search",
                "--query",
                candidate.title.as_str(),
                "--status",
                "open",
                "--limit",
                "20",
                "--json",
            ])
            .map(|value| {
                Self::parse_tasks(&value)
                    .into_iter()
                    .filter(|task| {
                        task.parent_id.as_deref() == Some(parent_id)
                            && task.title.to_lowercase().trim() == target
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let mut merged = from_children;
        merged.extend(from_search);
        merged.sort_by(|a, b| a.id.cmp(&b.id));
        merged.dedup_by(|a, b| a.id == b.id);
        Ok(merged)
    }

    fn create_child_task(&self, parent_id: &str, candidate: &SpawnTaskCandidate) -> Result<Task> {
        let labels = Self::candidate_labels(candidate).join(",");
        let priority = candidate.priority.min(4).to_string();
        let args = vec![
            "create",
            "--title",
            candidate.title.as_str(),
            "--description",
            candidate.description.as_str(),
            "--type",
            "task",
            "--parent",
            parent_id,
            "--priority",
            priority.as_str(),
            "--labels",
            labels.as_str(),
            "--silent",
        ];

        let id = self.run_checked(&args)?.trim().to_string();
        if id.is_empty() {
            bail!("bd create did not return issue id");
        }

        self.show_task(&id)
    }

    fn add_blocking_dependency(&self, child_id: &str, parent_id: &str) -> Result<()> {
        let _ = self.run_checked(&["dep", child_id, "--blocks", parent_id])?;
        Ok(())
    }

    fn add_notes(&self, task_id: &str, notes: &str) -> Result<()> {
        let _ = self.run_checked(&["update", task_id, "--notes", notes])?;
        Ok(())
    }
}
