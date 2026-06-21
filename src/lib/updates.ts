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

async function requestNotificationPermissionAtStartup(): Promise<void> {
  if (!isTauriRuntime()) return;

  try {
    if (await isPermissionGranted()) return;
    await requestPermission();
  } catch (error) {
    console.warn('Notification permission request failed:', error);
  }
}

async function notificationsAllowed(): Promise<boolean> {
  if (!isTauriRuntime()) return false;

  try {
    return await isPermissionGranted();
  } catch (error) {
    console.warn('Notification permission check failed:', error);
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
  try {
    [dismissedVersion, notifiedVersion] = await Promise.all([
      invoke<string | null>('get_setting', { key: 'update_dismissed_version' }),
      invoke<string | null>('get_setting', { key: 'update_notified_version' }),
    ]);
  } catch (error) {
    console.warn('Update state lookup failed:', error);
  }

  if (dismissedVersion === update.version) return;

  appStore.updateInfo = update;

  if (notifiedVersion === update.version) return;
  if (!(await notificationsAllowed())) return;

  try {
    await sendUpdateNotification(update);
  } catch (error) {
    console.warn('Update notification failed:', error);
  } finally {
    try {
      await saveSetting('update_notified_version', update.version);
    } catch (error) {
      console.warn('Failed to persist notified update version:', error);
    }
  }
}

export async function startAutomaticUpdateChecks(): Promise<() => void> {
  await requestNotificationPermissionAtStartup();
  await checkForAutomaticUpdates();

  const timer = window.setInterval(() => {
    void checkForAutomaticUpdates();
  }, UPDATE_CHECK_INTERVAL_MS);

  return () => {
    window.clearInterval(timer);
  };
}
