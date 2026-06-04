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

/** The default dictation hotkey codes for the current platform. */
export const defaultHotkey: string[] = isMac ? ['ControlLeft', 'Fn'] : ['ControlLeft', 'MetaLeft'];
