#![cfg(any(test, debug_assertions))]
#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Debug)]
pub struct FixtureSpec {
    pub task: String,
    pub provider: String,
    pub model: String,
    pub response: Option<String>,
    pub error_kind: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InjectionRecord {
    pub text: String,
    pub target_hwnd: usize,
    pub contextual_caps: bool,
    pub auto_spacing: bool,
    pub profile: String,
}

#[derive(Default)]
struct HarnessState {
    enabled: bool,
    fixtures: HashMap<String, FixtureSpec>,
    hits: HashMap<String, usize>,
    injections: Vec<InjectionRecord>,
}

fn harness() -> &'static Mutex<HarnessState> {
    static HARNESS: OnceLock<Mutex<HarnessState>> = OnceLock::new();
    HARNESS.get_or_init(|| Mutex::new(HarnessState::default()))
}

fn lock_harness() -> std::sync::MutexGuard<'static, HarnessState> {
    match harness().lock() {
        Ok(guard) => guard,
        Err(err) => {
            let mut guard = err.into_inner();
            guard.fixtures.clear();
            guard.hits.clear();
            guard.injections.clear();
            guard
        }
    }
}

fn key(task: &str, provider: &str, model: &str) -> String {
    format!(
        "{}|{}|{}",
        task.trim().to_lowercase(),
        provider.trim().to_lowercase(),
        model.trim()
    )
}

fn provider_label(provider: &str) -> &'static str {
    match provider {
        "openai" => "OpenAI",
        "google" => "Google",
        "assemblyai" => "AssemblyAI",
        "local" => "Local",
        _ => "Groq",
    }
}

pub fn is_enabled() -> bool {
    if std::env::var("OPEN_FLOW_TEST_MODE").ok().as_deref() == Some("1") {
        return true;
    }
    lock_harness().enabled
}

pub fn set_enabled(enabled: bool) {
    lock_harness().enabled = enabled;
}

pub fn reset() {
    *lock_harness() = HarnessState::default();
}

pub fn register_fixture(spec: FixtureSpec) {
    let mut state = lock_harness();
    state
        .fixtures
        .insert(key(&spec.task, &spec.provider, &spec.model), spec);
}

pub fn fixture_hit_count(task: &str, provider: &str, model: &str) -> usize {
    lock_harness()
        .hits
        .get(&key(task, provider, model))
        .copied()
        .unwrap_or(0)
}

pub fn resolve_provider_fixture(
    task: &str,
    provider: &str,
    model: &str,
) -> Option<anyhow::Result<String>> {
    if !is_enabled() {
        return None;
    }

    let lookup = key(task, provider, model);
    let mut state = lock_harness();
    let hit = state.hits.entry(lookup.clone()).or_insert(0);
    *hit += 1;

    let fixture = match state.fixtures.get(&lookup).cloned() {
        Some(f) => f,
        None => {
            return Some(Err(anyhow::anyhow!(
                "Missing mock fixture for task='{}' provider='{}' model='{}'",
                task,
                provider,
                model
            )));
        }
    };

    if let Some(response) = fixture.response {
        return Some(Ok(response));
    }

    let error_kind = fixture.error_kind.as_deref().unwrap_or("generic");
    let error_message = fixture
        .error_message
        .unwrap_or_else(|| format!("fixture {task} failure for {provider}/{model}"));

    let error = match error_kind {
        "quota" => crate::api::quota_bail(model),
        "auth_invalid" => crate::api::auth_401_error(
            provider_label(provider),
            model,
            "fixture-request",
            crate::api::AuthErrorCategory::InvalidOrRevokedKey,
        ),
        "auth_scope" => crate::api::auth_401_error(
            provider_label(provider),
            model,
            "fixture-request",
            crate::api::AuthErrorCategory::ScopeOrAccountRestriction,
        ),
        "timeout" => anyhow::anyhow!("fixture timeout: {error_message}"),
        "status_503" => anyhow::anyhow!("provider failure status=503: {error_message}"),
        "status_429" => anyhow::anyhow!("provider failure status=429: {error_message}"),
        _ => anyhow::anyhow!("{error_message}"),
    };

    Some(Err(error))
}

pub fn record_injection(record: InjectionRecord) {
    if !is_enabled() {
        return;
    }
    lock_harness().injections.push(record);
}

pub fn take_injections() -> Vec<InjectionRecord> {
    std::mem::take(&mut lock_harness().injections)
}
