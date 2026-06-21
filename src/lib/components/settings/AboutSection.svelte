<script lang="ts">
  import { invoke } from '../../tauri';
  import { appStore, type UpdateInfo } from '../../stores';
  import { saveSetting } from '../../settings';

  let { appVersion }: { appVersion: string } = $props();

  type UpdateCheckState = 'idle' | 'checking' | 'up-to-date' | 'available';
  let updateCheckState: UpdateCheckState = $state('idle');
  let installingFromAbout = $state(false);
  let versionTapCount = $state(0);
  let versionTapTimer: ReturnType<typeof setTimeout> | null = null;
  let devModeHintVisible = $state(false);

  const SOURCE_REPO = 'MONKE2525E/Verenu';

  $effect(() => {
    if (appStore.updateInfo) updateCheckState = 'available';
  });

  async function openRepo() {
    const url = `https://github.com/${SOURCE_REPO}`;
    try {
      const { open } = await import('@tauri-apps/plugin-shell');
      await open(url);
    } catch {
      window.open(url, '_blank');
    }
  }

  async function checkForUpdateManual() {
    updateCheckState = 'checking';
    try {
      const update = await invoke<UpdateInfo | null>('check_for_update');
      if (update) {
        try { await saveSetting('update_dismissed_version', null); } catch {}
        appStore.updateInfo = update;
        updateCheckState = 'available';
      } else {
        updateCheckState = 'up-to-date';
      }
    } catch {
      updateCheckState = 'idle';
    }
  }

  function installActionLabel(update: UpdateInfo | null): string {
    return update?.installMode === 'download' ? 'Download DMG' : 'Install Now';
  }

  async function handleInstall() {
    if (!appStore.updateInfo) return;
    installingFromAbout = true;
    try {
      await invoke('install_update', { downloadUrl: appStore.updateInfo.downloadUrl });
    } catch (e) {
      console.error('Install failed:', e);
    } finally {
      installingFromAbout = false;
    }
  }

  function rerunSetup() {
    appStore.setupComplete = false;
  }

  function handleVersionTap() {
    if (appStore.devModeEnabled) return;
    versionTapCount += 1;
    if (versionTapTimer) clearTimeout(versionTapTimer);
    versionTapTimer = setTimeout(() => {
      versionTapCount = 0;
    }, 2600);
    if (versionTapCount >= 10) {
      appStore.devModeEnabled = true;
      invoke('set_dev_logging_enabled', { enabled: true }).catch(() => {});
      versionTapCount = 0;
      if (versionTapTimer) {
        clearTimeout(versionTapTimer);
        versionTapTimer = null;
      }
      devModeHintVisible = true;
      setTimeout(() => {
        devModeHintVisible = false;
      }, 1800);
    }
  }
</script>

<h2 class="settings-h">About</h2>
<div class="setting-row">
  <div><div class="label">Version</div></div>
  <button class="version-tap desc" onclick={handleVersionTap}>v{appVersion}</button>
</div>
{#if devModeHintVisible}
  <div class="dev-hint-row">
    <span class="desc dev-hint">Developer mode enabled for this session.</span>
  </div>
{/if}
<div class="setting-row">
  <div><div class="label">License</div></div>
  <span class="desc">MIT</span>
</div>
<div class="setting-row">
  <div><div class="label">Source</div></div>
  <button class="btn-ghost" onclick={openRepo}>github.com/{SOURCE_REPO}</button>
</div>
<div class="setting-row">
  <div>
    <div class="label">Setup</div>
    <div class="desc">Re-run onboarding to review your provider, key, and defaults.</div>
  </div>
  <button class="btn-ghost" onclick={rerunSetup}>Re-run setup</button>
</div>
<div class="setting-row">
  <div>
    <div class="label">Updates</div>
    {#if updateCheckState === 'up-to-date'}
      <div class="update-status-wrap">
        <div class="desc update-ok update-status">You're on the latest version</div>
      </div>
    {:else if updateCheckState === 'available' && appStore.updateInfo}
      <div class="update-status-wrap">
        <div class="desc update-available update-status">v{appStore.updateInfo.version} is available</div>
      </div>
    {/if}
  </div>
  <div class="update-controls">
    {#if updateCheckState === 'available' && appStore.updateInfo}
      <button class="btn-ghost" onclick={handleInstall} disabled={installingFromAbout}>
        {installingFromAbout
          ? (appStore.updateInfo?.installMode === 'download' ? 'Opening…' : 'Installing…')
          : installActionLabel(appStore.updateInfo)}
      </button>
    {:else}
      <button
        class="btn-ghost"
        onclick={checkForUpdateManual}
        disabled={updateCheckState === 'checking'}
      >
        {updateCheckState === 'checking' ? 'Checking…' : 'Check for Updates'}
      </button>
    {/if}
  </div>
</div>

<style>
  .update-controls { flex-shrink: 0; }
  .version-tap {
    border: none;
    background: transparent;
    padding: 0;
  }
  .dev-hint-row {
    margin-top: -4px;
    margin-bottom: 8px;
  }
  .dev-hint {
    color: var(--ink-faint);
  }
  .update-ok { color: var(--success); }
  .update-available { color: var(--accent); }

  .update-status-wrap {
    overflow: hidden;
  }

  .update-status {
    animation: update-drop 260ms cubic-bezier(0.22, 1, 0.36, 1) both;
  }

  @keyframes update-drop {
    from {
      opacity: 0;
      transform: translateY(-6px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }
</style>
