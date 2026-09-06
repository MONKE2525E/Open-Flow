<script lang="ts">
  import type { ProviderStatusAlert } from '../../stores';
  import type { ProviderId } from '../../settings';
  import { getProviderLogo } from '../../setup/ProviderLogos';
  import { fly, fade } from 'svelte/transition';
  import { expoOut } from 'svelte/easing';
  import { MOTION_MS, MOTION_PX, motionMs, motionPx } from '../../motion';

  let { alerts }: { alerts: ProviderStatusAlert[] } = $props();

  function providerLogo(alert: ProviderStatusAlert): string | null {
    const id = alert.providerId.toLowerCase();
    if (!['groq', 'openai', 'google', 'assemblyai', 'local'].includes(id)) return null;
    return getProviderLogo(id as ProviderId);
  }

  async function openDetails(url: string) {
    if (!url.startsWith('https://') && !url.startsWith('http://')) {
      console.warn('Blocked opening non-HTTP(S) URL:', url);
      return;
    }
    try {
      const { open } = await import('@tauri-apps/plugin-shell');
      await open(url);
    } catch {
      window.open(url, '_blank');
    }
  }
</script>

{#if alerts.length > 0}
  <div class="notice-wrap">
    <div
      class="status-banner"
      in:fly={{ y: -motionPx(MOTION_PX.nudge), duration: motionMs(MOTION_MS.panel), easing: expoOut }}
      out:fade={{ duration: motionMs(MOTION_MS.fast) }}
    >
      {#each alerts as alert, i (alert.providerId + '-' + i)}
        <div class="status-row">
          {#if providerLogo(alert)}
            <span class="provider-logo" aria-hidden="true">{@html providerLogo(alert) ?? ''}</span>
          {/if}
          <span class="status-text"><strong>{alert.providerName}</strong> {alert.message}</span>
          {#if alert.detailsUrl}
            <button class="status-link" onclick={() => openDetails(alert.detailsUrl)}>Check status</button>
          {/if}
        </div>
      {/each}
    </div>
  </div>
{/if}

<style>
  .notice-wrap {
    position: relative;
    margin-bottom: 22px;
  }

  .status-banner {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px 18px;
    background: var(--danger-bg);
    border: 1px solid var(--danger-line);
    border-radius: var(--r-lg);
    font-size: 13px;
    color: var(--danger);
  }

  .status-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 14px;
  }

  .status-text {
    flex: 1;
    font-family: var(--sans);
    font-weight: 500;
  }

  .provider-logo {
    display: inline-flex;
    width: 20px;
    height: 20px;
    flex: 0 0 20px;
    color: currentColor;
  }
  .provider-logo :global(svg) { width: 100%; height: 100%; }

  .status-link {
    flex-shrink: 0;
    padding: 6px 12px;
    background: transparent;
    color: var(--danger);
    border: 1px solid var(--danger-line);
    border-radius: 6px;
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
    transition: opacity 0.15s;
  }
  .status-link:hover {
    opacity: 0.75;
  }
</style>
