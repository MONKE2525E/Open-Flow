use super::*;

#[cfg(windows)]
pub(super) struct EventModeHookGuard;

#[cfg(windows)]
impl EventModeHookGuard {
    pub(super) fn new() -> Self {
        ACTIVE_EVENT_MODE_MONITORS.fetch_add(1, Ordering::SeqCst);
        Self
    }
}

#[cfg(windows)]
impl Drop for EventModeHookGuard {
    fn drop(&mut self) {
        loop {
            let current = ACTIVE_EVENT_MODE_MONITORS.load(Ordering::SeqCst);
            if current == 0 {
                return;
            }
            if ACTIVE_EVENT_MODE_MONITORS
                .compare_exchange(current, current - 1, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                if current == 1 {
                    request_value_change_hook_shutdown();
                }
                return;
            }
        }
    }
}
#[cfg(windows)]
struct ComGuard(bool);

#[cfg(windows)]
impl ComGuard {
    fn init() -> Self {
        use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
        let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
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
    static FOCUSED_TEXT_STATE: std::cell::RefCell<FocusedTextState> = const { std::cell::RefCell::new(FocusedTextState::new()) };
}

#[cfg(windows)]
struct FocusedTextReader {
    automation: windows::Win32::UI::Accessibility::IUIAutomation,
}

#[cfg_attr(not(windows), allow(dead_code))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FocusedTextProbe {
    Text(String),
    NonTextFocus,
    Unavailable,
}

#[cfg(windows)]
pub(super) fn control_type_label(control_type: i32) -> String {
    use windows::Win32::UI::Accessibility::{
        UIA_CustomControlTypeId, UIA_DocumentControlTypeId, UIA_EditControlTypeId,
        UIA_PaneControlTypeId, UIA_TextControlTypeId, UIA_WindowControlTypeId,
    };

    match control_type {
        value if value == UIA_EditControlTypeId.0 => "edit".to_string(),
        value if value == UIA_DocumentControlTypeId.0 => "document".to_string(),
        value if value == UIA_TextControlTypeId.0 => "text".to_string(),
        value if value == UIA_PaneControlTypeId.0 => "pane".to_string(),
        value if value == UIA_WindowControlTypeId.0 => "window".to_string(),
        value if value == UIA_CustomControlTypeId.0 => "custom".to_string(),
        other => format!("control_type_{other}"),
    }
}

#[cfg_attr(not(windows), allow(dead_code))]
pub(super) fn is_effectively_empty_text(text: &str) -> bool {
    text.chars().all(is_invisible_probe_char)
}

#[cfg(test)]
pub(super) fn classify_caret_char(ch: char) -> Option<SentenceContext> {
    if matches!(ch, '.' | '!' | '?' | '\n' | '\r') {
        return Some(SentenceContext::NewSentence);
    }
    if is_invisible_probe_char(ch) {
        return None;
    }
    if ch.is_alphanumeric()
        || matches!(
            ch,
            ',' | ';' | ':' | '-' | '–' | '—' | '/' | '\\' | ')' | ']' | '}' | '>'
        )
    {
        return Some(SentenceContext::MidSentence);
    }
    None
}

#[cfg(windows)]
struct ContextEdges {
    left: String,
    right: String,
    left_reliable: bool,
    right_reliable: bool,
}

#[cfg(windows)]
unsafe fn read_context_edges(
    range: &windows::Win32::UI::Accessibility::IUIAutomationTextRange,
    document_range: Option<&windows::Win32::UI::Accessibility::IUIAutomationTextRange>,
) -> Option<ContextEdges> {
    use windows::Win32::UI::Accessibility::{
        TextPatternRangeEndpoint_End, TextPatternRangeEndpoint_Start, TextUnit_Character,
    };

    let left = range.Clone().ok()?;
    left.MoveEndpointByRange(
        TextPatternRangeEndpoint_End,
        range,
        TextPatternRangeEndpoint_Start,
    )
    .ok()?;
    left.MoveEndpointByUnit(TextPatternRangeEndpoint_Start, TextUnit_Character, -64)
        .ok()?;
    let left_text = left.GetText(-1).ok()?.to_string();

    let right = range.Clone().ok()?;
    right
        .MoveEndpointByRange(
            TextPatternRangeEndpoint_Start,
            range,
            TextPatternRangeEndpoint_End,
        )
        .ok()?;
    right
        .MoveEndpointByUnit(TextPatternRangeEndpoint_End, TextUnit_Character, 64)
        .ok()?;
    let right_text = right.GetText(-1).ok()?.to_string();

    let left_at_document_start = document_range.is_some_and(|document| {
        matches!(
            range.CompareEndpoints(
                TextPatternRangeEndpoint_Start,
                document,
                TextPatternRangeEndpoint_Start,
            ),
            Ok(0)
        )
    });
    let right_at_document_end = document_range.is_some_and(|document| {
        matches!(
            range.CompareEndpoints(
                TextPatternRangeEndpoint_End,
                document,
                TextPatternRangeEndpoint_End,
            ),
            Ok(0)
        )
    });

    Some(ContextEdges {
        left_reliable: !left_text.is_empty() || left_at_document_start,
        right_reliable: !right_text.is_empty() || right_at_document_end,
        left: left_text,
        right: right_text,
    })
}

#[cfg(windows)]
unsafe fn range_is_collapsed(
    range: &windows::Win32::UI::Accessibility::IUIAutomationTextRange,
) -> bool {
    use windows::Win32::UI::Accessibility::{
        TextPatternRangeEndpoint_End, TextPatternRangeEndpoint_Start,
    };

    matches!(
        range.CompareEndpoints(
            TextPatternRangeEndpoint_Start,
            range,
            TextPatternRangeEndpoint_End
        ),
        Ok(0)
    )
}

#[cfg(test)]
pub(super) fn resolve_injection_context(
    field_empty: bool,
    caret_context: Option<char>,
) -> SentenceContext {
    if field_empty {
        SentenceContext::NewSentence
    } else if let Some(ch) = caret_context {
        classify_caret_char(ch).unwrap_or(SentenceContext::Unknown)
    } else {
        SentenceContext::Unknown
    }
}

#[cfg(windows)]
struct FocusedTextState {
    // Reader drops before COM guard because fields drop in declaration order.
    reader: Option<Option<FocusedTextReader>>,
    com: Option<ComGuard>,
}

#[cfg(windows)]
impl FocusedTextState {
    const fn new() -> Self {
        Self {
            reader: None,
            com: None,
        }
    }
}

#[cfg(windows)]
impl FocusedTextReader {
    fn new() -> Option<Self> {
        use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
        use windows::Win32::UI::Accessibility::{CUIAutomation, IUIAutomation};

        unsafe {
            let automation: IUIAutomation =
                CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()?;
            Some(Self { automation })
        }
    }

    fn read_probe(&self) -> FocusedTextProbe {
        use windows::Win32::UI::Accessibility::{
            IUIAutomationTextPattern, IUIAutomationValuePattern, UIA_TextPatternId,
            UIA_ValuePatternId,
        };

        unsafe {
            let element = match self.automation.GetFocusedElement() {
                Ok(element) => element,
                Err(_) => return FocusedTextProbe::Unavailable,
            };
            let mut saw_text_pattern = false;
            let mut accessible_empty: Option<String> = None;

            if let Ok(pattern) =
                element.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
            {
                saw_text_pattern = true;
                if let Ok(val) = pattern.CurrentValue() {
                    let s = val.to_string();
                    if !is_effectively_empty_text(&s) {
                        return FocusedTextProbe::Text(s);
                    }
                    accessible_empty = Some(s);
                }
            }

            if let Ok(pattern) =
                element.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
            {
                saw_text_pattern = true;
                if let Ok(doc_range) = pattern.DocumentRange() {
                    if let Ok(val) = doc_range.GetText(-1) {
                        let s = val.to_string();
                        if !is_effectively_empty_text(&s) {
                            return FocusedTextProbe::Text(s);
                        }
                        accessible_empty = Some(s);
                    }
                }
            }

            if let Some(s) = accessible_empty {
                return FocusedTextProbe::Text(s);
            }

            if saw_text_pattern {
                FocusedTextProbe::Text(String::new())
            } else {
                FocusedTextProbe::NonTextFocus
            }
        }
    }

    fn read_injection_context_probe(&self) -> InjectionContextProbe {
        use windows::Win32::UI::Accessibility::{
            IUIAutomationTextPattern, IUIAutomationTextPattern2, IUIAutomationValuePattern,
            UIA_TextPattern2Id, UIA_TextPatternId, UIA_ValuePatternId,
        };

        unsafe {
            let element = match self.automation.GetFocusedElement() {
                Ok(element) => element,
                Err(_) => {
                    return InjectionContextProbe::unavailable(
                        ContextProbeSource::Unavailable,
                        "unknown",
                    );
                }
            };

            let control_type = element
                .CurrentControlType()
                .map(|value| control_type_label(value.0))
                .unwrap_or_else(|_| "unknown".to_string());
            let value_pattern = element
                .GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
                .ok();
            let text_pattern = element
                .GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
                .ok();
            let text_pattern2 = element
                .GetCurrentPatternAs::<IUIAutomationTextPattern2>(UIA_TextPattern2Id)
                .ok();
            let read_only = value_pattern
                .as_ref()
                .and_then(|pattern| pattern.CurrentIsReadOnly().ok())
                .map(|value| value.as_bool());
            let value_text = value_pattern
                .as_ref()
                .and_then(|pattern| pattern.CurrentValue().ok())
                .map(|value| value.to_string());
            let value_is_empty = value_text.as_deref().map(is_effectively_empty_text);
            let automation_id = element
                .CurrentAutomationId()
                .map(|value| value.to_string())
                .unwrap_or_default();
            let class_name = element
                .CurrentClassName()
                .map(|value| value.to_string())
                .unwrap_or_default();
            let native_hwnd = element
                .CurrentNativeWindowHandle()
                .map(|value| format!("{:p}", value.0))
                .unwrap_or_default();
            let control_identity_hash = stable_metadata_hash(&[
                control_type.as_str(),
                automation_id.as_str(),
                class_name.as_str(),
                native_hwnd.as_str(),
            ]);
            let target_id = element.CurrentProcessId().unwrap_or_default().max(0) as usize;

            if read_only == Some(true) {
                return InjectionContextProbe {
                    context: SentenceContext::Unknown,
                    source: ContextProbeSource::UnsupportedControl,
                    context_tail: String::new(),
                    context_head: String::new(),
                    left_reliable: false,
                    right_reliable: false,
                    control_type,
                    selection_state: SelectionState::Unknown,
                    control_identity_hash,
                    target_id,
                };
            }

            let mut range_seen = false;
            let mut range_collapsed = false;
            let document_range = text_pattern
                .as_ref()
                .and_then(|pattern| pattern.DocumentRange().ok());
            let mut context_edges: Option<ContextEdges> = None;
            let mut source = ContextProbeSource::UnsupportedControl;

            // TextPattern selection is authoritative for both collapsed carets
            // and replacement selections, and gives us both insertion edges.
            if let Some(pattern) = &text_pattern {
                if let Ok(selection) = pattern.GetSelection() {
                    if let Ok(len) = selection.Length() {
                        if len == 1 {
                            if let Ok(range) = selection.GetElement(0) {
                                range_seen = true;
                                range_collapsed = range_is_collapsed(&range);
                                context_edges = read_context_edges(&range, document_range.as_ref());
                                source = if context_edges.as_ref().is_some_and(|edges| {
                                    edges.left_reliable || edges.right_reliable
                                }) {
                                    ContextProbeSource::CaretLocal
                                } else {
                                    ContextProbeSource::AmbiguousSelection
                                };
                            }
                        } else if len > 1 {
                            range_seen = true;
                            source = ContextProbeSource::AmbiguousSelection;
                        }
                    }
                }
            }

            if let Some(pattern) = &text_pattern2 {
                let mut is_active = windows::core::BOOL::default();
                // Also retry via the caret range when a selection existed but
                // its edges could not be read - otherwise a failed TextPattern
                // read would permanently block this fallback.
                if !range_seen || context_edges.is_none() {
                    if let Ok(range) = pattern.GetCaretRange(&mut is_active) {
                        if is_active.as_bool() {
                            range_seen = true;
                            range_collapsed = true;
                            context_edges = read_context_edges(&range, document_range.as_ref());
                            if context_edges
                                .as_ref()
                                .is_some_and(|edges| edges.left_reliable || edges.right_reliable)
                            {
                                source = ContextProbeSource::CaretLocal;
                            } else {
                                source = ContextProbeSource::AmbiguousSelection;
                            }
                        }
                    }
                }
            }

            if !range_seen && value_is_empty == Some(true) {
                source = ContextProbeSource::EmptyField;
                context_edges = Some(ContextEdges {
                    left: String::new(),
                    right: String::new(),
                    left_reliable: true,
                    right_reliable: true,
                });
            }

            let ContextEdges {
                left: context_tail,
                right: context_head,
                mut left_reliable,
                mut right_reliable,
            } = context_edges.unwrap_or(ContextEdges {
                left: String::new(),
                right: String::new(),
                left_reliable: false,
                right_reliable: false,
            });
            if source == ContextProbeSource::CaretLocal && range_collapsed {
                if context_tail.is_empty()
                    && left_reliable
                    && value_text
                        .as_ref()
                        .is_some_and(|value| !value.is_empty() && !value.starts_with(&context_head))
                {
                    left_reliable = false;
                }
                if context_head.is_empty()
                    && right_reliable
                    && value_text
                        .as_ref()
                        .is_some_and(|value| !value.is_empty() && !value.ends_with(&context_tail))
                {
                    right_reliable = false;
                }
            }
            if source == ContextProbeSource::CaretLocal
                && context_tail.is_empty()
                && context_head.is_empty()
            {
                if value_is_empty == Some(true) {
                    source = ContextProbeSource::EmptyField;
                    left_reliable = true;
                    right_reliable = true;
                } else {
                    // Two empty edge strings are not proof of an empty editor.
                    // Providers may return an empty TextPattern range when the
                    // caret read failed. Require independent ValuePattern
                    // confirmation before allowing sentence-start casing.
                    source = ContextProbeSource::AmbiguousSelection;
                    left_reliable = false;
                    right_reliable = false;
                }
            }
            let context = resolve_context_from_tail(
                source == ContextProbeSource::EmptyField,
                (source == ContextProbeSource::CaretLocal && left_reliable)
                    .then_some(context_tail.as_str()),
            );
            let selection_state = describe_selection_state(range_seen, range_collapsed);

            // Structural diagnostics for "blank box reads as full" (Antigravity /
            // Chromium controls). App-control metadata only — never tail content.
            log::debug!(
                "injection: probe detail control_type={control_type} class={class_name} automation_id={automation_id} value_empty={value_is_empty:?} read_only={read_only:?} caret_active={range_seen} collapsed={range_collapsed} source={} context={} tail_len={} head_len={} left_reliable={} right_reliable={} tail_newline={}",
                source.as_str(),
                context.as_str(),
                context_tail.chars().count(),
                context_head.chars().count(),
                left_reliable,
                right_reliable,
                context_tail.contains('\n') || context_tail.contains('\r'),
            );

            InjectionContextProbe {
                context,
                source,
                context_tail,
                context_head,
                left_reliable,
                right_reliable,
                control_type,
                selection_state,
                control_identity_hash,
                target_id,
            }
        }
    }
}

#[cfg_attr(not(windows), allow(dead_code))]
#[cfg(windows)]
pub fn read_focused_text_probe() -> FocusedTextProbe {
    FOCUSED_TEXT_STATE.with(|cell| {
        let mut guard = cell.borrow_mut();
        if guard.com.is_none() {
            guard.com = Some(ComGuard::init());
        }
        let reader = guard.reader.get_or_insert_with(FocusedTextReader::new);
        match reader.as_ref() {
            Some(reader) => reader.read_probe(),
            None => FocusedTextProbe::Unavailable,
        }
    })
}

#[cfg(windows)]
pub fn read_focused_text() -> Option<String> {
    match read_focused_text_probe() {
        FocusedTextProbe::Text(text) => Some(text),
        FocusedTextProbe::NonTextFocus | FocusedTextProbe::Unavailable => None,
    }
}

#[cfg_attr(not(windows), allow(dead_code))]
#[cfg(windows)]
pub fn read_injection_context_probe() -> InjectionContextProbe {
    FOCUSED_TEXT_STATE.with(|cell| {
        let mut guard = cell.borrow_mut();
        if guard.com.is_none() {
            guard.com = Some(ComGuard::init());
        }
        let reader = guard.reader.get_or_insert_with(FocusedTextReader::new);
        match reader.as_ref() {
            Some(reader) => reader.read_injection_context_probe(),
            None => {
                InjectionContextProbe::unavailable(ContextProbeSource::Unavailable, "unavailable")
            }
        }
    })
}

#[cfg(windows)]
static VALUE_CHANGE_HOOK_READY: AtomicBool = AtomicBool::new(false);
#[cfg(windows)]
static VALUE_CHANGE_HOOK_SPAWNED: AtomicBool = AtomicBool::new(false);
#[cfg(windows)]
pub(super) static VALUE_CHANGE_SEQ: AtomicU64 = AtomicU64::new(0);
#[cfg(windows)]
static VALUE_CHANGE_HOOK_THREAD_ID: AtomicU32 = AtomicU32::new(0);
#[cfg(windows)]
static ACTIVE_EVENT_MODE_MONITORS: AtomicUsize = AtomicUsize::new(0);
#[cfg(windows)]
static VALUE_CHANGE_HOOK_STOP_REQUESTED: AtomicBool = AtomicBool::new(false);
#[cfg(windows)]
const WM_APP_AUTO_LEARN_STOP: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 17;

#[cfg(windows)]
pub(super) fn request_value_change_hook_shutdown() {
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::PostThreadMessageW;

    if !VALUE_CHANGE_HOOK_SPAWNED.load(Ordering::SeqCst) {
        return;
    }
    VALUE_CHANGE_HOOK_STOP_REQUESTED.store(true, Ordering::SeqCst);

    let thread_id = VALUE_CHANGE_HOOK_THREAD_ID.load(Ordering::SeqCst);
    if thread_id == 0 {
        return;
    }

    unsafe {
        if PostThreadMessageW(thread_id, WM_APP_AUTO_LEARN_STOP, WPARAM(0), LPARAM(0)).is_err() {
            log::debug!(
                "auto-learn: failed to post stop message to value-change hook thread id={thread_id}"
            );
        }
    }
}

#[cfg(windows)]
unsafe extern "system" fn value_change_event_proc(
    _hook: windows::Win32::UI::Accessibility::HWINEVENTHOOK,
    event: u32,
    hwnd: windows::Win32::Foundation::HWND,
    _id_object: i32,
    _id_child: i32,
    _id_event_thread: u32,
    _event_time: u32,
) {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, IsChild, EVENT_OBJECT_VALUECHANGE,
    };

    if event != EVENT_OBJECT_VALUECHANGE {
        return;
    }
    if hwnd.0.is_null() {
        return;
    }

    let foreground = GetForegroundWindow();
    if foreground.0.is_null() {
        return;
    }

    if hwnd == foreground || IsChild(foreground, hwnd).as_bool() {
        VALUE_CHANGE_SEQ.fetch_add(1, Ordering::Relaxed);
    }
}

#[cfg(windows)]
pub(super) fn ensure_value_change_hook() -> bool {
    if VALUE_CHANGE_HOOK_READY.load(Ordering::Relaxed)
        && !VALUE_CHANGE_HOOK_STOP_REQUESTED.load(Ordering::SeqCst)
    {
        return true;
    }
    if VALUE_CHANGE_HOOK_SPAWNED
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return VALUE_CHANGE_HOOK_READY.load(Ordering::Relaxed)
            && !VALUE_CHANGE_HOOK_STOP_REQUESTED.load(Ordering::SeqCst);
    }

    let (spawned, should_reset_flags) = {
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();

        let spawn_result = std::thread::Builder::new()
            .name("auto_learn_value_change_hook".to_string())
            .spawn(move || unsafe {
                use windows::Win32::System::Threading::GetCurrentThreadId;
                use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent};
                use windows::Win32::UI::WindowsAndMessaging::{
                    DispatchMessageW, GetMessageW, PeekMessageW, TranslateMessage,
                    EVENT_OBJECT_VALUECHANGE, MSG, PM_NOREMOVE, WINEVENT_OUTOFCONTEXT,
                };

                let thread_id = GetCurrentThreadId();
                VALUE_CHANGE_HOOK_THREAD_ID.store(thread_id, Ordering::SeqCst);
                let mut queue_msg = MSG::default();
                let _ = PeekMessageW(&mut queue_msg, None, 0, 0, PM_NOREMOVE);

                let hook = SetWinEventHook(
                    EVENT_OBJECT_VALUECHANGE,
                    EVENT_OBJECT_VALUECHANGE,
                    None,
                    Some(value_change_event_proc),
                    0,
                    0,
                    WINEVENT_OUTOFCONTEXT,
                );

                let ready = !hook.is_invalid();
                let _ = ready_tx.send(ready);
                if !ready {
                    VALUE_CHANGE_HOOK_THREAD_ID.store(0, Ordering::SeqCst);
                    VALUE_CHANGE_HOOK_SPAWNED.store(false, Ordering::Relaxed);
                    VALUE_CHANGE_HOOK_STOP_REQUESTED.store(false, Ordering::SeqCst);
                    return;
                }
                VALUE_CHANGE_HOOK_READY.store(true, Ordering::Relaxed);
                VALUE_CHANGE_HOOK_STOP_REQUESTED.store(false, Ordering::SeqCst);
                if ACTIVE_EVENT_MODE_MONITORS.load(Ordering::SeqCst) == 0 {
                    let _ = UnhookWinEvent(hook);
                    VALUE_CHANGE_HOOK_READY.store(false, Ordering::Relaxed);
                    VALUE_CHANGE_HOOK_SPAWNED.store(false, Ordering::Relaxed);
                    VALUE_CHANGE_HOOK_THREAD_ID.store(0, Ordering::SeqCst);
                    VALUE_CHANGE_HOOK_STOP_REQUESTED.store(false, Ordering::SeqCst);
                    return;
                }

                let mut msg = MSG::default();
                loop {
                    let status = GetMessageW(&mut msg, None, 0, 0).0;
                    if status == -1 {
                        log::error!("GetMessageW failed in auto-learn hook thread");
                        break;
                    }
                    if status == 0 {
                        break;
                    }
                    if msg.message == WM_APP_AUTO_LEARN_STOP {
                        if ACTIVE_EVENT_MODE_MONITORS.load(Ordering::SeqCst) == 0 {
                            break;
                        }
                        VALUE_CHANGE_HOOK_STOP_REQUESTED.store(false, Ordering::SeqCst);
                        continue;
                    }
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }

                let _ = UnhookWinEvent(hook);
                VALUE_CHANGE_HOOK_READY.store(false, Ordering::Relaxed);
                VALUE_CHANGE_HOOK_SPAWNED.store(false, Ordering::Relaxed);
                VALUE_CHANGE_HOOK_THREAD_ID.store(0, Ordering::SeqCst);
                VALUE_CHANGE_HOOK_STOP_REQUESTED.store(false, Ordering::SeqCst);
            });

        match spawn_result {
            Ok(_) => match ready_rx.recv_timeout(std::time::Duration::from_secs(2)) {
                Ok(ready) => (ready, false),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => (false, false),
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => (false, true),
            },
            Err(_) => (false, true),
        }
    };

    if should_reset_flags {
        VALUE_CHANGE_HOOK_SPAWNED.store(false, Ordering::Relaxed);
        VALUE_CHANGE_HOOK_STOP_REQUESTED.store(false, Ordering::SeqCst);
    }
    spawned
}

#[cfg_attr(not(windows), allow(dead_code))]
#[cfg(not(windows))]
pub fn read_focused_text_probe() -> FocusedTextProbe {
    FocusedTextProbe::Unavailable
}

#[cfg(not(windows))]
pub fn read_focused_text() -> Option<String> {
    None
}

#[cfg_attr(not(windows), allow(dead_code))]
#[cfg(not(windows))]
pub fn read_injection_context_probe() -> InjectionContextProbe {
    InjectionContextProbe::unavailable(ContextProbeSource::Unavailable, "unavailable")
}
