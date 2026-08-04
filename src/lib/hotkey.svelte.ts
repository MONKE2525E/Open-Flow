// Single source of truth for the *user's* dictation hotkey in the UI.
//
// Before this module, four places each had their own idea of the chord: two
// hardcoded `fn + Control` strings (the pre-migration macOS default), and two
// that read `defaultHotkey` and never looked at the saved setting. Rebinding
// the hotkey in Settings left the setup wizard advertising keys that do nothing.
//
// Codes are `KeyboardEvent.code` values, exactly as `save_hotkey` stores them.
// Slot 2 may be '' for a single-key macOS binding.
import { invoke } from './tauri';
import { defaultHotkey, formatKeyLabel } from './platform';

const state = $state<{ codes: string[] }>({ codes: defaultHotkey });

/** The current hotkey codes — reactive; reads `defaultHotkey` until `loadHotkey()` resolves. */
export function hotkeyCodes(): string[] {
	return state.codes;
}

/** Display labels for the chord, empty slots dropped (e.g. `['Ctrl', 'Windows']`). */
export function hotkeyLabels(): string[] {
	return state.codes.filter(Boolean).map(formatKeyLabel);
}

/**
 * Loads the saved hotkey. Safe to call repeatedly — the backend returns null
 * when the user has never set one, in which case the platform default stands.
 */
export async function loadHotkey(): Promise<void> {
	try {
		const saved = await invoke<string[] | null>('get_setting', { key: 'hotkey' });
		if (Array.isArray(saved) && saved.length === 2 && saved.some(Boolean)) {
			state.codes = saved;
		}
	} catch {
		// Keep the platform default; a missing hotkey is not worth surfacing.
	}
}

/** Left/Right modifier variants are the same physical intent to a user pressing the chord. */
function codeVariants(code: string): string[] {
	if (code.endsWith('Left')) return [code, `${code.slice(0, -4)}Right`];
	if (code.endsWith('Right')) return [code, `${code.slice(0, -5)}Left`];
	return [code];
}

/** Every `KeyboardEvent.code` that should be tracked to detect the chord. */
export function hotkeyWatchCodes(): Set<string> {
	return new Set(state.codes.filter(Boolean).flatMap(codeVariants));
}

/** True when `pressed` satisfies every slot of the chord. */
export function matchesHotkey(pressed: Set<string>): boolean {
	const slots = state.codes.filter(Boolean);
	if (slots.length === 0) return false;
	return slots.every((code) => codeVariants(code).some((variant) => pressed.has(variant)));
}
