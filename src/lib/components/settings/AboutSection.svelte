<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '../../tauri';
  import { appStore, type UpdateInfo } from '../../stores';
  import { saveSetting } from '../../settings';
  import Toggle from '../Toggle.svelte';
  import { modalFocusTrap } from '../../modalFocus';
  import { modalBackdrop, modalCard, MOTION_PX, motionPx } from '../../motion';

  let { appVersion }: { appVersion: string } = $props();

  type UpdateCheckState = 'idle' | 'checking' | 'up-to-date' | 'available';
  let updateCheckState: UpdateCheckState = $state('idle');
  let installingFromAbout = $state(false);
  let versionTapCount = $state(0);
  let versionTapTimer: ReturnType<typeof setTimeout> | null = null;
  let devModeHintVisible = $state(false);
  let betaUpdatesEnabled = $state(false);
  let confirmBetaUpdates = $state(false);
  let savingBetaUpdates = $state(false);
  let betaCancelButton = $state<HTMLButtonElement | null>(null);

  const SOURCE_REPO = 'MONKE2525E/Verenu';
  const WEBSITE_URL = 'https://verenu.com';

  $effect(() => {
    if (appStore.updateInfo) updateCheckState = 'available';
  });

  onMount(() => {
    invoke<boolean | null>('get_setting', { key: 'beta_updates_enabled' })
      .then((value) => {
        betaUpdatesEnabled = value ?? false;
        appStore.betaUpdatesEnabled = betaUpdatesEnabled;
      })
      .catch((error) => console.error('Failed to load beta update setting:', error));
  });

  async function openExternal(url: string) {
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

  function handleBetaUpdatesToggle(value: boolean) {
    if (value) {
      confirmBetaUpdates = true;
      return;
    }
    void setBetaUpdates(false);
  }

  async function setBetaUpdates(value: boolean) {
    if (savingBetaUpdates) return;
    const previous = betaUpdatesEnabled;
    savingBetaUpdates = true;
    betaUpdatesEnabled = value;
    appStore.betaUpdatesEnabled = value;
    try {
      try {
        await saveSetting('beta_updates_enabled', value);
      } catch (error) {
        betaUpdatesEnabled = previous;
        appStore.betaUpdatesEnabled = previous;
        console.error('Failed to save beta update setting:', error);
        return;
      }

      // A result from the other channel is no longer trustworthy. Re-check now
      // so switching channels has an immediate, visible effect.
      try { await saveSetting('update_dismissed_version', null); } catch {}
      try { await saveSetting('update_notified_version', null); } catch {}
      appStore.updateInfo = null;
      updateCheckState = 'idle';
      await checkForUpdateManual();
    } finally {
      savingBetaUpdates = false;
    }
  }

  async function confirmEnableBetaUpdates() {
    confirmBetaUpdates = false;
    await setBetaUpdates(true);
  }

  function handleBetaModalKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape' && confirmBetaUpdates) {
      event.preventDefault();
      event.stopPropagation();
      confirmBetaUpdates = false;
    }
  }

  function downloadActionLabel(update: UpdateInfo): string {
    return update.assetName.toLowerCase().endsWith('.dmg')
      ? 'Download DMG'
      : 'Download Installer';
  }

  function installActionLabel(update: UpdateInfo | null): string {
    if (!update) return 'Install Now';
    return update.installMode === 'download' ? downloadActionLabel(update) : 'Install Now';
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

<svelte:window onkeydown={handleBetaModalKeydown} />

<h2 class="settings-h">About</h2>
<div class="setting-row" data-setting-target="about-version">
  <div><div class="label">Version</div></div>
  <button class="version-tap desc" onclick={handleVersionTap}>v{appVersion}</button>
</div>
{#if devModeHintVisible}
  <div class="dev-hint-row">
    <span class="desc dev-hint">Developer mode enabled for this session.</span>
  </div>
{/if}
<div class="setting-row" data-setting-target="about-license">
  <div><div class="label">License</div></div>
  <span class="desc">MIT</span>
</div>
<div class="setting-row" data-setting-target="about-website">
  <div><div class="label">Website</div></div>
  <button class="btn-ghost" onclick={() => openExternal(WEBSITE_URL)}>verenu.com</button>
</div>
<div class="setting-row" data-setting-target="about-source">
  <div><div class="label">Source</div></div>
  <button class="btn-ghost" onclick={() => openExternal(`https://github.com/${SOURCE_REPO}`)}>github.com/{SOURCE_REPO}</button>
</div>
<div class="setting-row" data-setting-target="about-setup">
  <div>
    <div class="label">Setup</div>
    <div class="desc">Re-run onboarding to review your provider, key, and defaults.</div>
  </div>
  <button class="btn-ghost" onclick={rerunSetup}>Re-run setup</button>
</div>
<div class="setting-row" data-setting-target="about-updates">
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
<div class="setting-row" data-setting-target="about-beta">
  <div>
    <div class="label">Beta updates</div>
    <div class="desc">Try early releases from the development branch. Expect bugs and possible data loss.</div>
  </div>
  <Toggle checked={betaUpdatesEnabled} onchange={handleBetaUpdatesToggle} label="Beta updates" />
</div>

{#if confirmBetaUpdates}
  <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
  <button class="modal-backdrop" aria-label="Close dialog" onclick={() => (confirmBetaUpdates = false)} in:modalBackdrop={{ duration: 180 }} out:modalBackdrop={{ duration: 160 }}></button>
  <div
    class="modal-card"
    use:modalFocusTrap={{
      active: confirmBetaUpdates,
      initialFocus: () => betaCancelButton,
    }}
    role="dialog"
    aria-modal="true"
    aria-labelledby="beta-updates-confirm-title"
    tabindex="-1"
    in:modalCard={{ duration: 220, distance: motionPx(MOTION_PX.panel), scaleFrom: 0.97 }}
    out:modalCard={{ duration: 160, distance: motionPx(MOTION_PX.nudge), scaleFrom: 0.985 }}
  >
    <div class="modal-header">
      <h2 id="beta-updates-confirm-title" class="modal-title">Enable beta updates?</h2>
    </div>
    <div class="modal-body">
      <p class="confirm-copy">
        Beta releases contain unfinished code from the development branch. They can be unstable,
        break features, or cause data loss. Only enable this if you want to test early builds.
      </p>
    </div>
    <div class="modal-footer">
      <div class="footer-actions">
        <button bind:this={betaCancelButton} class="btn-ghost" onclick={() => (confirmBetaUpdates = false)}>Cancel</button>
        <button class="btn-primary" onclick={confirmEnableBetaUpdates} disabled={savingBetaUpdates}>Enable beta updates</button>
      </div>
    </div>
  </div>
{/if}

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

  .modal-backdrop {
    position: fixed;
    inset: 0;
    border: 0;
    padding: 0;
    appearance: none;
    background: var(--overlay);
    z-index: 50;
    outline: none;
  }
  .modal-card {
    position: fixed;
    top: 50%;
    left: 50%;
    translate: -50% -50%;
    z-index: 51;
    isolation: isolate;
    background: var(--bg-elev);
    border: 1px solid var(--line);
    border-radius: var(--r-lg);
    width: min(420px, calc(100vw - 40px));
    box-shadow: var(--shadow-elev);
    overflow: hidden;
  }
  .modal-header { padding: 20px 20px 0; }
  .modal-title {
    font-family: var(--sans);
    font-size: 17px;
    font-weight: 600;
    letter-spacing: -0.01em;
    color: var(--ink);
    margin: 0;
  }
  .modal-body { padding: 10px 20px 18px; }
  .confirm-copy {
    margin: 0;
    font-size: 13px;
    line-height: 1.5;
    color: var(--ink-soft);
  }
  .modal-footer { padding: 0 20px 20px; }
  .footer-actions { display: flex; justify-content: flex-end; gap: 8px; }

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
