import type { ProviderStatusAlert } from './stores';
import { appStore } from './stores';
import { invoke, listen } from './tauri';

const PROVIDER_STATUS_INTERVAL_MS = 5 * 60 * 1000;
const API_HEALTH_INTERVAL_MS = 20 * 60 * 1000;

async function checkProviderStatus(): Promise<void> {
  try {
    appStore.providerStatusAlerts = (await invoke<ProviderStatusAlert[] | null>('check_provider_status')) ?? [];
  } catch (error) {
    console.warn('Provider status check failed:', error);
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
  void checkProviderStatus();
  const timer = window.setInterval(() => void checkProviderStatus(), PROVIDER_STATUS_INTERVAL_MS);

  // The backend fires this after a pipeline call fails in a way that looks
  // provider-side (quota or a retryable timeout/429/5xx), so a real outage
  // shows up immediately instead of waiting up to 5 minutes.
  let disposed = false;
  let unlistenRecheck: (() => void) | undefined;
  listen('verenu:recheck-provider-status', () => void checkProviderStatus())
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
