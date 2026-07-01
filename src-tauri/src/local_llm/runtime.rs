use super::model::{prompt_family_for_model, LocalLlmModelManifest, LocalLlmPromptFamily};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

const RECENT_LOG_CAPACITY: usize = 64;

pub struct ManagedLocalLlmServer {
    child: Child,
    pub endpoint: String,
    recent_log: Arc<Mutex<VecDeque<String>>>,
}

impl ManagedLocalLlmServer {
    pub fn stop(&mut self) -> anyhow::Result<()> {
        let _ = self.child.kill();
        let _ = self.child.wait();
        Ok(())
    }

    /// Last few lines the runtime process wrote to stderr, newest last.
    /// Useful to surface alongside an empty/failed completion, since the
    /// HTTP response alone often doesn't say why generation produced nothing.
    pub fn recent_log_tail(&self) -> Vec<String> {
        self.recent_log
            .lock()
            .map(|guard| guard.iter().cloned().collect())
            .unwrap_or_default()
    }
}

fn llama_server_binary_name() -> &'static str {
    #[cfg(windows)]
    {
        "llama-server.exe"
    }
    #[cfg(not(windows))]
    {
        "llama-server"
    }
}

fn resolve_llama_server_binary(app: &AppHandle) -> anyhow::Result<PathBuf> {
    if let Some(path) = std::env::var_os("VERENU_LLAMA_SERVER_PATH") {
        return Ok(PathBuf::from(path));
    }

    let candidates = [
        crate::app_data_dir().join("models").join("bin").join(llama_server_binary_name()),
        app.path()
            .resolve(
                std::path::PathBuf::from("bin").join(llama_server_binary_name()),
                tauri::path::BaseDirectory::Resource,
            )
            .unwrap_or_else(|_| PathBuf::from(llama_server_binary_name())),
    ];

    for candidate in candidates {
        if candidate.is_file() {
            return Ok(candidate);
        }
    }

    anyhow::bail!(
        "Local cleanup runtime not installed. Go to Settings \u{2192} Models \u{2192} Cleanup downloads to download it."
    )
}

fn pick_local_port() -> anyhow::Result<u16> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}

fn start_server_process(
    binary: &Path,
    model_path: &Path,
    port: u16,
) -> anyhow::Result<(Child, Arc<Mutex<VecDeque<String>>>)> {
    let mut command = Command::new(binary);
    command
        .arg("-m")
        .arg(model_path)
        .arg("--host")
        .arg("127.0.0.1")
        .arg("--port")
        .arg(port.to_string())
        .arg("--ctx-size")
        .arg("4096")
        // "none" forces all output into `content` unfiltered, including any
        // chain-of-thought preamble a reasoning-capable GGUF emits — verified
        // in practice this leaks raw "thinking" text into completions. "auto"
        // lets llama-server's own template-aware parser attempt to separate
        // reasoning from the final answer. Whatever this produces is still
        // screened by looks_like_model_artifact_leak() before ever being
        // used, so this is a best-effort improvement, not the safety net.
        .arg("--reasoning-format")
        .arg("auto")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());
    let mut child = command.spawn()?;

    // llama-server's own stderr is the only place that explains *why* a
    // request produced nothing (missing chat template, sampling/EOS issues,
    // GPU backend load failures, etc.) — at default verbosity it logs
    // timing/status, not prompt or completion text, so this is safe to log.
    let recent_log = Arc::new(Mutex::new(VecDeque::with_capacity(RECENT_LOG_CAPACITY)));
    if let Some(stderr) = child.stderr.take() {
        let recent_log_writer = Arc::clone(&recent_log);
        std::thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines().map_while(Result::ok) {
                log::debug!("local-llm: runtime stderr: {line}");
                if let Ok(mut guard) = recent_log_writer.lock() {
                    if guard.len() >= RECENT_LOG_CAPACITY {
                        guard.pop_front();
                    }
                    guard.push_back(line);
                }
            }
        });
    }

    Ok((child, recent_log))
}

pub(super) fn format_log_tail(lines: &[String]) -> String {
    if lines.is_empty() {
        return String::new();
    }
    let tail: Vec<&str> = lines.iter().rev().take(6).map(String::as_str).collect();
    format!(" — recent runtime log: {}", tail.into_iter().rev().collect::<Vec<_>>().join(" | "))
}

async fn wait_until_ready(
    endpoint: &str,
    child: &mut Child,
    recent_log: &Arc<Mutex<VecDeque<String>>>,
) -> anyhow::Result<()> {
    let started_at = Instant::now();
    let url = format!("{endpoint}/v1/models");
    loop {
        let tail = recent_log
            .lock()
            .map(|guard| guard.iter().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        if started_at.elapsed() > Duration::from_secs(45) {
            // child.kill() on a `Child` that's still running is the only way
            // to stop it — dropping the handle on the `?` in start_server()
            // does NOT kill the process, it just detaches the handle, which
            // previously left an orphaned llama-server.exe holding the
            // loaded model's GPU/RAM allocation with no way to clean it up.
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!(
                "local cleanup runtime timed out while starting{}",
                format_log_tail(&tail)
            )
        }
        if let Some(status) = child.try_wait()? {
            anyhow::bail!(
                "local cleanup runtime exited during startup with status {status}{}",
                format_log_tail(&tail)
            )
        }

        match crate::api::client::get().get(&url).send().await {
            Ok(resp) if resp.status().is_success() => return Ok(()),
            Ok(_) | Err(_) => {
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
        }
    }
}

pub async fn start_server(
    app: &AppHandle,
    manifest: &LocalLlmModelManifest,
    root: &Path,
) -> anyhow::Result<ManagedLocalLlmServer> {
    let binary = resolve_llama_server_binary(app)?;
    let model_path = manifest.primary_model_path(root);
    if !model_path.is_file() {
        anyhow::bail!("Downloaded local cleanup model is missing its primary GGUF file.")
    }
    let port = pick_local_port()?;
    let endpoint = format!("http://127.0.0.1:{port}");
    let (mut child, recent_log) = start_server_process(&binary, &model_path, port)?;
    wait_until_ready(&endpoint, &mut child, &recent_log).await?;
    Ok(ManagedLocalLlmServer {
        child,
        endpoint,
        recent_log,
    })
}

#[derive(Serialize)]
struct ChatReq {
    model: String,
    messages: Vec<Msg>,
    max_tokens: u32,
    temperature: f32,
    // Greedy decoding (temperature 0) has no randomness to escape a
    // repetition loop once it enters one — observed in practice on a small
    // quantized local model: the same word repeated dozens of times in a
    // row. `repeat_penalty` (a llama-server extension to the OpenAI-compatible
    // endpoint, not part of the official spec) directly discourages
    // re-emitting recently used tokens; cloud providers are unaffected since
    // this struct/endpoint is local-only.
    //
    // Deliberately the gentle ~1.1 most llama.cpp guides recommend, not a
    // stronger value: llama.cpp's repeat penalty applies over the trailing
    // token window regardless of whether those tokens came from the prompt
    // or the completion-so-far, and for a "lightly edit this dictation" task
    // the <raw_dictation> text sits at the very end of the prompt — well
    // inside that window for any non-trivial penalty strength. A stronger
    // value (this was 1.3) actively suppresses the logits for the speaker's
    // own words, which pushes the model toward paraphrasing with different
    // vocabulary just to avoid the penalty even on a short, simple
    // dictation — exactly backwards for a cleanup pass that's supposed to
    // preserve almost everything verbatim, and it shows up downstream as
    // `looks_like_fabricated_content` rejecting otherwise-fine output
    // because word overlap with the input collapsed. The original
    // degenerate-repetition risk this guarded against is now also caught
    // independently by `looks_like_degenerate_repetition` plus the pipeline's
    // hardened-retry/raw-text-fallback path, so a milder penalty here doesn't
    // remove the safety net, just stops it from firing constantly on normal
    // input.
    repeat_penalty: f32,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    stop: Vec<String>,
}

#[derive(Serialize)]
struct Msg {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatResp {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: MsgResp,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct MsgResp {
    #[serde(default)]
    content: String,
    /// Some llama-server builds segregate "thinking"/reasoning output here
    /// instead of `content` when they (mis)detect a reasoning-capable model.
    /// `--reasoning-format none` at server startup should prevent this, but
    /// fall back to reading it if a build still splits it out anyway.
    #[serde(default)]
    reasoning_content: Option<String>,
}

/// Each open-weight chat template ends a turn with a different marker.
/// Passed as an explicit `stop` string (matched against decoded text, not
/// raw token ids) so generation still terminates correctly even when a
/// GGUF's tokenizer metadata is broken and llama.cpp's automatic
/// end-of-generation detection can't find the model's real stop token.
fn stop_sequences_for_family(family: LocalLlmPromptFamily) -> Vec<String> {
    match family {
        LocalLlmPromptFamily::Gemma4 => vec!["<end_of_turn>".to_string()],
        LocalLlmPromptFamily::Qwen25 | LocalLlmPromptFamily::Smollm2 => {
            vec!["<|im_end|>".to_string()]
        }
        LocalLlmPromptFamily::Phi3 => vec!["<|end|>".to_string()],
        LocalLlmPromptFamily::Granite33 => vec!["<|end_of_text|>".to_string()],
    }
}

struct ChatAttempt {
    content: Option<String>,
    reasoning: Option<String>,
    finish_reason: Option<String>,
}

async fn send_chat_completion(endpoint: &str, body: &ChatReq) -> anyhow::Result<ChatAttempt> {
    let url = format!("{endpoint}/v1/chat/completions");
    let response = crate::api::client::get().post(&url).json(body).send().await?;
    let status = response.status();
    let response = response.error_for_status()?;
    let parsed: ChatResp = response.json().await?;
    let choice = parsed.choices.into_iter().next();
    let content = choice
        .as_ref()
        .map(|c| c.message.content.trim().to_string())
        .filter(|s| !s.is_empty());
    let reasoning = choice
        .as_ref()
        .and_then(|c| c.message.reasoning_content.as_deref())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let finish_reason = choice.as_ref().and_then(|c| c.finish_reason.clone());

    // Diagnostic only — character counts and the finish_reason enum value,
    // never the generated text itself (cleaned text must not be logged).
    log::debug!(
        "local-llm: chat completion status={status} finish_reason={finish_reason:?} content_chars={} reasoning_content_chars={}",
        content.as_ref().map(|s| s.chars().count()).unwrap_or(0),
        reasoning.as_ref().map(|s| s.chars().count()).unwrap_or(0),
    );

    Ok(ChatAttempt {
        content,
        reasoning,
        finish_reason,
    })
}

/// finish_reason="length" means the server hard-cut generation at the token
/// budget — never that the model decided to stop on its own, which always
/// reports finish_reason="stop" instead. So this alone is an unambiguous
/// truncation signal regardless of how much of the input length the output
/// reached before being cut off. Previously this also required the content
/// to be under half the input length, on the theory that a deliberately
/// brief "high" intensity result hitting its (small, proportional) budget
/// wasn't really truncated — but that ratio gate had a real blind spot: a
/// cleanup that got cut off after reaching 50-99% of the input length (a
/// dropped trailing clause, not a dropped paragraph) reported finish_reason
/// "length" just the same and went completely undetected. The budget is
/// already computed proportional to the input for every intensity (see
/// `cleanup_max_output_tokens`), so hitting it is always premature, never a
/// sign of "the model deliberately chose to be this short."
fn looks_truncated(attempt: &ChatAttempt, input_chars: usize) -> bool {
    attempt.finish_reason.as_deref() == Some("length")
        && attempt
            .content
            .as_ref()
            .map(|content| input_chars > 20 && !content.is_empty())
            .unwrap_or(false)
}

/// Combines the truncation, fabrication, and perspective-flip checks into a
/// single "is this attempt worth retrying" decision, returning a short
/// reason for logging (never the text itself).
fn retry_reason(attempt: &ChatAttempt, input_text: &str) -> Option<&'static str> {
    if looks_truncated(attempt, input_text.chars().count()) {
        return Some("looks truncated (finish_reason=length — generation was cut off at the token budget)");
    }
    if let Some(content) = &attempt.content {
        if crate::api::prompts::looks_like_fabricated_content(input_text, content) {
            return Some("looks fabricated (output shares almost no words with the input)");
        }
        if crate::api::prompts::looks_like_perspective_flip(input_text, content) {
            return Some("looks like a perspective flip (you/I swapped)");
        }
    }
    None
}

pub async fn request_cleanup(
    endpoint: &str,
    model_id: &str,
    prompt: &str,
    text: &str,
    max_tokens: u32,
) -> anyhow::Result<String> {
    // Folded into a single "user" turn rather than a separate "system" role:
    // several local chat templates (Gemma's notably) don't define a system
    // slot at all, and llama-server can silently produce an empty completion
    // instead of erroring when one is sent. A single user turn works
    // regardless of whether the model's template supports a system role.
    let stop = prompt_family_for_model(model_id)
        .map(stop_sequences_for_family)
        .unwrap_or_default();
    let body = ChatReq {
        model: model_id.to_string(),
        messages: vec![Msg {
            role: "user".into(),
            content: format!("{prompt}\n\n<raw_dictation>\n{text}\n</raw_dictation>"),
        }],
        max_tokens,
        // Small non-zero temperature: greedy decoding (0.0) has no
        // randomness to escape a repetition loop once it enters one, which
        // is exactly the failure mode observed (the same word repeated
        // dozens of times). Still low enough to stay close to deterministic.
        temperature: 0.15,
        repeat_penalty: 1.1,
        stop,
    };
    let mut attempt = send_chat_completion(endpoint, &body).await?;
    if let Some(reason) = retry_reason(&attempt, text) {
        log::warn!("local-llm: completion {reason} — retrying once");
        attempt = send_chat_completion(endpoint, &body).await?;
        if let Some(reason) = retry_reason(&attempt, text) {
            log::warn!("local-llm: retry also {reason} — using it anyway, nothing better available");
        }
    }

    attempt
        .content
        .or(attempt.reasoning)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "local cleanup runtime returned no choices (finish_reason={:?})",
                attempt.finish_reason
            )
        })
}

#[cfg(test)]
mod tests {
    use super::{looks_truncated, stop_sequences_for_family, ChatAttempt, LocalLlmPromptFamily};

    fn attempt(content: Option<&str>, finish_reason: Option<&str>) -> ChatAttempt {
        ChatAttempt {
            content: content.map(str::to_string),
            reasoning: None,
            finish_reason: finish_reason.map(str::to_string),
        }
    }

    #[test]
    fn truncation_detected_when_length_cutoff_and_content_far_shorter_than_input() {
        // The actual observed bug: a 256-char dictation cleaned down to 57 chars.
        let result = attempt(Some(&"x".repeat(57)), Some("length"));
        assert!(looks_truncated(&result, 256));
    }

    #[test]
    fn not_flagged_when_finish_reason_is_a_natural_stop() {
        let result = attempt(Some(&"x".repeat(57)), Some("stop"));
        assert!(!looks_truncated(&result, 256));
    }

    #[test]
    fn flagged_even_when_content_is_comparable_length_to_input() {
        // The actual gap this closed: a cleanup that got cut off after
        // reaching 78% of the input length (a dropped trailing clause, not a
        // dropped paragraph) used to slip through undetected because the old
        // check required content under half the input length. finish_reason
        // "length" alone already means the server cut it off mid-generation
        // — there's no length ratio at which that stops being true.
        let result = attempt(Some(&"x".repeat(200)), Some("length"));
        assert!(looks_truncated(&result, 256));
    }

    #[test]
    fn not_flagged_for_tiny_inputs_even_if_short() {
        // Avoid false positives on trivially short dictations.
        let result = attempt(Some("hi"), Some("length"));
        assert!(!looks_truncated(&result, 10));
    }

    #[test]
    fn every_prompt_family_has_a_non_empty_stop_sequence() {
        for family in [
            LocalLlmPromptFamily::Gemma4,
            LocalLlmPromptFamily::Qwen25,
            LocalLlmPromptFamily::Phi3,
            LocalLlmPromptFamily::Smollm2,
            LocalLlmPromptFamily::Granite33,
        ] {
            let stops = stop_sequences_for_family(family);
            assert!(!stops.is_empty(), "{family:?} has no stop sequence configured");
            assert!(stops.iter().all(|s| !s.trim().is_empty()));
        }
    }

    #[test]
    fn gemma_stop_sequence_matches_its_real_turn_marker() {
        // Gemma's chat template ends a turn with <end_of_turn>, not </s> —
        // this is the specific marker this fix exists to enforce.
        assert_eq!(
            stop_sequences_for_family(LocalLlmPromptFamily::Gemma4),
            vec!["<end_of_turn>".to_string()]
        );
    }
}
