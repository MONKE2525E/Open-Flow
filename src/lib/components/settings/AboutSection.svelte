<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { updateInfo, type UpdateInfo } from '../../stores';
  import { saveSetting } from '../../settings';

  let { appVersion }: { appVersion: string } = $props();

  type UpdateCheckState = 'idle' | 'checking' | 'up-to-date' | 'available';
  let updateCheckState: UpdateCheckState = $state('idle');
  let installingFromAbout = $state(false);

  $effect(() => {
    if ($updateInfo) updateCheckState = 'available';
  });

  async function openRepo() {
    try {
      const { open } = await import('@tauri-apps/plugin-shell');
      await open('https://github.com/MONKE2525E/Open-Flow');
    } catch {
      window.open('https://github.com/MONKE2525E/Open-Flow', '_blank');
    }
  }

  async function checkForUpdateManual() {
    updateCheckState = 'checking';
    try {
      const update = await invoke<UpdateInfo | null>('check_for_update');
      if (update) {
        try { await saveSetting('update_dismissed_version', null); } catch {}
        updateInfo.set(update);
        updateCheckState = 'available';
      } else {
        updateCheckState = 'up-to-date';
      }
    } catch {
      updateCheckState = 'idle';
    }
  }

  async function handleInstall() {
    if (!$updateInfo) return;
    installingFromAbout = true;
    try {
      await invoke('install_update', { downloadUrl: $updateInfo.downloadUrl });
    } catch (e) {
      console.error('Install failed:', e);
    } finally {
      installingFromAbout = false;
    }
  }
</script>

<h2 class="settings-h">About</h2>
<div class="setting-row">
  <div><div class="label">Version</div></div>
  <span class="desc">v{appVersion}</span>
</div>
<div class="setting-row">
  <div><div class="label">License</div></div>
  <span class="desc">MIT</span>
</div>
<div class="setting-row">
  <div><div class="label">Source</div></div>
  <button class="btn-ghost" onclick={openRepo}>github.com/MONKE2525E/Open-Flow</button>
</div>
<div class="setting-row">
  <div>
    <div class="label">Updates</div>
    {#if updateCheckState === 'up-to-date'}
      <div class="update-status-wrap">
        <div class="desc update-ok update-status">You're on the latest version</div>
      </div>
    {:else if updateCheckState === 'available' && $updateInfo}
      <div class="update-status-wrap">
        <div class="desc update-available update-status">v{$updateInfo.version} is available</div>
      </div>
    {/if}
  </div>
  <div class="update-controls">
    {#if updateCheckState === 'available' && $updateInfo}
      <button class="btn-ghost" onclick={handleInstall} disabled={installingFromAbout}>
        {installingFromAbout ? 'Downloading…' : 'Install Now'}
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
