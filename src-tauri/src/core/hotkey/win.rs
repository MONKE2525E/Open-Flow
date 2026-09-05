use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

const VK_BACK: u32 = 0x08; // Backspace
const VK_ESCAPE: u32 = 0x1B; // Escape
const VK_SPACE: u32 = 0x20; // Spacebar
const VK_SHIFT: u32 = 0x10; // VK_SHIFT
const VK_CTRL: u32 = 0x11; // VK_CONTROL (generic, used with modifier_held)
const VK_ALT: u32 = 0x12; // VK_MENU (generic, used with modifier_held)
const VK_RETURN: u32 = 0x0D; // Enter
const VK_C: u32 = 0x43; // 'C' — used by the Ctrl+Alt+C copy-last-dictation shortcut

// Side-specific modifier VK codes that should never trigger a history update.
// Generic codes (0x10/0x11/0x12) are omitted: the !is_injected guard already
// filters all synthetic input where those codes appear, so only side-specific
// codes reach this path from real physical key presses.
static MODIFIER_VKS: &[u32] = &[
    0xA0, 0xA1, // VK_LSHIFT, VK_RSHIFT
    0xA2, 0xA3, // VK_LCONTROL, VK_RCONTROL
    0xA4, 0xA5, // VK_LMENU, VK_RMENU
    0x5B, 0x5C, // VK_LWIN, VK_RWIN
    0x14, 0x90, 0x91, // VK_CAPITAL, VK_NUMLOCK, VK_SCROLL
];
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::SystemInformation::GetTickCount64;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, GetKeyState, GetKeyboardLayout, MapVirtualKeyExW, RegisterHotKey,
    ToUnicodeEx, UnregisterHotKey, HOT_KEY_MODIFIERS, MAPVK_VK_TO_VSC, MOD_ALT, MOD_CONTROL,
    MOD_SHIFT, MOD_WIN,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetForegroundWindow, GetMessageW, GetWindowThreadProcessId,
    SetWindowsHookExW, TranslateMessage, HC_ACTION, KBDLLHOOKSTRUCT, LLKHF_INJECTED, MSG,
    WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

// Returns true if the given specific-side VK (or its mirror) is currently held.
// Uses generic VKs for Shift/Ctrl/Alt so either side satisfies the check.
// Win key has no generic VK, so both sides are checked explicitly.
unsafe fn modifier_held(vk: u32) -> bool {
    let held = |v: u32| -> bool { (GetAsyncKeyState(v as i32) & 0x8000u16 as i16) != 0 };
    match vk {
        160 | 161 => held(16),           // L/RShift -> VK_SHIFT
        162 | 163 => held(17),           // L/RControl -> VK_CONTROL
        164 | 165 => held(18),           // L/RMenu -> VK_MENU
        91 | 92 => held(91) || held(92), // LWin / RWin (no generic VK_WIN)
        _ => held(vk),
    }
}

// Returns true if the hook vkCode matches the configured key, including the
// mirror side for modifiers (so LCtrl binding also matches RCtrl events).
fn vk_matches(vk: u32, key: u32) -> bool {
    match key {
        160 | 161 => vk == 160 || vk == 161,
        162 | 163 => vk == 162 || vk == 163,
        164 | 165 => vk == 164 || vk == 165,
        91 | 92 => vk == 91 || vk == 92,
        _ => vk == key,
    }
}

fn is_cursor_movement_key(vk: u32) -> bool {
    matches!(
        vk,
        0x21..=0x28 | // PgUp, PgDn, End, Home, Left, Up, Right, Down
        0x2D |        // Insert
        0x2E // Delete (forward)
    )
}

#[inline]
fn map_vk_to_scan_code(vk: u32, layout: windows::Win32::UI::Input::KeyboardAndMouse::HKL) -> u32 {
    // windows crate (0.61.x) wraps MapVirtualKeyExW with Option<HKL>.
    unsafe { MapVirtualKeyExW(vk, MAPVK_VK_TO_VSC, Some(layout)) }
}

#[inline]
fn to_unicode_layout(
    vk: u32,
    scan: u32,
    state: &[u8; 256],
    buff: &mut [u16],
    layout: windows::Win32::UI::Input::KeyboardAndMouse::HKL,
) -> i32 {
    // windows crate (0.61.x) wrapper derives cchBuff from buff.len().
    unsafe { ToUnicodeEx(vk, scan, state, buff, 0, Some(layout)) }
}

/// Maps a VK code to the character it produces (US QWERTY layout).
/// Letters are always returned lowercase - case doesn't affect sentence-ender
/// or whitespace checks downstream. Returns None for keys with no stable
/// printable character (numpad, function keys, etc.); those reset history.
fn vk_to_char(vk: u32) -> Option<char> {
    if vk == VK_RETURN {
        return Some('\n');
    }
    if vk == 0x20 {
        return Some(' ');
    }

    unsafe {
        let mut state = [0u8; 256];
        if GetAsyncKeyState(0xA0) < 0 {
            state[0xA0] |= 0x80; // VK_LSHIFT
            state[0x10] |= 0x80; // VK_SHIFT
        }
        if GetAsyncKeyState(0xA1) < 0 {
            state[0xA1] |= 0x80; // VK_RSHIFT
            state[0x10] |= 0x80;
        }
        if GetAsyncKeyState(0xA2) < 0 {
            state[0xA2] |= 0x80; // VK_LCONTROL
            state[0x11] |= 0x80; // VK_CONTROL
        }
        if GetAsyncKeyState(0xA3) < 0 {
            state[0xA3] |= 0x80; // VK_RCONTROL
            state[0x11] |= 0x80;
        }
        if GetAsyncKeyState(0xA4) < 0 {
            state[0xA4] |= 0x80; // VK_LMENU
            state[0x12] |= 0x80; // VK_MENU
        }
        if GetAsyncKeyState(0xA5) < 0 {
            state[0xA5] |= 0x80; // VK_RMENU (AltGr)
            state[0x12] |= 0x80;
        }
        // Caps Lock is a toggle key; low-order bit indicates toggle state.
        if (GetKeyState(0x14) & 0x0001) != 0 {
            state[0x14] |= 0x01;
        }

        let foreground = GetForegroundWindow();
        let thread_id = GetWindowThreadProcessId(foreground, None);
        let layout = GetKeyboardLayout(thread_id);
        let scan = map_vk_to_scan_code(vk, layout);
        if scan == 0 {
            return None;
        }

        let mut buff = [0u16; 8];
        let rc = to_unicode_layout(vk, scan, &state, &mut buff, layout);
        if rc < 0 {
            // Flush dead-key compose state with a neutral key so subsequent
            // translations are not polluted by stale composition state.
            let neutral_vk = 0x20u32; // VK_SPACE
            let neutral_scan = map_vk_to_scan_code(neutral_vk, layout);
            for _ in 0..4 {
                if to_unicode_layout(neutral_vk, neutral_scan, &state, &mut buff, layout) >= 0 {
                    break;
                }
            }
            return None;
        }
        if rc == 0 {
            return None;
        }

        let s = String::from_utf16_lossy(&buff[..rc as usize]);
        s.chars()
            .next()
            .map(|ch| ch.to_lowercase().next().unwrap_or(ch))
    }
}

pub fn is_hotkey_available(key1: &str, key2: &str) -> bool {
    let mod_flag = match key1 {
        "ShiftLeft" | "ShiftRight" => MOD_SHIFT,
        "ControlLeft" | "ControlRight" => MOD_CONTROL,
        "AltLeft" | "AltRight" => MOD_ALT,
        "MetaLeft" | "MetaRight" => MOD_WIN,
        _ => return true,
    };

    let vk2 = map_code_to_vk(key2);
    if vk2 == 0 {
        return true;
    }

    unsafe {
        // Use a dummy ID (e.g. 0x5A8E) and no HWND.
        if RegisterHotKey(None, 0x5A8E, HOT_KEY_MODIFIERS(mod_flag.0), vk2).is_ok() {
            let _ = UnregisterHotKey(None, 0x5A8E);
            true
        } else {
            false
        }
    }
}

pub fn is_repair_hotkey_available(key1: &str, key2: &str, key3: &str) -> bool {
    let modifier_flags = [key1, key2].iter().try_fold(0u32, |flags, key| {
        let flag = match *key {
            "ShiftLeft" | "ShiftRight" => MOD_SHIFT.0,
            "ControlLeft" | "ControlRight" => MOD_CONTROL.0,
            "AltLeft" | "AltRight" => MOD_ALT.0,
            "MetaLeft" | "MetaRight" => MOD_WIN.0,
            _ => return None,
        };
        Some(flags | flag)
    });
    let Some(modifier_flags) = modifier_flags else {
        return true;
    };
    let vk = map_code_to_vk(key3);
    if vk == 0 {
        return true;
    }
    unsafe {
        if RegisterHotKey(None, 0x5A8F, HOT_KEY_MODIFIERS(modifier_flags), vk).is_ok() {
            let _ = UnregisterHotKey(None, 0x5A8F);
            true
        } else {
            false
        }
    }
}

static KEY1: AtomicU32 = AtomicU32::new(162); // VK_LCONTROL / Ctrl
static KEY2: AtomicU32 = AtomicU32::new(91); // VK_LWIN / Windows

const CHORD_TAP_MAX_HOLD_MS: u64 = 200;
const CHORD_DOUBLE_TAP_WINDOW_MS: u64 = 300;

fn is_menu_trigger_vk(vk: u32) -> bool {
    matches!(vk, 164 | 165 | 18 | 91 | 92)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ChordKey {
    Key1,
    Key2,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum KeyEdge {
    Down,
    Up,
}

// The shared "Fire" prefix reads clearly as "fire this callback" at each call
// site; not worth losing that for clippy's glob-import naming heuristic.
#[allow(clippy::enum_variant_names)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ChordAction {
    FirePress,
    FireRelease,
    FireCancel,
    FireHandless,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum KeyDisposition {
    Suppress,
    Passthrough,
}

#[derive(Clone, Copy, Debug)]
struct ChordOutcome {
    action: Option<ChordAction>,
    disposition: KeyDisposition,
}

impl ChordOutcome {
    fn suppress(action: Option<ChordAction>) -> Self {
        Self {
            action,
            disposition: KeyDisposition::Suppress,
        }
    }
    fn passthrough() -> Self {
        Self {
            action: None,
            disposition: KeyDisposition::Passthrough,
        }
    }
}

#[derive(Default, Clone, Copy, PartialEq, Eq, Debug)]
enum TapState {
    #[default]
    None,
    WaitingForFullRelease,
    AwaitingSecondTap {
        deadline_start_ms: u64,
    },
}

/// Chord/handsfree gesture tracking. Owned exclusively by the hook thread
/// (accessed only via the `CHORD_MACHINE` thread-local below) so it never
/// needs locking inside `hook_proc`, which must return quickly or Windows
/// silently unhooks it.
#[derive(Default)]
struct ChordStateMachine {
    key1_down: bool,
    key2_down: bool,
    key2_passed_through: bool,
    key1_passed_through: bool,
    key1_was_chord: bool,
    key2_was_chord: bool,
    chord_down: bool,
    chord_first_down_ms: u64,
    tap: TapState,
    space_down: bool,
    space_passed_through: bool,
    handless_from_chord: bool,
}

impl ChordStateMachine {
    fn key_down_mut(&mut self, key: ChordKey) -> &mut bool {
        match key {
            ChordKey::Key1 => &mut self.key1_down,
            ChordKey::Key2 => &mut self.key2_down,
        }
    }
    fn key_passed_through_mut(&mut self, key: ChordKey) -> &mut bool {
        match key {
            ChordKey::Key1 => &mut self.key1_passed_through,
            ChordKey::Key2 => &mut self.key2_passed_through,
        }
    }
    fn mark_key_passed_through(&mut self, key: ChordKey) {
        *self.key_passed_through_mut(key) = true;
    }
    /// Corrects stale ownership bookkeeping against live OS key state. A
    /// keyup can occasionally never reach this hook (e.g. swallowed by
    /// another low-level hook, or eaten by the OS's own Start-menu handling
    /// of a bare Win press) which leaves `key_down` stuck true forever —
    /// after that, the next lone press of the *other* key looks like a
    /// chord-forming edge and fires dictation off a single key. Called with
    /// the live `GetAsyncKeyState` read for the *other* key (never the one
    /// whose edge is currently being processed — its own down/up handling
    /// already reconciles itself).
    fn reconcile_stale_key(&mut self, key: ChordKey, os_held: bool) {
        if os_held || !*self.key_down_mut(key) {
            return;
        }
        *self.key_down_mut(key) = false;
        *self.key_passed_through_mut(key) = false;
        self.set_key_was_chord(key, false);
        if !self.key1_down && !self.key2_down {
            self.chord_down = false;
            self.handless_from_chord = false;
        }
    }

    fn key_was_chord(&self, key: ChordKey) -> bool {
        match key {
            ChordKey::Key1 => self.key1_was_chord,
            ChordKey::Key2 => self.key2_was_chord,
        }
    }
    fn set_key_was_chord(&mut self, key: ChordKey, v: bool) {
        match key {
            ChordKey::Key1 => self.key1_was_chord = v,
            ChordKey::Key2 => self.key2_was_chord = v,
        }
    }

    /// Clears gesture/timing state only — never physical-key or ownership
    /// state. A full reset here would forget a currently-suppressed chord's
    /// keys are still Verenu-owned, letting a bare Ctrl-up/Win-up leak to the
    /// OS (Start menu) if a reset lands between a chord's two keyups.
    fn reset_gesture_state(&mut self) {
        self.chord_down = false;
        self.chord_first_down_ms = 0;
        self.tap = TapState::None;
        self.space_down = false;
        self.space_passed_through = false;
        self.handless_from_chord = false;
    }

    fn on_key_event(&mut self, key: ChordKey, edge: KeyEdge, now_ms: u64) -> ChordOutcome {
        match edge {
            KeyEdge::Down => self.on_key_down(key, now_ms),
            KeyEdge::Up => self.on_key_up(key, now_ms),
        }
    }

    fn on_key_down(&mut self, key: ChordKey, now_ms: u64) -> ChordOutcome {
        if !self.key1_down && !self.key2_down {
            self.handless_from_chord = false;
        }

        let was_down = std::mem::replace(self.key_down_mut(key), true);
        if was_down {
            // Autorepeat. If this key is chord-owned, Verenu already claimed
            // it and must keep suppressing it (this is the actual fix for the
            // original bug: a handsfree trigger deliberately leaves
            // `chord_down` false while the keys may still be held, so without
            // this explicit already-down check a repeat could otherwise look
            // like a fresh edge and re-enter chord-formed handling). If it's
            // not chord-owned, it was never ours to begin with.
            return if self.key_was_chord(key) {
                ChordOutcome::suppress(None)
            } else {
                ChordOutcome::passthrough()
            };
        }

        if !(self.key1_down && self.key2_down) {
            self.mark_key_passed_through(key);
            // Only one key down so far — not our gesture yet, let it through
            // untouched (so a lone Ctrl or Win press still behaves normally).
            return ChordOutcome::passthrough();
        }

        // Chord-formed edge: both keys just became down together, regardless
        // of press order. This is the second key's down-edge; the first
        // already passed through above.
        self.key1_was_chord = true;
        self.key2_was_chord = true;

        if self.chord_down {
            // Re-formation, not a new press — reclaim ownership and stop.
            //
            // A chord-forming edge normally implies a key went up and back
            // down, and that keyup would have cleared `chord_down`. So finding
            // it still set means something cleared a key's bookkeeping while it
            // was still physically held: `reconcile_stale_key` doing its job on
            // a keyup the hook never received, or `force_release_win_key`
            // wiping all of it before a paste. The key then autorepeats (it IS
            // still down), and that repeat arrives here looking like a fresh
            // chord.
            //
            // Restoring ownership above is right. Restarting the hold clock is
            // not: `held_ms` in on_key_up is measured from `chord_first_down_ms`,
            // so a chord held for seconds would be measured from this repeat,
            // come out under CHORD_TAP_MAX_HOLD_MS, and be classified as a tap
            // — firing FireCancel and throwing the dictation away. That is the
            // "every dictation gets cancelled" bug.
            return ChordOutcome::suppress(None);
        }

        self.chord_first_down_ms = now_ms;

        if let TapState::AwaitingSecondTap { deadline_start_ms } = self.tap {
            if now_ms.saturating_sub(deadline_start_ms) <= CHORD_DOUBLE_TAP_WINDOW_MS {
                self.tap = TapState::None;
                // Handsfree is a discrete toggle, not a held chord — leave
                // chord_down false so the user's fingers coming off both keys
                // afterward needs no further chord bookkeeping here.
                return ChordOutcome::suppress(Some(ChordAction::FireHandless));
            }
        }

        self.tap = TapState::None;
        self.chord_down = true;
        ChordOutcome::suppress(Some(ChordAction::FirePress))
    }

    fn on_space_event(&mut self, edge: KeyEdge) -> ChordOutcome {
        match edge {
            KeyEdge::Down => {
                if self.space_down {
                    return if self.space_passed_through {
                        ChordOutcome::passthrough()
                    } else if self.handless_from_chord {
                        ChordOutcome::suppress(None)
                    } else {
                        ChordOutcome::passthrough()
                    };
                }

                if self.chord_down && !self.handless_from_chord {
                    self.space_down = true;
                    self.space_passed_through = false;
                    self.chord_down = false;
                    self.tap = TapState::None;
                    self.handless_from_chord = true;
                    ChordOutcome::suppress(Some(ChordAction::FireHandless))
                } else if self.handless_from_chord {
                    self.space_down = true;
                    self.space_passed_through = false;
                    ChordOutcome::suppress(None)
                } else {
                    self.space_down = true;
                    self.space_passed_through = true;
                    ChordOutcome::passthrough()
                }
            }
            KeyEdge::Up => {
                let was_down = std::mem::replace(&mut self.space_down, false);
                let passed_through = std::mem::replace(&mut self.space_passed_through, false);
                if was_down && !passed_through {
                    ChordOutcome::suppress(None)
                } else {
                    ChordOutcome::passthrough()
                }
            }
        }
    }

    fn on_key_up(&mut self, key: ChordKey, now_ms: u64) -> ChordOutcome {
        let _was_down = std::mem::replace(self.key_down_mut(key), false);
        let key_passed_through = std::mem::replace(self.key_passed_through_mut(key), false);

        if !self.key_was_chord(key) {
            // Never claimed as part of a chord — always pass through, or an
            // ordinary Ctrl/Windows release could get silently swallowed.
            return ChordOutcome::passthrough();
        }
        self.set_key_was_chord(key, false);

        let mut action = None;
        if self.chord_down {
            self.chord_down = false;
            let held_ms = now_ms.saturating_sub(self.chord_first_down_ms);
            if held_ms >= CHORD_TAP_MAX_HOLD_MS {
                action = Some(ChordAction::FireRelease);
            } else {
                action = Some(ChordAction::FireCancel);
                self.tap = TapState::WaitingForFullRelease;
            }
        }

        // The double-tap clock starts at the first release following a quick
        // chord tap. This deliberately does NOT require the *other* key to
        // also be up: holding one key down continuously and quick-tapping the
        // other twice (e.g. hold Ctrl, double-click Win) must also arm the
        // second tap, since a remapped mouse button can only ever send taps
        // of a single key, never hold one key while tapping another. When
        // both keys happen to release together (the classic "double-tap the
        // whole chord" gesture) this still starts the window at the first of
        // the two releases, which are normally only a few ms apart.
        if matches!(self.tap, TapState::WaitingForFullRelease) {
            self.tap = TapState::AwaitingSecondTap {
                deadline_start_ms: now_ms,
            };
        }

        if !self.key1_down && !self.key2_down {
            self.handless_from_chord = false;
        }

        if key_passed_through {
            ChordOutcome {
                action,
                disposition: KeyDisposition::Passthrough,
            }
        } else {
            ChordOutcome::suppress(action)
        }
    }
}

thread_local! {
    static CHORD_MACHINE: std::cell::RefCell<ChordStateMachine> =
        std::cell::RefCell::new(ChordStateMachine::default());
}

// Cross-thread reset request: `reset_chord_state()` is called from the async
// pipeline task, which cannot reach the hook thread's thread-local directly.
static RESET_REQUESTED: AtomicBool = AtomicBool::new(false);

// Cross-thread request to force both chord keys' down/ownership bookkeeping
// to false, regardless of gesture state. Unlike RESET_REQUESTED (gesture
// state only — see reset_gesture_state's own doc comment on why it never
// touches physical-key state), this is a deliberately blunter reset: it's
// only ever set by force_release_win_key(), after GetAsyncKeyState has
// already confirmed the Win key reads stuck down at the OS level, so there's
// no risk of prematurely forgetting a legitimately-still-suppressed key.
static FORCE_KEY_RELEASE_REQUESTED: AtomicBool = AtomicBool::new(false);

pub fn update_keys(k1: u32, k2: u32) {
    if k1 != 0 {
        KEY1.store(k1, Ordering::SeqCst);
    }
    if k2 != 0 {
        KEY2.store(k2, Ordering::SeqCst);
    }
    if PRESS_CB.get().is_some() {
        return;
    }
    reset_chord_state();
}

// Requests that mid-chord gesture/timing state be cleared. Called from the
// Tokio handler after a handsfree stop-via-cancel so the still-open
// double-tap window can't accidentally start a fresh handsfree session on a
// stray second key press. Applied by the hook thread at the top of its next
// invocation (only gesture/timing state — see `reset_gesture_state`).
pub fn reset_chord_state() {
    RESET_REQUESTED.store(true, Ordering::SeqCst);
}

pub fn set_handless_active(v: bool) {
    HANDLESS_ACTIVE.store(v, Ordering::SeqCst);
}

// 0 = not processing. Set once, at Stopping -> Processing; cleared via
// compare-exchange (a no-op if the generation doesn't match, so a stale,
// superseded task's cleanup can never clobber a newer generation's flag).
static PROCESSING_GENERATION: AtomicU64 = AtomicU64::new(0);

pub fn set_processing_generation(generation: u64) {
    PROCESSING_GENERATION.store(generation, Ordering::SeqCst);
}

pub fn clear_processing_generation(expected_generation: u64) {
    let _ = PROCESSING_GENERATION.compare_exchange(
        expected_generation,
        0,
        Ordering::SeqCst,
        Ordering::SeqCst,
    );
}

/// Current Caps Lock toggle state, tracked from the hook thread.
pub fn caps_lock_is_on() -> bool {
    CAPS_LOCK_ON.load(Ordering::SeqCst)
}

/// Live OS-level check (not our own bookkeeping) — true if Windows currently
/// thinks either Win key is held. Reuses the same `GetAsyncKeyState` primitive
/// `modifier_held` already uses for Win (VK 91/92, no generic VK_WIN).
pub fn is_win_key_down() -> bool {
    unsafe { modifier_held(91) }
}

/// Recovery action for a stuck Win key (confirmed via `is_win_key_down()`
/// first) — called right before a paste so a leftover "Win held" OS state
/// can't turn the paste's Ctrl+V into a Win-shortcut. Synthesizes a real
/// keyup for both Win keys (Windows should honor this as authoritative
/// regardless of why its internal state was wrong) and asks the hook thread
/// to forget its own chord-key ownership bookkeeping too, in case the hook
/// itself still thinks it's holding/suppressing the key.
pub fn force_release_win_key() {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
        KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, VK_LWIN, VK_RWIN,
    };

    // Win is an "extended key" per SendInput's own contract — omitting
    // KEYEVENTF_EXTENDEDKEY can leave the OS's shell-hotkey state machine
    // out of sync even when GetAsyncKeyState reports the key up.
    let ki = |vk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: KEYBD_EVENT_FLAGS(KEYEVENTF_KEYUP.0 | KEYEVENTF_EXTENDEDKEY.0),
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    let release = [ki(VK_LWIN), ki(VK_RWIN)];
    unsafe { SendInput(&release, std::mem::size_of::<INPUT>() as i32) };

    FORCE_KEY_RELEASE_REQUESTED.store(true, Ordering::SeqCst);
}

pub fn map_code_to_vk(code: &str) -> u32 {
    match code {
        "ShiftLeft" => 160,
        "ShiftRight" => 161,
        "ControlLeft" => 162,
        "ControlRight" => 163,
        "AltLeft" => 164,
        "AltRight" => 165,
        "MetaLeft" => 91,
        "MetaRight" => 92,
        "Space" => 32,
        "Escape" => 27,
        "Enter" => VK_RETURN,
        "Backspace" => 8,
        "Tab" => 9,
        "CapsLock" => 20,
        "Minus" => 189,
        "Equal" => 187,
        "BracketLeft" => 219,
        "BracketRight" => 221,
        "Backslash" => 220,
        "Semicolon" => 186,
        "Quote" => 222,
        "Comma" => 188,
        "Period" => 190,
        "Slash" => 191,
        "Backquote" => 192,
        "ArrowUp" => 38,
        "ArrowDown" => 40,
        "ArrowLeft" => 37,
        "ArrowRight" => 39,
        "Insert" => 45,
        "Delete" => 46,
        "Home" => 36,
        "End" => 35,
        "PageUp" => 33,
        "PageDown" => 34,
        c if c.starts_with("Key") && c.len() == 4 => c.as_bytes()[3] as u32,
        c if c.starts_with("Digit") && c.len() == 6 => c.as_bytes()[5] as u32,
        c if c.starts_with("F") && c.len() > 1 => {
            if let Ok(n) = c[1..].parse::<u32>() {
                if (1..=12).contains(&n) {
                    111 + n
                } else {
                    0
                }
            } else {
                0
            }
        }
        c if c.starts_with("Numpad") && c.len() == 7 => {
            let b = c.as_bytes()[6];
            if b.is_ascii_digit() {
                96 + (b - b'0') as u32
            } else {
                0
            }
        }
        "NumpadMultiply" => 106,
        "NumpadAdd" => 107,
        "NumpadSubtract" => 109,
        "NumpadDecimal" => 110,
        "NumpadDivide" => 111,
        _ => 0,
    }
}

static PRESS_CB: std::sync::OnceLock<Box<dyn Fn() + Send + Sync>> = std::sync::OnceLock::new();
static RELEASE_CB: std::sync::OnceLock<Box<dyn Fn() + Send + Sync>> = std::sync::OnceLock::new();
static HANDLESS_CB: std::sync::OnceLock<Box<dyn Fn() + Send + Sync>> = std::sync::OnceLock::new();
static CANCEL_CB: std::sync::OnceLock<Box<dyn Fn() + Send + Sync>> = std::sync::OnceLock::new();

// Seeded once at hook-thread startup and updated on every hook callback (see
// `start` and `hook_proc`) - both writes happen on the hook's dedicated
// message-pumping thread, where `GetKeyState`'s toggle bit is reliably in
// sync. Querying `GetKeyState` directly from elsewhere (e.g. the Tokio
// pipeline thread, which has no message queue of its own) would not reflect
// real toggle state. Backs `caps_lock_is_on()` for the optional caps-lock
// output-uppercasing setting.
static CAPS_LOCK_ON: AtomicBool = AtomicBool::new(false);

static HANDLESS_ACTIVE: AtomicBool = AtomicBool::new(false);
static ESCAPE_CANCELLED: AtomicBool = AtomicBool::new(false);
static ESCAPE_KEY_DOWN: AtomicBool = AtomicBool::new(false);
static ESCAPE_CB: std::sync::OnceLock<Box<dyn Fn() + Send + Sync>> = std::sync::OnceLock::new();

// Ctrl+Alt+C: always-available fallback to re-copy the last dictation to the
// clipboard, in case paste failed in a way the pipeline's own detection
// missed. Not chord-based (no hold/release) — a plain keydown fires it,
// mirroring how Escape is handled below.
static COPY_LAST_KEY_DOWN: AtomicBool = AtomicBool::new(false);
static COPY_LAST_CB: std::sync::OnceLock<Box<dyn Fn() + Send + Sync>> = std::sync::OnceLock::new();

// User-configurable "open the repair complaint box" hotkey. Two modifiers
// plus one regular trigger key (default Ctrl+Alt+Z) — deliberately NOT a
// two-key modifier-only chord like the main hotkey. An earlier version let
// this be set to e.g. Ctrl+Alt alone and suppressed (return LRESULT(1))
// *either* configured key's own down/up the moment it matched, before the
// main chord's ChordStateMachine ever saw it — so a bare Ctrl press (half of
// the default main hotkey) got eaten outright, breaking dictation entirely.
// This mirrors the existing Ctrl+Alt+C copy-last shortcut instead: only the
// trigger key's own event is ever intercepted, and only once GetAsyncKeyState
// confirms both modifiers are already down — bare Ctrl/Alt presses, and every
// other shortcut built on them (including the main hotkey), are untouched.
static REPAIR_MOD1: AtomicU32 = AtomicU32::new(162); // Ctrl
static REPAIR_MOD2: AtomicU32 = AtomicU32::new(164); // Alt
static REPAIR_TRIGGER: AtomicU32 = AtomicU32::new(0x5A); // 'Z'
static REPAIR_TRIGGER_DOWN: AtomicBool = AtomicBool::new(false);
static REPAIR_OPEN_CB: std::sync::OnceLock<Box<dyn Fn() + Send + Sync>> =
    std::sync::OnceLock::new();

pub fn update_repair_keys(mod1: u32, mod2: u32, trigger: u32) {
    REPAIR_MOD1.store(mod1, Ordering::SeqCst);
    REPAIR_MOD2.store(mod2, Ordering::SeqCst);
    REPAIR_TRIGGER.store(trigger, Ordering::SeqCst);
    REPAIR_TRIGGER_DOWN.store(false, Ordering::SeqCst);
}

unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let kb = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        let msg = wparam.0 as u32;
        let vk = kb.vkCode;

        // This thread pumps messages (see `start`'s GetMessageW loop), so
        // GetKeyState's toggle bit is reliably in sync here.
        CAPS_LOCK_ON.store((GetKeyState(0x14) & 0x0001) != 0, Ordering::SeqCst);

        let k1 = KEY1.load(Ordering::Relaxed);
        let k2 = KEY2.load(Ordering::Relaxed);

        let is_key2 = vk_matches(vk, k2);
        let is_key1 = vk_matches(vk, k1);
        let is_down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
        let is_up = msg == WM_KEYUP || msg == WM_SYSKEYUP;

        // Repair-open hotkey (Ctrl+Alt+Z by default): only the trigger key's
        // own event is ever intercepted, exactly like the Ctrl+Alt+C
        // copy-last shortcut below — bare modifier presses are never touched,
        // so this can never eat half of the main hotkey (see the doc comment
        // on REPAIR_MOD1 for the bug this replaced).
        if vk == REPAIR_TRIGGER.load(Ordering::Relaxed) && (is_down || is_up) {
            let mods_held = modifier_held(REPAIR_MOD1.load(Ordering::Relaxed))
                && modifier_held(REPAIR_MOD2.load(Ordering::Relaxed));
            if is_down && mods_held {
                if !REPAIR_TRIGGER_DOWN.swap(true, Ordering::SeqCst) {
                    if let Some(cb) = REPAIR_OPEN_CB.get() {
                        cb();
                    }
                }
                return LRESULT(1);
            }
            if is_up && REPAIR_TRIGGER_DOWN.swap(false, Ordering::SeqCst) && mods_held {
                return LRESULT(1);
            }
        }

        if RESET_REQUESTED.swap(false, Ordering::SeqCst) {
            CHORD_MACHINE.with(|m| m.borrow_mut().reset_gesture_state());
            ESCAPE_CANCELLED.store(false, Ordering::SeqCst);
            ESCAPE_KEY_DOWN.store(false, Ordering::SeqCst);
        }

        if FORCE_KEY_RELEASE_REQUESTED.swap(false, Ordering::SeqCst) {
            CHORD_MACHINE.with(|m| {
                let mut machine = m.borrow_mut();
                machine.key1_down = false;
                machine.key2_down = false;
                machine.key1_passed_through = false;
                machine.key2_passed_through = false;
                machine.key1_was_chord = false;
                machine.key2_was_chord = false;
                machine.chord_down = false;
            });
        }

        if (is_key1 || is_key2) && (is_down || is_up) {
            let key = if vk == k1 {
                ChordKey::Key1
            } else if vk == k2 {
                ChordKey::Key2
            } else if is_key1 {
                ChordKey::Key1
            } else {
                ChordKey::Key2
            };
            let edge = if is_down { KeyEdge::Down } else { KeyEdge::Up };
            let now = GetTickCount64();

            // Live OS truth for whichever key this event is NOT about — never
            // the key whose own edge we're processing, since its bookkeeping
            // is about to be updated by on_key_event itself.
            let (other_key, other_os_held) = if key == ChordKey::Key1 {
                (ChordKey::Key2, unsafe { modifier_held(k2) })
            } else {
                (ChordKey::Key1, unsafe { modifier_held(k1) })
            };

            let (outcome, key1_was_passed_through, key2_was_passed_through) =
                CHORD_MACHINE.with(|m| {
                    let mut machine = m.borrow_mut();
                    machine.reconcile_stale_key(other_key, other_os_held);
                    let key1_was_passed_through =
                        key == ChordKey::Key2 && machine.key1_passed_through;
                    let key2_was_passed_through =
                        key == ChordKey::Key1 && machine.key2_passed_through;
                    let outcome = machine.on_key_event(key, edge, now);
                    (outcome, key1_was_passed_through, key2_was_passed_through)
                });
            let mut action = outcome.action;
            let mut disposition = outcome.disposition;

            if matches!(
                action,
                Some(ChordAction::FireRelease) | Some(ChordAction::FireCancel)
            ) && ESCAPE_CANCELLED.swap(false, Ordering::SeqCst)
            {
                // Escape already handled cancellation while the chord was
                // held - don't also fire release/cancel now that it's up.
                action = None;
            }

            if key == ChordKey::Key1
                && edge == KeyEdge::Down
                && disposition == KeyDisposition::Suppress
                && key2_was_passed_through
                && is_menu_trigger_vk(k2)
            {
                // Key2's menu-trigger down edge was already passed through
                // before this chord formed. Pass the second key down too so
                // the OS sees a real modifier chord and does not interpret a
                // bare Win/Alt release as a Start-menu/menu activation.
                disposition = KeyDisposition::Passthrough;
                CHORD_MACHINE.with(|m| m.borrow_mut().mark_key_passed_through(key));
            }

            if key == ChordKey::Key2
                && edge == KeyEdge::Down
                && disposition == KeyDisposition::Suppress
                && key1_was_passed_through
                && is_menu_trigger_vk(k1)
            {
                // Symmetric case: the first key was a menu trigger whose
                // down-edge already reached the OS, so pass this chord edge
                // too and keep the eventual key-up balanced.
                disposition = KeyDisposition::Passthrough;
                CHORD_MACHINE.with(|m| m.borrow_mut().mark_key_passed_through(key));
            }

            match action {
                Some(ChordAction::FirePress) => {
                    if let Some(cb) = PRESS_CB.get() {
                        cb();
                    }
                }
                Some(ChordAction::FireRelease) => {
                    if let Some(cb) = RELEASE_CB.get() {
                        cb();
                    }
                }
                Some(ChordAction::FireCancel) => {
                    if let Some(cb) = CANCEL_CB.get() {
                        cb();
                    }
                }
                Some(ChordAction::FireHandless) => {
                    if let Some(cb) = HANDLESS_CB.get() {
                        cb();
                    }
                }
                None => {}
            }

            if disposition == KeyDisposition::Suppress {
                return LRESULT(1);
            }
            // Passthrough falls through to the rest of hook_proc below.
        }

        // While the hold-to-talk chord is active, Space converts it into the
        // existing hands-free mode. Consume both Space edges so the trigger
        // does not leak into the focused application.
        if vk == VK_SPACE && (is_down || is_up) {
            let edge = if is_down { KeyEdge::Down } else { KeyEdge::Up };
            let outcome = CHORD_MACHINE.with(|m| m.borrow_mut().on_space_event(edge));
            if outcome.action == Some(ChordAction::FireHandless) {
                if let Some(cb) = HANDLESS_CB.get() {
                    cb();
                }
            }
            if outcome.disposition == KeyDisposition::Suppress {
                return LRESULT(1);
            }
        }

        if vk == VK_ESCAPE {
            let chord_down = CHORD_MACHINE.with(|m| m.borrow().chord_down);
            if is_down
                && (chord_down
                    || HANDLESS_ACTIVE.load(Ordering::SeqCst)
                    || PROCESSING_GENERATION.load(Ordering::SeqCst) != 0)
            {
                if chord_down {
                    ESCAPE_CANCELLED.store(true, Ordering::SeqCst);
                }
                ESCAPE_KEY_DOWN.store(true, Ordering::SeqCst);
                if let Some(cb) = ESCAPE_CB.get() {
                    cb();
                }
                return LRESULT(1);
            }
            if is_up && ESCAPE_KEY_DOWN.swap(false, Ordering::SeqCst) {
                return LRESULT(1);
            }
        }

        if vk == VK_C {
            if is_down && unsafe { modifier_held(VK_CTRL) && modifier_held(VK_ALT) } {
                // Only intercept (swallow) the chord when there's a callback
                // to act on it — otherwise the user's Ctrl+Alt+C would be
                // lost entirely, so let it fall through to the target app.
                // Windows auto-repeat sends repeated WM_KEYDOWN/WM_SYSKEYDOWN
                // while the chord is held; fire the callback only on the
                // first physical keydown while still intercepting the repeats.
                if COPY_LAST_CB.get().is_some() {
                    if !COPY_LAST_KEY_DOWN.swap(true, Ordering::SeqCst) {
                        if let Some(cb) = COPY_LAST_CB.get() {
                            cb();
                        }
                    }
                    return LRESULT(1);
                }
            }
            // Swallow the C keyup only while the chord modifiers are still
            // held (the normal release order: C first, then Ctrl/Alt). If the
            // user released Ctrl/Alt first, letting the C keyup pass keeps
            // the target app's modifier/keyup bookkeeping consistent — the
            // down was already suppressed, so it's just an orphaned keyup.
            if is_up
                && COPY_LAST_KEY_DOWN.swap(false, Ordering::SeqCst)
                && unsafe { modifier_held(VK_CTRL) && modifier_held(VK_ALT) }
            {
                return LRESULT(1);
            }
        }

        // Update injection history for real user keystrokes only.
        // Synthetic events (LLKHF_INJECTED) are skipped - this prevents our own
        // Ctrl+V paste and any app-generated keyboard events from corrupting the
        // history that backs backspace recovery.
        let is_injected = (kb.flags.0 & LLKHF_INJECTED.0) != 0;
        if !is_injected && is_down && !MODIFIER_VKS.contains(&vk) {
            if vk == VK_BACK {
                // Ctrl+Backspace and Alt+Backspace both delete a whole word -
                // unknown char count, so reset entirely. Plain Backspace pops
                // just the last character to keep context accurate.
                if unsafe { modifier_held(VK_CTRL) || modifier_held(VK_ALT) } {
                    crate::core::injection::reset_injection_history();
                } else {
                    let hwnd = unsafe { GetForegroundWindow().0 as usize };
                    crate::core::injection::backspace_injection_history(hwnd);
                }
            } else if (vk == VK_RETURN
                && !unsafe {
                    modifier_held(VK_SHIFT) || modifier_held(VK_CTRL) || modifier_held(VK_ALT)
                })
                || is_cursor_movement_key(vk)
                || unsafe { modifier_held(VK_CTRL) || modifier_held(VK_ALT) }
            {
                // Keyboard shortcut (Ctrl+Z, Ctrl+A, etc.) - context unknown.
                crate::core::injection::reset_injection_history();
            } else if let Some(ch) = vk_to_char(vk) {
                let hwnd = unsafe { GetForegroundWindow().0 as usize };
                crate::core::injection::append_or_reset_injection_history(hwnd, ch);
            } else {
                crate::core::injection::reset_injection_history();
            }
        }
    }

    CallNextHookEx(None, code, wparam, lparam)
}

#[cfg(test)]
mod chord_tests {
    use super::*;

    fn fresh() -> ChordStateMachine {
        ChordStateMachine::default()
    }

    #[test]
    fn press_order_independent() {
        for order in [
            [ChordKey::Key1, ChordKey::Key2],
            [ChordKey::Key2, ChordKey::Key1],
        ] {
            let mut m = fresh();
            let first = m.on_key_event(order[0], KeyEdge::Down, 0);
            assert_eq!(first.action, None);
            assert_eq!(first.disposition, KeyDisposition::Passthrough);
            let second = m.on_key_event(order[1], KeyEdge::Down, 10);
            assert_eq!(second.action, Some(ChordAction::FirePress));
            assert_eq!(second.disposition, KeyDisposition::Suppress);
            assert!(m.chord_down);
        }
    }

    #[test]
    fn long_hold_still_releases_after_ownership_is_reconciled_away() {
        // Regression: every dictation got cancelled instead of transcribed.
        //
        // `reconcile_stale_key` (and `force_release_win_key`, which does the
        // same thing wholesale before a paste) can clear a key's down/ownership
        // bookkeeping while the user is still physically holding the chord —
        // that is the entire point of it, for keyups the hook never received.
        // But the key keeps autorepeating, and the next repeat then looked like
        // a brand-new chord-forming edge, which reset `chord_first_down_ms` to
        // "now". The hold length is measured from that field, so a chord held
        // for seconds measured as a sub-200ms tap and fired FireCancel.
        let mut m = fresh();
        m.on_key_event(ChordKey::Key1, KeyEdge::Down, 0);
        m.on_key_event(ChordKey::Key2, KeyEdge::Down, 5);
        assert_eq!(m.chord_first_down_ms, 5);

        // Something reconciles key2's ownership away mid-hold.
        m.reconcile_stale_key(ChordKey::Key2, false);
        // ...and key2 autorepeats, since it is still physically down.
        let repeat = m.on_key_event(ChordKey::Key2, KeyEdge::Down, 3000);
        assert_eq!(
            repeat.action, None,
            "a repeat mid-hold must not re-fire a press"
        );
        assert_eq!(
            m.chord_first_down_ms, 5,
            "re-forming the chord mid-hold must not restart the hold clock"
        );

        // The real release, seconds after the real press, is a hold.
        let up = m.on_key_event(ChordKey::Key1, KeyEdge::Up, 4000);
        assert_eq!(up.action, Some(ChordAction::FireRelease));
    }

    #[test]
    fn autorepeat_while_chord_owned_is_suppressed_with_no_action() {
        let mut m = fresh();
        m.on_key_event(ChordKey::Key1, KeyEdge::Down, 0);
        m.on_key_event(ChordKey::Key2, KeyEdge::Down, 10);
        let repeat = m.on_key_event(ChordKey::Key1, KeyEdge::Down, 40);
        assert_eq!(repeat.action, None);
        assert_eq!(repeat.disposition, KeyDisposition::Suppress);
    }

    #[test]
    fn autorepeat_after_handless_trigger_does_not_refire() {
        let mut m = fresh();
        // First tap.
        m.on_key_event(ChordKey::Key1, KeyEdge::Down, 0);
        m.on_key_event(ChordKey::Key2, KeyEdge::Down, 5);
        m.on_key_event(ChordKey::Key1, KeyEdge::Up, 50);
        m.on_key_event(ChordKey::Key2, KeyEdge::Up, 55);
        // Second tap within window -> handsfree.
        m.on_key_event(ChordKey::Key1, KeyEdge::Down, 100);
        let second = m.on_key_event(ChordKey::Key2, KeyEdge::Down, 105);
        assert_eq!(second.action, Some(ChordAction::FireHandless));
        assert!(!m.chord_down);
        // Both physical keys still held -> autorepeat must not refire anything.
        let repeat1 = m.on_key_event(ChordKey::Key1, KeyEdge::Down, 130);
        let repeat2 = m.on_key_event(ChordKey::Key2, KeyEdge::Down, 160);
        assert_eq!(repeat1.action, None);
        assert_eq!(repeat1.disposition, KeyDisposition::Suppress);
        assert_eq!(repeat2.action, None);
        assert_eq!(repeat2.disposition, KeyDisposition::Suppress);
    }

    #[test]
    fn duplicate_keyup_not_owned_passes_through() {
        let mut m = fresh();
        let outcome = m.on_key_event(ChordKey::Key1, KeyEdge::Up, 0);
        assert_eq!(outcome.action, None);
        assert_eq!(outcome.disposition, KeyDisposition::Passthrough);
    }

    #[test]
    fn standalone_key2_keyup_passes_through() {
        let mut m = fresh();
        m.on_key_event(ChordKey::Key2, KeyEdge::Down, 0);
        let outcome = m.on_key_event(ChordKey::Key2, KeyEdge::Up, 10);
        assert_eq!(outcome.action, None);
        assert_eq!(outcome.disposition, KeyDisposition::Passthrough);
    }

    #[test]
    fn owned_keyup_is_suppressed() {
        let mut m = fresh();
        m.on_key_event(ChordKey::Key1, KeyEdge::Down, 0);
        m.on_key_event(ChordKey::Key2, KeyEdge::Down, 5);
        let up = m.on_key_event(ChordKey::Key2, KeyEdge::Up, 500);
        assert_eq!(up.disposition, KeyDisposition::Suppress);
    }

    #[test]
    fn first_key2_keyup_matches_its_passthrough_down() {
        let mut m = fresh();
        m.on_key_event(ChordKey::Key2, KeyEdge::Down, 0);
        m.on_key_event(ChordKey::Key1, KeyEdge::Down, 5);
        let up = m.on_key_event(ChordKey::Key2, KeyEdge::Up, 500);
        assert_eq!(up.action, Some(ChordAction::FireRelease));
        assert_eq!(up.disposition, KeyDisposition::Passthrough);
    }

    #[test]
    fn held_key_double_tap_of_other_key_triggers_handless() {
        // Hold key1 continuously (e.g. Ctrl), quick-tap key2 (e.g. Win) twice
        // without ever releasing key1 — the gesture a mouse button remapped
        // to double-click a single key needs, since it can never hold one
        // key while tapping another.
        let mut m = fresh();
        m.on_key_event(ChordKey::Key1, KeyEdge::Down, 0);
        m.on_key_event(ChordKey::Key2, KeyEdge::Down, 5);
        let cancel = m.on_key_event(ChordKey::Key2, KeyEdge::Up, 50);
        assert_eq!(cancel.action, Some(ChordAction::FireCancel));
        // key1 never released; key2 taps down again within the window.
        let outcome = m.on_key_event(ChordKey::Key2, KeyEdge::Down, 100);
        assert_eq!(outcome.action, Some(ChordAction::FireHandless));
        assert!(!m.chord_down);
        assert!(m.key1_down); // key1 still physically held throughout
    }

    #[test]
    fn full_release_then_second_tap_within_window_triggers_handless_once() {
        let mut m = fresh();
        m.on_key_event(ChordKey::Key1, KeyEdge::Down, 0);
        m.on_key_event(ChordKey::Key2, KeyEdge::Down, 5);
        m.on_key_event(ChordKey::Key1, KeyEdge::Up, 50);
        m.on_key_event(ChordKey::Key2, KeyEdge::Up, 55); // full release at 55
        let second = m.on_key_event(ChordKey::Key1, KeyEdge::Down, 300);
        let outcome = m.on_key_event(ChordKey::Key2, KeyEdge::Down, 340); // 340-50=290 <= 300
        assert_eq!(second.action, None);
        assert_eq!(outcome.action, Some(ChordAction::FireHandless));
    }

    #[test]
    fn window_measured_from_first_release() {
        // The double-tap clock starts at the FIRST release of the pair (not
        // the last), so the solo-hold gesture above has a well-defined start
        // point even though the held key may never release at all.
        let mut m = fresh();
        m.on_key_event(ChordKey::Key1, KeyEdge::Down, 0);
        m.on_key_event(ChordKey::Key2, KeyEdge::Down, 5);
        m.on_key_event(ChordKey::Key1, KeyEdge::Up, 50); // first release -> deadline starts here
        m.on_key_event(ChordKey::Key2, KeyEdge::Up, 130); // second released 80ms later
        m.on_key_event(ChordKey::Key1, KeyEdge::Down, 340);
        let outcome = m.on_key_event(ChordKey::Key2, KeyEdge::Down, 345); // 345-50=295 <= 300
        assert_eq!(outcome.action, Some(ChordAction::FireHandless));
    }

    #[test]
    fn second_tap_outside_window_starts_fresh_press() {
        let mut m = fresh();
        m.on_key_event(ChordKey::Key1, KeyEdge::Down, 0);
        m.on_key_event(ChordKey::Key2, KeyEdge::Down, 5);
        m.on_key_event(ChordKey::Key1, KeyEdge::Up, 50);
        m.on_key_event(ChordKey::Key2, KeyEdge::Up, 55);
        m.on_key_event(ChordKey::Key1, KeyEdge::Down, 1000);
        let outcome = m.on_key_event(ChordKey::Key2, KeyEdge::Down, 1010);
        assert_eq!(outcome.action, Some(ChordAction::FirePress));
        assert!(m.chord_down);
    }

    #[test]
    fn solo_key_tap_never_fires_anything() {
        let mut m = fresh();
        let down = m.on_key_event(ChordKey::Key1, KeyEdge::Down, 0);
        let up = m.on_key_event(ChordKey::Key1, KeyEdge::Up, 50);
        assert_eq!(down.action, None);
        assert_eq!(down.disposition, KeyDisposition::Passthrough);
        assert_eq!(up.action, None);
        assert_eq!(up.disposition, KeyDisposition::Passthrough);
    }

    #[test]
    fn space_while_chord_is_held_converts_to_handless_and_stays_suppressed() {
        let mut m = fresh();
        m.on_key_event(ChordKey::Key1, KeyEdge::Down, 0);
        m.on_key_event(ChordKey::Key2, KeyEdge::Down, 5);

        let trigger = m.on_space_event(KeyEdge::Down);
        assert_eq!(trigger.action, Some(ChordAction::FireHandless));
        assert_eq!(trigger.disposition, KeyDisposition::Suppress);
        assert!(!m.chord_down);

        let repeat = m.on_space_event(KeyEdge::Down);
        assert_eq!(repeat.action, None);
        assert_eq!(repeat.disposition, KeyDisposition::Suppress);

        let space_up = m.on_space_event(KeyEdge::Up);
        assert_eq!(space_up.action, None);
        assert_eq!(space_up.disposition, KeyDisposition::Suppress);

        // Releasing the original hold keys must not fire the normal release
        // action after Space has converted the session.
        let key1_up = m.on_key_event(ChordKey::Key1, KeyEdge::Up, 100);
        let key2_up = m.on_key_event(ChordKey::Key2, KeyEdge::Up, 105);
        assert_eq!(key1_up.action, None);
        assert_eq!(key2_up.action, None);
    }

    #[test]
    fn space_outside_active_chord_passes_through() {
        let mut m = fresh();
        let down = m.on_space_event(KeyEdge::Down);
        let up = m.on_space_event(KeyEdge::Up);
        assert_eq!(down.disposition, KeyDisposition::Passthrough);
        assert_eq!(up.disposition, KeyDisposition::Passthrough);
    }

    #[test]
    fn reset_gesture_state_clears_space_conversion_state() {
        let mut m = fresh();
        m.on_key_event(ChordKey::Key1, KeyEdge::Down, 0);
        m.on_key_event(ChordKey::Key2, KeyEdge::Down, 5);
        let trigger = m.on_space_event(KeyEdge::Down);
        assert_eq!(trigger.action, Some(ChordAction::FireHandless));

        m.reset_gesture_state();

        let down = m.on_space_event(KeyEdge::Down);
        let up = m.on_space_event(KeyEdge::Up);
        assert_eq!(down.disposition, KeyDisposition::Passthrough);
        assert_eq!(up.disposition, KeyDisposition::Passthrough);
    }

    #[test]
    fn stale_key_bookkeeping_does_not_fire_on_lone_press() {
        let mut m = fresh();
        // Simulate a missed keyup: key1 (e.g. Ctrl) is marked down internally
        // but the OS no longer reports it held.
        m.on_key_event(ChordKey::Key1, KeyEdge::Down, 0);
        m.reconcile_stale_key(ChordKey::Key1, false);
        assert!(!m.key1_down);
        // A lone press of key2 must not look like a chord-forming edge.
        let outcome = m.on_key_event(ChordKey::Key2, KeyEdge::Down, 100);
        assert_eq!(outcome.action, None);
        assert_eq!(outcome.disposition, KeyDisposition::Passthrough);
    }

    #[test]
    fn reconcile_leaves_genuinely_held_key_untouched() {
        let mut m = fresh();
        m.on_key_event(ChordKey::Key1, KeyEdge::Down, 0);
        m.reconcile_stale_key(ChordKey::Key1, true);
        assert!(m.key1_down);
        let outcome = m.on_key_event(ChordKey::Key2, KeyEdge::Down, 10);
        assert_eq!(outcome.action, Some(ChordAction::FirePress));
    }

    #[test]
    fn reset_gesture_state_preserves_key_ownership() {
        let mut m = fresh();
        m.on_key_event(ChordKey::Key1, KeyEdge::Down, 0);
        m.on_key_event(ChordKey::Key2, KeyEdge::Down, 5);
        assert!(m.chord_down);
        m.reset_gesture_state();
        assert!(!m.chord_down);
        assert_eq!(m.tap, TapState::None);
        // Physical/ownership state must survive the reset.
        assert!(m.key1_down);
        assert!(m.key2_down);
        assert!(m.key1_was_chord);
        assert!(m.key2_was_chord);
        // The first key's down-edge was passed through before the chord formed,
        // so its matching keyup must pass through too. The second key remains
        // fully owned by Verenu.
        let up1 = m.on_key_event(ChordKey::Key1, KeyEdge::Up, 10);
        let up2 = m.on_key_event(ChordKey::Key2, KeyEdge::Up, 15);
        assert_eq!(up1.disposition, KeyDisposition::Passthrough);
        assert_eq!(up2.disposition, KeyDisposition::Suppress);
    }
}

#[allow(clippy::too_many_arguments)]
pub fn start<P, R, H, C, E, L, O>(
    on_press: P,
    on_release: R,
    on_handless: H,
    on_cancel: C,
    on_escape: E,
    on_copy_last: L,
    on_repair_open: O,
) -> Result<std::thread::JoinHandle<()>, String>
where
    P: Fn() + Send + Sync + 'static,
    R: Fn() + Send + Sync + 'static,
    H: Fn() + Send + Sync + 'static,
    C: Fn() + Send + Sync + 'static,
    E: Fn() + Send + Sync + 'static,
    L: Fn() + Send + Sync + 'static,
    O: Fn() + Send + Sync + 'static,
{
    let _ = PRESS_CB.set(Box::new(on_press));
    let _ = RELEASE_CB.set(Box::new(on_release));
    let _ = HANDLESS_CB.set(Box::new(on_handless));
    let _ = CANCEL_CB.set(Box::new(on_cancel));
    let _ = ESCAPE_CB.set(Box::new(on_escape));
    let _ = COPY_LAST_CB.set(Box::new(on_copy_last));
    let _ = REPAIR_OPEN_CB.set(Box::new(on_repair_open));

    // Verify the hook can be installed before spawning the thread so the caller
    // gets a synchronous error instead of a silent panic on a background thread.
    unsafe {
        let probe = SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), None, 0)
            .map_err(|e| format!("Failed to install keyboard hook: {e}"))?;
        windows::Win32::UI::WindowsAndMessaging::UnhookWindowsHookEx(probe).ok();
    }

    let handle = std::thread::spawn(|| unsafe {
        let hook = match SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), None, 0) {
            Ok(h) => h,
            Err(e) => {
                log::error!("SetWindowsHookExW failed on hook thread: {e}");
                return;
            }
        };

        // Seed the cached state immediately: SetWindowsHookExW just gave this
        // thread a message queue, so GetKeyState's toggle bit already reflects
        // the real current state here, before the first key event arrives.
        CAPS_LOCK_ON.store((GetKeyState(0x14) & 0x0001) != 0, Ordering::SeqCst);

        let mut msg = MSG::default();
        loop {
            let status = GetMessageW(&mut msg, None, 0, 0).0;
            if status == -1 {
                log::error!("GetMessageW failed in hotkey hook thread");
                break;
            }
            if status == 0 {
                break;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        windows::Win32::UI::WindowsAndMessaging::UnhookWindowsHookEx(hook).ok();
    });

    Ok(handle)
}
