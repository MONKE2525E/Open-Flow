use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter};

use crate::data::db;
use crate::DbHandle;

static MONITOR_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Minimum edit distance threshold for two words to be considered a spelling
/// correction rather than a completely different word.
fn is_spelling_correction(a: &str, b: &str) -> bool {
    let max_len = a.len().max(b.len());
    if max_len == 0 {
        return false;
    }
    edit_distance(a, b) <= 2_usize.max(max_len / 2)
}

fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (m, n) = (a.len(), b.len());
    let mut row: Vec<usize> = (0..=n).collect();
    for i in 1..=m {
        let mut prev = row[0];
        row[0] = i;
        for j in 1..=n {
            let old = row[j];
            row[j] = if a[i - 1] == b[j - 1] {
                prev
            } else {
                1 + prev.min(row[j]).min(row[j - 1])
            };
            prev = old;
        }
    }
    row[n]
}

/// COM apartment guard — initializes once per thread, uninitializes on drop.
/// Stored in a thread-local so COM is set up at most once per spawned thread.
#[cfg(windows)]
struct ComGuard(bool);

#[cfg(windows)]
impl ComGuard {
    fn init() -> Self {
        use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
        let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        // S_OK / S_FALSE = we initialized it; RPC_E_CHANGED_MODE = already init'd by caller.
        ComGuard(hr.is_ok())
    }
}

#[cfg(windows)]
impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.0 {
            use windows::Win32::System::Com::CoUninitialize;
            unsafe { CoUninitialize() };
        }
    }
}

#[cfg(windows)]
thread_local! {
    static COM_INIT: std::cell::RefCell<Option<ComGuard>> = std::cell::RefCell::new(None);
}

/// Read the text value of the currently focused UI element via Windows UI Automation.
/// Tries ValuePattern first (simple inputs), then TextPattern (browsers, rich text editors).
#[cfg(windows)]
pub fn read_focused_text() -> Option<String> {
    use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
    use windows::Win32::UI::Accessibility::{
        CUIAutomation, IUIAutomation, IUIAutomationTextPattern, IUIAutomationValuePattern,
        UIA_TextPatternId, UIA_ValuePatternId,
    };

    // Ensure COM is initialized for this thread — no-op after the first call.
    COM_INIT.with(|cell| {
        let mut guard = cell.borrow_mut();
        if guard.is_none() {
            *guard = Some(ComGuard::init());
        }
    });

    unsafe {
        let automation: IUIAutomation =
            CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()?;
        let element = automation.GetFocusedElement().ok()?;

        // Try ValuePattern first (input fields, textareas in native apps).
        if let Ok(pattern) =
            element.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
        {
            if let Ok(val) = pattern.CurrentValue() {
                let s = val.to_string();
                if !s.is_empty() {
                    return Some(s);
                }
            }
        }

        // Fall back to TextPattern (browsers, rich text editors, VS Code, etc.).
        if let Ok(pattern) =
            element.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
        {
            if let Ok(doc_range) = pattern.DocumentRange() {
                if let Ok(val) = doc_range.GetText(-1) {
                    let s = val.to_string();
                    if !s.is_empty() {
                        return Some(s);
                    }
                }
            }
        }

        None
    }
}

#[cfg(not(windows))]
pub fn read_focused_text() -> Option<String> {
    None
}

/// Word-level positional diff returning (injected_word, user_corrected_word) pairs.
/// Only returns pairs that look like spelling corrections (similar words, not rewrites).
pub fn diff_words(original: &str, current: &str) -> Vec<(String, String)> {
    let orig: Vec<&str> = original.split_whitespace().collect();
    let curr: Vec<&str> = current.split_whitespace().collect();

    // If word count grew >2x+10 the user typed substantial new content — ignore.
    if curr.len() > orig.len() * 2 + 10 {
        return vec![];
    }

    let mut diffs = Vec::new();
    for i in 0..orig.len().min(curr.len()) {
        let o = orig[i].trim_matches(|c: char| !c.is_alphanumeric());
        let c = curr[i].trim_matches(|c: char| !c.is_alphanumeric());
        if o.is_empty() || c.is_empty() {
            continue;
        }
        let o_low = o.to_lowercase();
        let c_low = c.to_lowercase();
        if o_low != c_low && is_spelling_correction(&o_low, &c_low) {
            diffs.push((o.to_string(), c.to_string()));
        }
    }
    diffs
}

/// Spawn a background monitor task that polls the focused text field every 2 seconds
/// and records word-level spelling corrections.
/// After the same (wrong → correct) pair is seen 3 times within a 7-day window
/// across any number of sessions, it is added to the dictionary automatically.
pub fn start_monitor(injected_text: String, db: DbHandle, app: AppHandle) {
    // Skip very short injections — positional diff on < 2 words is too noisy.
    if injected_text.split_whitespace().count() < 2 {
        return;
    }

    // Only one monitor at a time.
    if MONITOR_ACTIVE.swap(true, Ordering::AcqRel) {
        return;
    }

    tauri::async_runtime::spawn(async move {
        let mut last_text: Option<String> = None;
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(60);
        // Track pairs already recorded this session so one 30-second window only
        // contributes one row per (wrong, correct) pair to pending_corrections.
        let mut recorded_this_session: HashSet<(String, String)> = HashSet::new();

        loop {
            if tokio::time::Instant::now() >= deadline {
                break;
            }

            tokio::time::sleep(std::time::Duration::from_secs(2)).await;

            let current_text = tokio::task::spawn_blocking(read_focused_text).await;

            let current_text = match current_text {
                Ok(Some(t)) => t,
                // UIA read failed (focus moved to unsupported element) — skip this tick.
                _ => continue,
            };

            let orig_wc = injected_text.split_whitespace().count();
            if current_text.split_whitespace().count() > orig_wc * 2 + 10 {
                break;
            }

            if last_text.as_deref() == Some(current_text.as_str()) {
                last_text = Some(current_text);
                continue;
            }

            let diffs = diff_words(&injected_text, &current_text);
            last_text = Some(current_text);

            for (mistake, correction) in diffs {
                let key = (mistake.clone(), correction.clone());
                if recorded_this_session.contains(&key) {
                    continue;
                }
                recorded_this_session.insert(key);

                let _ = db::insert_pending_correction(&db, &mistake, &correction);

                let count = db::count_pending_corrections_last_week(&db, &mistake, &correction)
                    .unwrap_or(0);

                if count >= 2 {
                    let _ = db::insert_dictionary_entry_auto_learned(
                        &db,
                        &correction,
                        Some(&mistake),
                    );
                    log::info!(
                        "auto-learn: '{}' → '{}' promoted to dictionary after {} corrections",
                        mistake,
                        correction,
                        count
                    );
                    app.emit("open-flow:dictionary-updated", ()).ok();
                }
            }
        }

        MONITOR_ACTIVE.store(false, Ordering::Release);
    });
}
