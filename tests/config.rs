use clap::Parser;

use agent_loop::config::{
    Cli, Commands, DEFAULT_MODEL, resolve_openai_api_key, resolve_openai_base_url,
};

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
        max_iterations: 3,
        timeout_minutes: 45,
        spawn_cap: 5,
        child_inline_timeout_minutes: 10,
        small_child_threshold_minutes: 20,
        model: Some("env-model".to_string()),
        hindsight_bank: "agent-loop".to_string(),
        dry_run: false,
    };

    assert_eq!(env_override.resolved_model(), "env-model");
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
