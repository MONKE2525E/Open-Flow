// Lightweight platform detection for UI labels. Uses the WebView user-agent so
// no extra Tauri plugin/dependency is required. The backend remains the source
// of truth for actual platform behavior.
const ua = typeof navigator !== 'undefined' ? navigator.userAgent : '';

export const isMac = /Mac/i.test(ua);
export const isWindows = /Win/i.test(ua);

/** Human label for a `KeyboardEvent.code`, OS-aware (⌘/⌃/⌥ + fn on macOS). */
export function formatKeyLabel(code: string): string {
	if (isMac) {
		const mac: Record<string, string> = {
			MetaLeft: '⌘',
			MetaRight: '⌘',
			ControlLeft: '⌃',
			ControlRight: '⌃',
			AltLeft: '⌥',
			AltRight: '⌥',
			ShiftLeft: '⇧',
			ShiftRight: '⇧',
			Fn: 'fn',
			CapsLock: '⇪',
			Space: 'Space'
		};
		if (mac[code]) return mac[code];
	}
	const generic: Record<string, string> = {
		ControlLeft: 'Ctrl',
		ControlRight: 'Ctrl',
		MetaLeft: 'Windows',
		MetaRight: 'Windows',
		AltLeft: 'Alt',
		AltRight: 'Alt',
		ShiftLeft: 'Shift',
		ShiftRight: 'Shift',
		Fn: 'Fn',
		Space: 'Space'
	};
	return (
		generic[code] ??
		code.replace('Left', '').replace('Right', '').replace('Key', '').replace('Digit', '')
	);
}

/**
 * The default dictation hotkey codes for the current platform.
 *
 * macOS uses Carbon `RegisterEventHotKey` (no Input Monitoring permission), which
 * can't bind a modifier-only chord — so the default is ⌥ Option + Space (two
 * adjacent bottom-row keys, no Fn/Spotlight conflict). Windows keeps its chord.
 */
export const defaultHotkey: string[] = isMac ? ['AltLeft', 'Space'] : ['ControlLeft', 'MetaLeft'];

/** Platform label for the fixed copy-last-dictation shortcut. */
export const copyLastHotkey: string[] = isMac
	? ['AltLeft', 'MetaLeft', 'KeyC']
	: ['ControlLeft', 'AltLeft', 'KeyC'];
