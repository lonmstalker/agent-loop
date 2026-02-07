use anyhow::{Result, bail};
use serde_json::Value;

use crate::adapters::shell::ShellCommandRunner;
use crate::loop_runner::HindsightClient;

pub struct HindsightCli {
    shell: Box<dyn ShellCommandRunner>,
}

impl HindsightCli {
    pub fn new(shell: Box<dyn ShellCommandRunner>) -> Self {
        Self { shell }
    }

    fn run_checked(&self, args: &[&str]) -> Result<String> {
        let out = self.shell.run("hindsight", args)?;
        if !out.success() {
            bail!("hindsight {} failed: {}", args.join(" "), out.stderr.trim());
        }
        Ok(out.stdout)
    }

    fn run_json(&self, args: &[&str]) -> Result<Value> {
        let stdout = self.run_checked(args)?;
        let trimmed = stdout.trim();
        if trimmed.is_empty() {
            return Ok(Value::Null);
        }
        Ok(serde_json::from_str(trimmed)?)
    }
}

impl HindsightClient for HindsightCli {
    fn ensure_available(&self) -> Result<()> {
        let _ = self.run_checked(&["bank", "list", "--output", "json"])?;
        Ok(())
    }

    fn ensure_project_bank(&self, bank: &str) -> Result<()> {
        let banks = self.run_json(&["bank", "list", "--output", "json"])?;
        let exists = banks
            .as_array()
            .map(|xs| {
                xs.iter().any(|x| {
                    x.get("bank_id")
                        .and_then(Value::as_str)
                        .map(|id| id == bank)
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false);

        if !exists {
            let _ = self.run_checked(&[
                "memory",
                "retain",
                bank,
                "Project bank initialized for agent-loop",
                "--context",
                "bootstrap",
                "--doc-id",
                "bootstrap",
            ])?;
        }

        Ok(())
    }

    fn recall_preferences(&self) -> Result<()> {
        let _ = self.shell.run(
            "hindsight",
            &[
                "memory",
                "recall",
                "user-preferences",
                "language preferences and response style",
                "--budget",
                "low",
                "--max-tokens",
                "512",
                "--output",
                "json",
            ],
        )?;
        Ok(())
    }

    fn recall_task_context(&self, bank: &str, query: &str) -> Result<()> {
        let _ = self.shell.run(
            "hindsight",
            &[
                "memory",
                "recall",
                bank,
                query,
                "--budget",
                "low",
                "--max-tokens",
                "512",
                "--output",
                "json",
            ],
        )?;
        Ok(())
    }

    fn retain_summary(
        &self,
        bank: &str,
        doc_id: &str,
        context: &str,
        content: &str,
        async_mode: bool,
    ) -> Result<()> {
        let mut args = vec![
            "memory",
            "retain",
            bank,
            content,
            "--context",
            context,
            "--doc-id",
            doc_id,
        ];
        if async_mode {
            args.push("--async");
        }
        let _ = self.run_checked(&args)?;
        Ok(())
    }
}
