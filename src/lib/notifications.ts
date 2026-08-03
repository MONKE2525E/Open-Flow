import { isPermissionGranted, requestPermission } from '@tauri-apps/plugin-notification';
import { isTauriRuntime } from './tauri';

// Request notification permission contextually — at the moment an action that
// will later notify begins (e.g. starting a model download), not at startup
// with no context, which users tend to decline. Best-effort: never throws.
export async function ensureNotificationPermission(): Promise<boolean> {
  if (!isTauriRuntime()) return false;
  try {
    if (await isPermissionGranted()) return true;
    return (await requestPermission()) === 'granted';
  } catch (error) {
    console.warn('Notification permission request failed:', error);
    return false;
  }
}
