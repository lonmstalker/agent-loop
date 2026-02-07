use clap::Parser;

use agent_loop::config::{
    Cli, Commands, DEFAULT_MODEL, LoopProfile, RetainMode, resolve_openai_api_key,
    resolve_openai_base_url,
};
use agent_loop::contracts::GateName;
use agent_loop::loop_runner::RunConfig;

#[test]
fn uses_default_model_gpt_5_3_codex_when_not_overridden() {
    let cli = Cli::parse_from(["agent-loop", "run"]);
    let Commands::Run(run) = cli.command;

    assert_eq!(run.resolved_model(), DEFAULT_MODEL);
}

#[test]
fn model_override_via_flag_or_env_has_priority_over_default() {
    let cli = Cli::parse_from(["agent-loop", "run", "--model", "custom-model"]);
    let Commands::Run(run) = cli.command;

    assert_eq!(run.resolved_model(), "custom-model");

    let env_override = agent_loop::config::RunCommand {
        task: None,
        bootstrap: None,
        max_iterations: 3,
        timeout_minutes: 45,
        spawn_cap: 5,
        child_inline_timeout_minutes: 10,
        small_child_threshold_minutes: 20,
        model: Some("env-model".to_string()),
        hindsight_bank: "agent-loop".to_string(),
        driver: agent_loop::config::ExecutionDriver::Agent,
        profile: LoopProfile::Delivery,
        non_blocking_gates: vec![],
        retain_mode: RetainMode::Sync,
        json_events: false,
        dry_run: false,
    };

    assert_eq!(env_override.resolved_model(), "env-model");
}

#[test]
fn parses_bootstrap_goal_for_taskless_product_storm() {
    let cli = Cli::parse_from(["agent-loop", "run", "--bootstrap", "Новый продукт"]);
    let Commands::Run(run) = cli.command;

    assert_eq!(run.task, None);
    assert_eq!(
        run.resolved_bootstrap_goal().as_deref(),
        Some("Новый продукт")
    );
}

#[test]
fn parses_profile_and_non_blocking_gates() {
    let cli = Cli::parse_from([
        "agent-loop",
        "run",
        "--profile",
        "discovery",
        "--non-blocking-gates",
        "clippy,fmt",
        "--retain-mode",
        "async",
        "--json-events",
    ]);
    let Commands::Run(run) = cli.command;

    assert!(matches!(run.profile, LoopProfile::Discovery));
    assert!(matches!(run.retain_mode, RetainMode::Async));
    assert!(run.json_events);
    let gates = run.resolved_non_blocking_gates();
    assert!(gates.contains(&GateName::Clippy));
    assert!(gates.contains(&GateName::Fmt));
}

#[test]
fn discovery_profile_adds_default_non_blocking_clippy() {
    let cli = Cli::parse_from(["agent-loop", "run", "--profile", "discovery"]);
    let Commands::Run(run) = cli.command;
    let cfg: RunConfig = run.into();

    assert!(cfg.non_blocking_gates.contains(&GateName::Clippy));
}

#[test]
fn defaults_to_agent_driver() {
    let cli = Cli::parse_from(["agent-loop", "run"]);
    let Commands::Run(run) = cli.command;
    let cfg: RunConfig = run.into();

    assert!(matches!(
        cfg.driver,
        agent_loop::config::ExecutionDriver::Agent
    ));
}

#[test]
fn openai_api_key_has_priority_over_vibeproxy_key() {
    let resolved = resolve_openai_api_key(
        Some("openai-key".to_string()),
        Some("vibeproxy-key".to_string()),
    );
    assert_eq!(resolved.as_deref(), Some("openai-key"));
}

#[test]
fn falls_back_to_vibeproxy_key_when_openai_key_missing() {
    let resolved = resolve_openai_api_key(None, Some("vibeproxy-key".to_string()));
    assert_eq!(resolved.as_deref(), Some("vibeproxy-key"));
}

#[test]
fn openai_base_url_has_priority_over_vibeproxy_localhost_fallback() {
    let resolved = resolve_openai_base_url(
        Some("https://api.example.com".to_string()),
        Some("vibeproxy-key".to_string()),
    );
    assert_eq!(resolved.as_deref(), Some("https://api.example.com"));
}

#[test]
fn falls_back_to_local_vibeproxy_base_url_when_key_present() {
    let resolved = resolve_openai_base_url(None, Some("vibeproxy-key".to_string()));
    assert_eq!(resolved.as_deref(), Some("http://127.0.0.1:8318"));
}
