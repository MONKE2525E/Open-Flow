#![cfg(target_os = "macos")]

use std::ffi::c_char;

use crate::core::context_probe::{
    resolve_context_from_tail, stable_metadata_hash, ContextProbeSource, InjectionContextProbe,
    SelectionState,
};

const LOOKBEHIND_CHARS: i32 = 64;

const SOURCE_CARET_LOCAL: i32 = 0;
const SOURCE_EMPTY_FIELD: i32 = 1;
const SOURCE_AMBIGUOUS_SELECTION: i32 = 2;
const SOURCE_PERMISSION_MISSING: i32 = 3;
const SOURCE_UNSUPPORTED_CONTROL: i32 = 4;
const SOURCE_UNAVAILABLE: i32 = 5;

const SELECTION_COLLAPSED: i32 = 0;
const SELECTION_NON_COLLAPSED: i32 = 1;
const SELECTION_UNKNOWN: i32 = 2;

#[repr(C)]
struct MacosContextProbeResult {
    source: i32,
    selection_state: i32,
    pid: i32,
    control_type: [c_char; 64],
    role: [c_char; 64],
    subrole: [c_char; 64],
    identifier: [c_char; 128],
    title: [c_char; 160],
    tail: [c_char; 256],
}

unsafe extern "C" {
    fn verenu_macos_read_context_probe(
        lookbehind_chars: i32,
        out_result: *mut MacosContextProbeResult,
    ) -> i32;
}

fn c_buf_to_string(buf: &[c_char]) -> String {
    // Find the null terminator within the buffer bounds to avoid a buffer
    // overread if the Objective-C shim fails to write a trailing null byte.
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    let u8_bytes = unsafe { std::slice::from_raw_parts(buf.as_ptr() as *const u8, len) };
    String::from_utf8_lossy(u8_bytes).trim().to_string()
}

fn map_source(source: i32) -> ContextProbeSource {
    match source {
        SOURCE_CARET_LOCAL => ContextProbeSource::CaretLocal,
        SOURCE_EMPTY_FIELD => ContextProbeSource::EmptyField,
        SOURCE_AMBIGUOUS_SELECTION => ContextProbeSource::AmbiguousSelection,
        SOURCE_PERMISSION_MISSING => ContextProbeSource::PermissionMissing,
        SOURCE_UNSUPPORTED_CONTROL => ContextProbeSource::UnsupportedControl,
        SOURCE_UNAVAILABLE => ContextProbeSource::Unavailable,
        _ => ContextProbeSource::Unavailable,
    }
}

fn map_selection_state(selection_state: i32) -> SelectionState {
    match selection_state {
        SELECTION_COLLAPSED => SelectionState::CollapsedCaret,
        SELECTION_NON_COLLAPSED => SelectionState::NonCollapsedSelection,
        SELECTION_UNKNOWN => SelectionState::Unknown,
        _ => SelectionState::Unknown,
    }
}

pub fn read_injection_context_probe_sync() -> InjectionContextProbe {
    let mut raw = MacosContextProbeResult {
        source: SOURCE_UNAVAILABLE,
        selection_state: SELECTION_UNKNOWN,
        pid: 0,
        control_type: [0; 64],
        role: [0; 64],
        subrole: [0; 64],
        identifier: [0; 128],
        title: [0; 160],
        tail: [0; 256],
    };

    // SAFETY: The shim fills the provided POD struct and does not retain Rust memory.
    let ok = unsafe { verenu_macos_read_context_probe(LOOKBEHIND_CHARS, &mut raw) };
    if ok == 0 {
        return InjectionContextProbe::unavailable(ContextProbeSource::Unavailable, "ffi_failed");
    }

    let source = map_source(raw.source);
    let selection_state = map_selection_state(raw.selection_state);
    let control_type = c_buf_to_string(&raw.control_type);
    let role = c_buf_to_string(&raw.role);
    let subrole = c_buf_to_string(&raw.subrole);
    let identifier = c_buf_to_string(&raw.identifier);
    let title = c_buf_to_string(&raw.title);
    let tail = c_buf_to_string(&raw.tail);
    let pid = raw.pid.to_string();
    let context = resolve_context_from_tail(source == ContextProbeSource::EmptyField, Some(&tail));
    let control_identity_hash = stable_metadata_hash(&[
        pid.as_str(),
        role.as_str(),
        subrole.as_str(),
        identifier.as_str(),
        title.as_str(),
    ]);

    InjectionContextProbe {
        context,
        source,
        context_tail: tail,
        selection_state,
        control_identity_hash,
        control_type: if control_type.is_empty() {
            "unknown".to_string()
        } else {
            control_type
        },
    }
}
