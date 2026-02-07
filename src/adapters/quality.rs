use anyhow::Result;

use crate::adapters::shell::ShellCommandRunner;
use crate::contracts::{GateReport, SpecOutput, Task};
use crate::loop_runner::QualityGateRunner;

pub struct CargoQualityGates {
    shell: Box<dyn ShellCommandRunner>,
}

impl CargoQualityGates {
    pub fn new(shell: Box<dyn ShellCommandRunner>) -> Self {
        Self { shell }
    }

    fn is_ok(&self, step: &str, args: &[&str]) -> Result<bool> {
        eprintln!("[agent-loop] quality gate: {step}");
        let out = self.shell.run("cargo", args)?;
        eprintln!(
            "[agent-loop] quality gate result: {step} => {}",
            if out.success() { "ok" } else { "failed" }
        );
        Ok(out.success())
    }
}

impl QualityGateRunner for CargoQualityGates {
    fn run_core_gates(&self, _task: &Task, spec: &SpecOutput) -> Result<GateReport> {
        let fmt_ok = self.is_ok("cargo fmt -- --check", &["fmt", "--", "--check"])?;
        let clippy_ok = self.is_ok(
            "cargo clippy -- -D warnings",
            &["clippy", "--", "-D", "warnings"],
        )?;
        let tests_ok = self.is_ok("cargo test", &["test"])?;
        let perf_ok = if spec.requires_perf_gate {
            self.is_ok("cargo test --release", &["test", "--release"])?
        } else {
            true
        };

        Ok(GateReport {
            fmt_ok,
            clippy_ok,
            tests_ok,
            perf_ok,
        })
    }
}
