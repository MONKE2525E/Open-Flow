use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};

use crate::data::db;
use crate::DbHandle;

pub struct AutoLearnState {
    pub pending: HashMap<(String, String), u32>,
    pub active_monitor: bool,
}

impl AutoLearnState {
    pub fn new() -> Self {
        Self {
            pending: HashMap::new(),
            active_monitor: false,
        }
    }
}

pub type SharedAutoLearnState = Arc<Mutex<AutoLearnState>>;

/// Read the text value of the currently focused UI element via Windows UI Automation.
/// Must be called from a thread where COM is initialized (use spawn_blocking).
#[cfg(windows)]
pub fn read_focused_text() -> Option<String> {
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Accessibility::{
        CUIAutomation, IUIAutomation, IUIAutomationValuePattern, UIA_ValuePatternId,
    };

    unsafe {
        let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let com_ok = hr.is_ok();

        let result = (|| -> Option<String> {
            let automation: IUIAutomation =
                CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()?;
            let element = automation.GetFocusedElement().ok()?;
            let pattern = element
                .GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
                .ok()?;
            Some(pattern.CurrentValue().ok()?.to_string())
        })();

        if com_ok {
            CoUninitialize();
        }

        result
    }
}

#[cfg(not(windows))]
pub fn read_focused_text() -> Option<String> {
    None
}

/// Word-level positional diff. Returns (original_word, corrected_word) pairs.
/// Strips leading/trailing punctuation before comparing so "colour," vs "color" matches.
pub fn diff_words(original: &str, current: &str) -> Vec<(String, String)> {
    let orig: Vec<&str> = original.split_whitespace().collect();
    let curr: Vec<&str> = current.split_whitespace().collect();

    // If the text grew more than 2× in word count the user typed substantial new
    // content — don't treat that as a correction.
    if curr.len() > orig.len() * 2 {
        return vec![];
    }

    let mut diffs = Vec::new();
    for i in 0..orig.len().min(curr.len()) {
        let o = orig[i].trim_matches(|c: char| !c.is_alphanumeric());
        let c = curr[i].trim_matches(|c: char| !c.is_alphanumeric());
        if !o.is_empty() && !c.is_empty() && o.to_lowercase() != c.to_lowercase() {
            diffs.push((o.to_string(), c.to_string()));
        }
    }
    diffs
}

/// Spawn a background monitor task that polls the focused text field every 2 seconds
/// and records word-level corrections. After the same correction is seen twice,
/// it is inserted into the dictionary with auto_learned = true.
pub fn start_monitor(
    injected_text: String,
    db: DbHandle,
    auto_learn_state: SharedAutoLearnState,
    app: AppHandle,
) {
    {
        let mut st = auto_learn_state.lock().unwrap();
        if st.active_monitor {
            return;
        }
        // Skip very short injections — positional diff on < 3 words is too noisy.
        if injected_text.split_whitespace().count() < 3 {
            return;
        }
        st.active_monitor = true;
    }

    let state_clone = auto_learn_state.clone();

    tauri::async_runtime::spawn(async move {
        let mut last_text: Option<String> = None;
        let deadline =
            tokio::time::Instant::now() + std::time::Duration::from_secs(30);

        loop {
            if tokio::time::Instant::now() >= deadline {
                break;
            }

            tokio::time::sleep(std::time::Duration::from_secs(2)).await;

            let current_text =
                tokio::task::spawn_blocking(read_focused_text).await;

            let current_text = match current_text {
                Ok(Some(t)) => t,
                // UIA unavailable or focused element has no value pattern — stop.
                _ => break,
            };

            // Stop if word count grew dramatically (user switched fields or typed a lot).
            let orig_wc = injected_text.split_whitespace().count();
            if current_text.split_whitespace().count() > orig_wc * 2 + 10 {
                break;
            }

            // Skip unchanged polls.
            if last_text.as_deref() == Some(current_text.as_str()) {
                last_text = Some(current_text);
                continue;
            }

            let diffs = diff_words(&injected_text, &current_text);
            last_text = Some(current_text);

            if diffs.is_empty() {
                continue;
            }

            let mut st = state_clone.lock().unwrap();
            for (mistake, correction) in diffs {
                let count = st
                    .pending
                    .entry((mistake.clone(), correction.clone()))
                    .or_insert(0);
                *count += 1;

                if *count >= 2 {
                    let _ = db::insert_dictionary_entry_auto_learned(
                        &db,
                        &correction,
                        Some(&mistake),
                    );
                    log::info!(
                        "auto-learn: '{}' → '{}' added to dictionary",
                        mistake,
                        correction
                    );
                    // Notify the frontend so the Dictionary view can refresh.
                    app.emit("open-flow:dictionary-updated", ()).ok();
                }
            }
        }

        state_clone.lock().unwrap().active_monitor = false;
    });
}
