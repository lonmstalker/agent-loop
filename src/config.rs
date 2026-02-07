use clap::{Args, Parser, Subcommand};

pub const DEFAULT_MODEL: &str = "gpt-5.3-codex";
pub const DEFAULT_MAX_ITERATIONS: u32 = 3;
pub const DEFAULT_TIMEOUT_MINUTES: u64 = 45;
pub const DEFAULT_SPAWN_CAP: usize = 5;
pub const DEFAULT_CHILD_INLINE_TIMEOUT_MINUTES: u64 = 10;
pub const DEFAULT_SMALL_CHILD_MINUTES: u32 = 20;

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
    #[arg(long, default_value_t = DEFAULT_MAX_ITERATIONS)]
    pub max_iterations: u32,
    #[arg(long, default_value_t = DEFAULT_TIMEOUT_MINUTES)]
    pub timeout_minutes: u64,
    #[arg(long, default_value_t = DEFAULT_SPAWN_CAP)]
    pub spawn_cap: usize,
    #[arg(long, default_value_t = DEFAULT_CHILD_INLINE_TIMEOUT_MINUTES)]
    pub child_inline_timeout_minutes: u64,
    #[arg(long, default_value_t = DEFAULT_SMALL_CHILD_MINUTES)]
    pub small_child_threshold_minutes: u32,
    #[arg(long, env = "AGENT_LOOP_MODEL")]
    pub model: Option<String>,
    #[arg(long, default_value = "agent-loop")]
    pub hindsight_bank: String,
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
