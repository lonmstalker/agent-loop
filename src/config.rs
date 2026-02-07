use clap::{Args, Parser, Subcommand, ValueEnum};

pub const DEFAULT_MODEL: &str = "gpt-5.3-codex";
pub const DEFAULT_MAX_ITERATIONS: u32 = 3;
pub const DEFAULT_TIMEOUT_MINUTES: u64 = 45;
pub const DEFAULT_SPAWN_CAP: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum LoopProfile {
    Discovery,
    Delivery,
    Hardening,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum RetainMode {
    Sync,
    Async,
    Off,
}

#[derive(Debug, Parser)]
#[command(name = "agent-loop")]
#[command(about = "Single-run agent loop with beads/hindsight/memory-bank orchestration")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    Run(RunCommand),
}

#[derive(Debug, Clone, Args)]
pub struct RunCommand {
    #[arg(long)]
    pub task: Option<String>,
    #[arg(long, conflicts_with = "task")]
    pub bootstrap: Option<String>,
    #[arg(long, default_value_t = DEFAULT_MAX_ITERATIONS)]
    pub max_iterations: u32,
    #[arg(long, default_value_t = DEFAULT_TIMEOUT_MINUTES)]
    pub timeout_minutes: u64,
    #[arg(long, default_value_t = DEFAULT_SPAWN_CAP)]
    pub spawn_cap: usize,
    #[arg(long, env = "AGENT_LOOP_MODEL")]
    pub model: Option<String>,
    #[arg(long, default_value = "agent-loop")]
    pub hindsight_bank: String,
    #[arg(long, value_enum, default_value_t = LoopProfile::Delivery)]
    pub profile: LoopProfile,
    #[arg(long, value_enum, default_value_t = RetainMode::Sync)]
    pub retain_mode: RetainMode,
    #[arg(long, default_value_t = false)]
    pub json_events: bool,
    #[arg(long, default_value_t = false)]
    pub dry_run: bool,
}

impl RunCommand {
    pub fn resolved_model(&self) -> String {
        self.model
            .clone()
            .filter(|x| !x.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_MODEL.to_string())
    }

    pub fn resolved_bootstrap_goal(&self) -> Option<String> {
        self.bootstrap
            .clone()
            .map(|x| x.trim().to_string())
            .filter(|x| !x.is_empty())
    }
}

pub fn resolve_openai_api_key(
    openai_api_key: Option<String>,
    vibeproxy_api_key: Option<String>,
) -> Option<String> {
    openai_api_key
        .filter(|x| !x.trim().is_empty())
        .or_else(|| vibeproxy_api_key.filter(|x| !x.trim().is_empty()))
}

pub fn resolve_openai_base_url(
    openai_base_url: Option<String>,
    vibeproxy_api_key: Option<String>,
) -> Option<String> {
    openai_base_url
        .filter(|x| !x.trim().is_empty())
        .or_else(|| {
            vibeproxy_api_key
                .filter(|x| !x.trim().is_empty())
                .map(|_| "http://127.0.0.1:8318".to_string())
        })
}
