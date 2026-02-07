use std::env;

use anyhow::Result;
use clap::Parser;

use agent_loop::adapters::beads::BeadsCli;
use agent_loop::adapters::hindsight::HindsightCli;
use agent_loop::adapters::memory_bank::FileMemoryBank;
use agent_loop::adapters::openai::OpenAiResponsesClient;
use agent_loop::adapters::shell::ProcessShell;
use agent_loop::config::{Cli, Commands, resolve_openai_api_key, resolve_openai_base_url};
use agent_loop::loop_runner::{AgentLoop, RunConfig};

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Run(run_cmd) => {
            let config: RunConfig = run_cmd.into();
            let model = config.model.clone();
            let openai_api_key = env::var("OPENAI_API_KEY").ok();
            let vibeproxy_api_key = env::var("VIBEPROXY_API_KEY").ok();

            let engine = AgentLoop {
                config,
                beads: Box::new(BeadsCli::new(Box::new(ProcessShell), "agent-loop")),
                hindsight: Box::new(HindsightCli::new(Box::new(ProcessShell))),
                memory_bank: Box::new(FileMemoryBank::new(".memory-bank")),
                llm: Box::new(OpenAiResponsesClient::new(
                    model,
                    resolve_openai_base_url(
                        env::var("OPENAI_BASE_URL").ok(),
                        vibeproxy_api_key.clone(),
                    ),
                    resolve_openai_api_key(openai_api_key, vibeproxy_api_key),
                )),
            };

            let outcome = engine.run_once()?;
            println!("{outcome:?}");
        }
    }

    Ok(())
}
