use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl CommandOutput {
    pub fn success(&self) -> bool {
        self.exit_code == 0
    }
}

pub trait ShellCommandRunner: Send + Sync {
    fn run(&self, program: &str, args: &[&str]) -> Result<CommandOutput>;
    fn run_with_input(&self, program: &str, args: &[&str], stdin: &str) -> Result<CommandOutput>;
}

#[derive(Debug, Clone, Default)]
pub struct ProcessShell;

impl ProcessShell {
    fn decode(output: std::process::Output) -> CommandOutput {
        CommandOutput {
            exit_code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        }
    }
}

impl ShellCommandRunner for ProcessShell {
    fn run(&self, program: &str, args: &[&str]) -> Result<CommandOutput> {
        let output = Command::new(program)
            .args(args)
            .output()
            .with_context(|| format!("failed to run command: {program} {}", args.join(" ")))?;
        Ok(Self::decode(output))
    }

    fn run_with_input(&self, program: &str, args: &[&str], stdin: &str) -> Result<CommandOutput> {
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("failed to spawn command: {program} {}", args.join(" ")))?;

        if let Some(mut pipe) = child.stdin.take() {
            use std::io::Write;
            pipe.write_all(stdin.as_bytes())
                .context("failed to write stdin to child process")?;
        } else {
            bail!("failed to open stdin for child process");
        }

        let output = child
            .wait_with_output()
            .context("failed waiting for child process output")?;
        Ok(Self::decode(output))
    }
}
