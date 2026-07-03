<script lang="ts">
  import type { ProviderStatusAlert } from '../../stores';

  export let alerts: ProviderStatusAlert[];

  async function openDetails(url: string) {
    try {
      const { open } = await import('@tauri-apps/plugin-shell');
      await open(url);
    } catch {
      window.open(url, '_blank');
    }
  }
</script>

<div class="notice-wrap">
  <div class="status-banner">
    {#each alerts as alert (alert.providerId)}
      <div class="status-row">
        <span class="status-text">{alert.providerName}: {alert.message}</span>
        <button class="status-link" onclick={() => openDetails(alert.detailsUrl)}>Check status</button>
      </div>
    {/each}
  </div>
</div>

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
    font-family: var(--serif);
    font-weight: 500;
  }

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
