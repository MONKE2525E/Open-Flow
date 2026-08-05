import type { UpdateInfo } from './stores';
import { appStore } from './stores';
import { saveSetting } from './settings';
import { invoke } from './tauri';
import { ensureNotificationPermission } from './notifications';

const UPDATE_CHECK_INTERVAL_MS = 6 * 60 * 60 * 1000;

async function checkForAutomaticUpdates(): Promise<void> {
  let update: UpdateInfo | null = null;

  try {
    update = await invoke<UpdateInfo | null>('check_for_update');
  } catch (error) {
    console.warn('Automatic update check failed:', error);
    return;
  }

  if (!update) return;

  let dismissedVersion: string | null = null;
  let notifiedVersion: string | null = null;
  // Resolve each lookup independently so a single failed setting read doesn't
  // wipe out the other — otherwise one failure could re-show a dismissed
  // update or re-fire an already-sent notification.
  try {
    [dismissedVersion, notifiedVersion] = await Promise.all([
      invoke<string | null>('get_setting', { key: 'update_dismissed_version' }).catch((error) => {
        console.warn('Failed to look up dismissed update version:', error);
        return null;
      }),
      invoke<string | null>('get_setting', { key: 'update_notified_version' }).catch((error) => {
        console.warn('Failed to look up notified update version:', error);
        return null;
      }),
    ]);
  } catch (error) {
    console.warn('Update state lookup failed:', error);
  }

  if (dismissedVersion === update.version) return;

  appStore.updateInfo = update;

  if (notifiedVersion === update.version) return;
  if (!(await ensureNotificationPermission())) return;

  try {
    // Deliver through Rust so Windows attributes the toast to Verenu rather
    // than the WebView host process, which can be PowerShell in development.
    await invoke('notify_update_available', { version: update.version });
  } catch (error) {
    // Don't persist the notified version if delivery failed — otherwise the
    // user never sees a notification yet we'd mark it as notified and never
    // retry on the next check.
    console.warn('Update notification failed:', error);
    return;
  }

  try {
    await saveSetting('update_notified_version', update.version);
  } catch (error) {
    console.warn('Failed to persist notified update version:', error);
  }
}

export function startAutomaticUpdateChecks(): () => void {
  // Fire the first check in the background rather than awaiting it, so this
  // function can return the cleanup synchronously. That removes the unmount
  // race the caller would otherwise have to guard against — the interval is
  // registered before we return, so cleanup can always clear it.
  void checkForAutomaticUpdates();

  const timer = window.setInterval(() => {
    void checkForAutomaticUpdates();
  }, UPDATE_CHECK_INTERVAL_MS);

  return () => {
    window.clearInterval(timer);
  };
}
