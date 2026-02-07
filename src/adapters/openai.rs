use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use reqwest::blocking::Client;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::contracts::{
    ProductStormOutput, ReviewOutput, ReviewStatus, SpawnTaskCandidate, SpawnTaskKind, SpecOutput,
    Task,
};
use crate::loop_runner::LlmClient;

#[derive(Debug, Clone)]
pub struct OpenAiResponsesClient {
    model: String,
    base_url: String,
    api_key: Option<String>,
    http: Client,
}

impl OpenAiResponsesClient {
    pub fn new(model: String, base_url: Option<String>, api_key: Option<String>) -> Self {
        let http = Client::builder()
            .timeout(Duration::from_secs(20))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_else(|_| Client::new());

        Self {
            model,
            base_url: base_url.unwrap_or_else(|| "https://api.openai.com".to_string()),
            api_key,
            http,
        }
    }

    fn api_enabled(&self) -> bool {
        self.api_key.as_ref().is_some_and(|x| !x.trim().is_empty())
    }

    fn model_candidates(&self) -> Vec<String> {
        let mut models = Vec::new();
        for model in [
            self.model.clone(),
            "gpt-5-codex".to_string(),
            "gpt-5".to_string(),
        ] {
            if model.trim().is_empty() {
                continue;
            }
            if !models.iter().any(|x| x == &model) {
                models.push(model);
            }
        }
        models
    }

    fn request_text_for_model(&self, model: &str, prompt: &str) -> Result<String> {
        let api_key = match &self.api_key {
            Some(value) if !value.trim().is_empty() => value,
            _ => anyhow::bail!("OPENAI_API_KEY is not set"),
        };

        let endpoint = format!("{}/v1/responses", self.base_url.trim_end_matches('/'));
        let body = json!({
            "model": model,
            "input": prompt,
            "text": {
                "format": {
                    "type": "json_object"
                }
            }
        });

        let resp = self
            .http
            .post(endpoint)
            .bearer_auth(api_key)
            .json(&body)
            .send()
            .context("failed to call OpenAI responses API")?;

        let status = resp.status();
        let raw_body = resp
            .text()
            .context("failed to read OpenAI responses API body")?;

        if !status.is_success() {
            let lower = raw_body.to_lowercase();
            let model_not_found = (status.as_u16() == 400 || status.as_u16() == 404)
                && lower.contains("model")
                && (lower.contains("not found")
                    || lower.contains("does not exist")
                    || lower.contains("unknown"));
            if model_not_found {
                return Err(anyhow!("MODEL_NOT_FOUND: {raw_body}"));
            }
            return Err(anyhow!(
                "OpenAI responses API returned status {}: {}",
                status,
                raw_body
            ));
        }

        let value: Value = serde_json::from_str(&raw_body)
            .context("failed to decode OpenAI responses API body")?;

        Self::extract_output_text(&value)
            .or_else(|| value.get("output").and_then(Self::extract_output_text))
            .ok_or_else(|| anyhow::anyhow!("unable to extract output text from responses API"))
    }

    fn request_text(&self, prompt: &str) -> Result<String> {
        let mut last_error = None;
        for model in self.model_candidates() {
            match self.request_text_for_model(&model, prompt) {
                Ok(text) => return Ok(text),
                Err(err) => {
                    let is_model_not_found =
                        err.to_string().to_lowercase().contains("model_not_found");
                    if is_model_not_found {
                        last_error = Some(err);
                        continue;
                    }
                    return Err(err);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow!("no model candidates available")))
    }

    fn request_json<T: DeserializeOwned>(&self, prompt: &str) -> Result<T> {
        let text = self.request_text(prompt)?;
        let normalized = Self::strip_fences(&text);
        serde_json::from_str::<T>(&normalized)
            .with_context(|| format!("failed to decode JSON from model output: {normalized}"))
    }

    fn extract_output_text(value: &Value) -> Option<String> {
        if let Some(text) = value.get("output_text").and_then(Value::as_str) {
            return Some(text.to_string());
        }

        match value {
            Value::Object(map) => {
                for key in ["text", "content"] {
                    if let Some(text) = map.get(key).and_then(Value::as_str) {
                        return Some(text.to_string());
                    }
                }
                for child in map.values() {
                    if let Some(text) = Self::extract_output_text(child) {
                        return Some(text);
                    }
                }
                None
            }
            Value::Array(items) => {
                for item in items {
                    if let Some(text) = Self::extract_output_text(item) {
                        return Some(text);
                    }
                }
                None
            }
            _ => None,
        }
    }

    fn strip_fences(text: &str) -> String {
        let trimmed = text.trim();
        if !trimmed.starts_with("```") {
            return trimmed.to_string();
        }

        let without_prefix = trimmed
            .strip_prefix("```json")
            .or_else(|| trimmed.strip_prefix("```"))
            .unwrap_or(trimmed)
            .trim();

        without_prefix
            .strip_suffix("```")
            .unwrap_or(without_prefix)
            .trim()
            .to_string()
    }

    fn fallback_storm(task: &Task) -> ProductStormOutput {
        let mut candidates = Vec::new();
        let lower = format!(
            "{} {}",
            task.title.to_lowercase(),
            task.description.to_lowercase()
        );
        if lower.contains("perf") || lower.contains("performance") {
            candidates.push(SpawnTaskCandidate {
                title: format!("[Perf] Baseline for {}", task.id),
                description: "Capture baseline and validate hot paths".to_string(),
                kind: SpawnTaskKind::Performance,
                blocking: false,
                estimated_minutes: 15,
                can_inline: true,
                acceptance: vec!["Baseline captured".to_string()],
                priority: 2,
                labels: vec!["auto-generated".to_string()],
            });
        }

        ProductStormOutput {
            summary: format!("Fallback storm for {}", task.title),
            spawn_candidates: candidates,
        }
    }

    fn fallback_spec(task: &Task) -> SpecOutput {
        SpecOutput {
            acceptance_criteria: vec![format!("{} is implemented and validated", task.title)],
            non_goals: Vec::new(),
            requires_perf_gate: task.title.to_lowercase().contains("perf"),
        }
    }

    fn fallback_review() -> ReviewOutput {
        ReviewOutput {
            status: ReviewStatus::Rework,
            coverage_score: 0.0,
            missing_items: vec![
                "LLM review unavailable, fallback review cannot mark task as done".to_string(),
            ],
            spawn_candidates: Vec::new(),
            risk_flags: vec!["fallback-review".to_string()],
        }
    }
}

impl LlmClient for OpenAiResponsesClient {
    fn product_storm(&self, task: &Task) -> Result<ProductStormOutput> {
        if !self.api_enabled() {
            return Ok(Self::fallback_storm(task));
        }

        let prompt = format!(
            "You are product-storm engine. Return JSON only with fields: summary:string, spawn_candidates:array. For each spawn candidate fields: title, description, kind(decomposition|backlog|performance|quality|tests), blocking(bool), estimated_minutes(number), can_inline(bool), acceptance(array string), priority(number 0..4), labels(array string). Task id: {}. Task title: {}. Task description: {}",
            task.id, task.title, task.description
        );

        self.request_json(&prompt)
            .or_else(|_| Ok(Self::fallback_storm(task)))
    }

    fn spec_first(&self, task: &Task, storm: &ProductStormOutput) -> Result<SpecOutput> {
        if !self.api_enabled() {
            return Ok(Self::fallback_spec(task));
        }

        let prompt = format!(
            "You are spec-first engine. Return JSON only with fields: acceptance_criteria(array string), non_goals(array string), requires_perf_gate(bool). Task title: {}. Task description: {}. Storm summary: {}",
            task.title, task.description, storm.summary
        );

        self.request_json(&prompt)
            .or_else(|_| Ok(Self::fallback_spec(task)))
    }

    fn run_red_phase(&self, _task: &Task, _spec: &SpecOutput, _iteration: u32) -> Result<()> {
        Ok(())
    }

    fn run_green_phase(&self, _task: &Task, _spec: &SpecOutput, _iteration: u32) -> Result<()> {
        Ok(())
    }

    fn run_refactor_phase(&self, _task: &Task, _spec: &SpecOutput, _iteration: u32) -> Result<()> {
        Ok(())
    }

    fn review(&self, task: &Task, spec: &SpecOutput, iteration: u32) -> Result<ReviewOutput> {
        if !self.api_enabled() {
            return Ok(Self::fallback_review());
        }

        let prompt = format!(
            "You are review engine. Return JSON only with fields: status(done|rework), coverage_score(number 0..1), missing_items(array string), spawn_candidates(array), risk_flags(array string). Task: {}. Iteration: {}. Acceptance criteria: {:?}",
            task.title, iteration, spec.acceptance_criteria
        );

        self.request_json(&prompt)
            .or_else(|_| Ok(Self::fallback_review()))
    }
}
