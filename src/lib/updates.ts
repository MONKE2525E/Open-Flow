import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from '@tauri-apps/plugin-notification';
import type { UpdateInfo } from './stores';
import { appStore } from './stores';
import { saveSetting } from './settings';
import { invoke, isTauriRuntime } from './tauri';

const UPDATE_CHECK_INTERVAL_MS = 6 * 60 * 60 * 1000;

// Request notification permission contextually — only when we actually have a
// new update to tell the user about — rather than firing the system prompt at
// startup with no context, which users tend to decline. Returns whether
// notifications are permitted after the (possibly skipped) request.
async function ensureNotificationPermission(): Promise<boolean> {
  if (!isTauriRuntime()) return false;

  try {
    if (await isPermissionGranted()) return true;
    return (await requestPermission()) === 'granted';
  } catch (error) {
    console.warn('Notification permission request failed:', error);
    return false;
  }
}

async function sendUpdateNotification(update: UpdateInfo): Promise<void> {
  await sendNotification({
    title: 'Verenu update available',
    body: `Version v${update.version} is ready. Open Verenu to update.`,
  });
}

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
    await sendUpdateNotification(update);
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

export async function startAutomaticUpdateChecks(): Promise<() => void> {
  await checkForAutomaticUpdates();

  const timer = window.setInterval(() => {
    void checkForAutomaticUpdates();
  }, UPDATE_CHECK_INTERVAL_MS);

  return () => {
    window.clearInterval(timer);
  };
}
