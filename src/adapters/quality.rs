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

    fn is_ok(&self, args: &[&str]) -> Result<bool> {
        let out = self.shell.run("cargo", args)?;
        Ok(out.success())
    }
}

impl QualityGateRunner for CargoQualityGates {
    fn run_core_gates(&self, _task: &Task, spec: &SpecOutput) -> Result<GateReport> {
        let fmt_ok = self.is_ok(&["fmt", "--", "--check"])?;
        let clippy_ok = self.is_ok(&["clippy", "--", "-D", "warnings"])?;
        let tests_ok = self.is_ok(&["test"])?;
        let perf_ok = if spec.requires_perf_gate {
            self.is_ok(&["test", "--release"])?
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
