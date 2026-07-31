import type { GlobalMessage, ProviderStatusAlert } from './stores';
import { appStore } from './stores';
import { invoke, listen } from './tauri';

const PROVIDER_STATUS_INTERVAL_MS = 5 * 60 * 1000;
const API_HEALTH_INTERVAL_MS = 20 * 60 * 1000;

export async function checkStatus(): Promise<void> {
  const [alertsResult, messageResult] = await Promise.allSettled([
    invoke<ProviderStatusAlert[] | null>('check_provider_status'),
    invoke<GlobalMessage | null>('check_global_message'),
  ]);

  if (alertsResult.status === 'fulfilled') {
    if (!appStore.providerStatusSimulation) {
      appStore.providerStatusAlerts = alertsResult.value ?? [];
    }
  } else {
    console.warn('Provider status check failed:', alertsResult.reason);
  }

  if (messageResult.status === 'fulfilled') {
    const message = messageResult.value;
    appStore.globalMessage = message && (message.showToUsers || appStore.globalMessageSimulation)
      ? { ...message, showToUsers: true }
      : null;
  } else {
    console.warn('Global message check failed:', messageResult.reason);
  }
}

// Health has no UI today — it's polled in the background so the state is
// already warm for future features that need to know whether api.verenu.com
// is reachable.
async function checkApiHealth(): Promise<void> {
  try {
    appStore.apiHealthy = await invoke<boolean>('check_verenu_api_health');
  } catch (error) {
    // Don't leave a stale prior result in place — a failed check means we no
    // longer know the current state, not that it's confirmed unhealthy.
    appStore.apiHealthy = null;
    console.warn('Verenu API health check failed:', error);
  }
}

export function startProviderStatusChecks(): () => void {
  void checkStatus();
  const timer = window.setInterval(() => void checkStatus(), PROVIDER_STATUS_INTERVAL_MS);

  // The backend fires this after a pipeline call fails in a way that looks
  // provider-side (quota or a retryable timeout/429/5xx), so a real outage
  // shows up immediately instead of waiting up to 5 minutes.
  let disposed = false;
  let unlistenRecheck: (() => void) | undefined;
  listen('verenu:recheck-provider-status', () => void checkStatus())
    .then((unlisten) => {
      if (disposed) {
        unlisten();
        return;
      }
      unlistenRecheck = unlisten;
    })
    .catch(() => {});

  return () => {
    disposed = true;
    window.clearInterval(timer);
    unlistenRecheck?.();
  };
}

export function startApiHealthChecks(): () => void {
  void checkApiHealth();
  const timer = window.setInterval(() => void checkApiHealth(), API_HEALTH_INTERVAL_MS);
  return () => window.clearInterval(timer);
}
