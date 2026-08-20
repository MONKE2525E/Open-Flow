//! Tauri command handlers, split by domain into submodules. Everything is
//! re-exported here so the `commands::*` paths used by `generate_handler!` in
//! `main.rs` stay stable. Shared imports are re-exported as `pub(crate) use` so
//! each submodule picks them up via `use super::*`.

pub(crate) use tauri::{AppHandle, Emitter, Manager};

pub(crate) use crate::data::{db, store};
pub(crate) use crate::media::audio;
pub(crate) use crate::pipeline::{self, SharedState};
pub(crate) use crate::system::apps::{AppMapping, InstalledApp};
pub(crate) use crate::DbHandle;

/// Shown to the user (as a command error / toast) whenever they try to
/// download a local STT/LLM model or runtime on an Intel Mac build. See
/// `system::platform::is_macos_intel` for why this is gated off entirely
/// rather than just discouraged.
pub(crate) const LOCAL_MODELS_UNAVAILABLE_ON_MACOS_INTEL: &str =
    "Local on-device models aren't available on Intel Macs yet — this hasn't been tested on Intel hardware. Use a cloud provider (Groq, OpenAI, or Google) for now.";

mod contexts;
mod history;
mod library;
mod local_llm;
mod local_stt;
mod permissions;
mod recording;
mod service_status;
mod settings;
mod system;
mod updater;

pub(crate) async fn run_blocking<T, F>(label: &'static str, f: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| format!("{label} task panicked: {e}"))?
}

fn db_state(app: &AppHandle) -> DbHandle {
    app.state::<DbHandle>().inner().clone()
}

pub use contexts::*;
pub use history::*;
pub use library::*;
pub use local_llm::*;
pub use local_stt::*;
pub use permissions::*;
pub use recording::*;
pub use service_status::*;
pub use settings::*;
pub use system::*;
pub use updater::*;

#[cfg(test)]
mod tests {
    use super::{backup_sqlite_database, classify_validation_response, validate_setting};
    use serde_json::json;

    #[test]
    fn classify_validation_response_accepts_2xx() {
        let result = classify_validation_response(200, "");
        assert!(result.ok);
        assert_eq!(result.status, "valid");
    }

    #[test]
    fn classify_validation_response_flags_invalid_key() {
        let result =
            classify_validation_response(401, r#"{"error":{"message":"Invalid API Key"}}"#);
        assert!(!result.ok);
        assert_eq!(result.status, "invalid");
        assert!(result.message.to_lowercase().contains("invalid"));
    }

    #[test]
    fn classify_validation_response_flags_scope_restriction() {
        let result = classify_validation_response(
            403,
            r#"{"error":{"message":"Only team owners or users with the developer role may create or manage API keys."}}"#,
        );
        assert!(!result.ok);
        assert_eq!(result.status, "invalid");
        assert!(result
            .message
            .to_lowercase()
            .contains("account or model-access"));
    }

    #[test]
    fn classify_validation_response_treats_other_statuses_as_inconclusive() {
        let result = classify_validation_response(503, "");
        assert!(!result.ok);
        assert_eq!(result.status, "unknown");
        assert!(result.message.contains("503"));
    }

    #[test]
    fn validate_setting_rejects_unknown_keys() {
        let err = validate_setting("not_a_setting", &json!(true)).expect_err("unknown key");
        assert!(err.contains("Invalid or unsupported setting"));
    }

    #[test]
    fn validate_setting_accepts_provider_model_maps() {
        let value = json!({
            "groq": ["whisper-large-v3-turbo"],
            "openai": ["gpt-4o-transcribe"],
            "google": ["gemini-3.7-flash"]
        });
        assert!(
            validate_setting(crate::data::store::TRANSCRIPTION_MODELS_BY_PROVIDER, &value).is_ok()
        );
    }

    #[test]
    fn validate_setting_rejects_empty_fallback_entries() {
        let value = json!(["groq/whisper-large-v3-turbo", ""]);
        let err = validate_setting(crate::data::store::TRANSCRIPTION_FALLBACK_MODELS, &value)
            .expect_err("empty fallback should fail");
        assert!(err.contains("Invalid or unsupported setting"));
    }

    #[test]
    fn validate_setting_rejects_invalid_language_codes() {
        let err = validate_setting(crate::data::store::TRANSCRIPTION_LANGUAGE, &json!("xx"))
            .expect_err("invalid language should fail");
        assert!(err.contains("Invalid or unsupported setting"));
    }

    #[test]
    fn validate_setting_requires_two_hotkey_parts() {
        let err = validate_setting(crate::data::store::HOTKEY, &json!(["ControlLeft"]))
            .expect_err("single hotkey part should fail");
        assert!(err.contains("Invalid or unsupported setting"));
    }

    #[test]
    fn validate_setting_hotkey_rejects_unknown_key_codes() {
        // An imported/edited settings.json must not be able to store a hotkey
        // no backend can register — that would silently disable dictation at
        // startup (both codes map to VK 0).
        let err = validate_setting(crate::data::store::HOTKEY, &json!(["Foo", "Bar"]))
            .expect_err("unknown hotkey codes should fail");
        assert!(err.contains("Invalid or unsupported setting"));
    }

    #[test]
    fn validate_setting_hotkey_accepts_registrable_codes() {
        assert!(validate_setting(
            crate::data::store::HOTKEY,
            &json!(["ControlLeft", "MetaLeft"])
        )
        .is_ok());
        // macOS single-key hotkeys store an empty second slot.
        assert!(validate_setting(crate::data::store::HOTKEY, &json!(["F5", ""])).is_ok());
        assert!(validate_setting(crate::data::store::HOTKEY, &json!(["AltLeft", "Space"])).is_ok());
    }

    #[test]
    fn validate_setting_mic_gain_enforces_slider_range() {
        assert!(validate_setting(crate::data::store::MIC_GAIN, &json!(1.0)).is_ok());
        assert!(validate_setting(crate::data::store::MIC_GAIN, &json!(8.0)).is_ok());
        assert!(validate_setting(crate::data::store::MIC_GAIN, &json!(0.9)).is_err());
        assert!(validate_setting(crate::data::store::MIC_GAIN, &json!(8.1)).is_err());
    }

    #[test]
    fn validate_setting_rejects_non_boolean_bools() {
        for key in [
            crate::data::store::CLEANUP_ENABLED,
            crate::data::store::NOISE_REDUCTION,
            crate::data::store::AUTO_SPACING,
            crate::data::store::CONTEXTUAL_FORMATTING,
            crate::data::store::DUAL_TRANSCRIPTION_ENABLED,
        ] {
            assert!(
                validate_setting(key, &json!("yes")).is_err(),
                "{key} should reject non-boolean values"
            );
            assert!(validate_setting(key, &json!(true)).is_ok());
        }
    }

    #[test]
    fn validate_setting_rejects_unknown_history_retention() {
        let err = validate_setting(crate::data::store::HISTORY_RETENTION, &json!("365 days"))
            .expect_err("unknown history retention should fail");
        assert!(err.contains("Invalid or unsupported setting"));
    }

    #[test]
    fn validate_setting_rejects_unknown_default_tone() {
        let err = validate_setting(crate::data::store::DEFAULT_TONE, &json!("business"))
            .expect_err("unknown default tone should fail");
        assert!(err.contains("Invalid or unsupported setting"));
    }

    #[test]
    fn validate_setting_rejects_unknown_cleanup_intensity() {
        let err = validate_setting(crate::data::store::CLEANUP_INTENSITY, &json!("extreme"))
            .expect_err("unknown cleanup intensity should fail");
        assert!(err.contains("Invalid or unsupported setting"));
    }

    #[test]
    fn validate_setting_rejects_invalid_app_mapping_profile() {
        let err = validate_setting(
            crate::data::store::APP_MAPPINGS,
            &json!([{
                "exe": "chrome.exe",
                "profile": "business"
            }]),
        )
        .expect_err("invalid app mapping profile should fail");
        assert!(err.contains("Invalid or unsupported setting"));
    }

    #[test]
    fn validate_setting_rejects_invalid_app_mapping_cleanup_intensity() {
        let err = validate_setting(
            crate::data::store::APP_MAPPINGS,
            &json!([{
                "exe": "chrome.exe",
                "profile": "casual",
                "cleanup_intensity": "extreme"
            }]),
        )
        .expect_err("invalid app mapping cleanup intensity should fail");
        assert!(err.contains("Invalid or unsupported setting"));
    }

    #[test]
    fn validate_setting_rejects_oversized_cleanup_prompt_override() {
        let too_long = "x".repeat(20_001);
        let err = validate_setting(
            crate::data::store::CLEANUP_PROMPT_OVERRIDES,
            &json!({
                "groq/llama-3.3-70b-versatile": too_long
            }),
        )
        .expect_err("oversized cleanup prompt override should fail");
        assert!(err.contains("Invalid or unsupported setting"));
    }

    #[test]
    fn backup_sqlite_database_copies_live_data() {
        let root = std::env::temp_dir().join(format!(
            "verenu-backup-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).expect("create temp dir");

        let db_path = root.join("verenu.db");
        let backup_path = root.join("verenu.db.bak");
        let conn = rusqlite::Connection::open(&db_path).expect("open source db");
        conn.execute_batch("CREATE TABLE t (id INTEGER PRIMARY KEY, val TEXT NOT NULL);")
            .expect("create table");
        conn.execute("INSERT INTO t (val) VALUES ('hello')", [])
            .expect("insert row");

        backup_sqlite_database(&conn, &backup_path).expect("backup succeeds");

        let backup_conn = rusqlite::Connection::open(&backup_path).expect("open backup db");
        let val: String = backup_conn
            .query_row("SELECT val FROM t WHERE id = 1", [], |r| r.get(0))
            .expect("read backed-up row");
        assert_eq!(val, "hello");

        drop(conn);
        drop(backup_conn);
        let _ = std::fs::remove_dir_all(root);
    }
}
