<script lang="ts">
  import { onMount } from 'svelte';

  type AppWindow = {
    minimize: () => Promise<void>;
  };

  let win: AppWindow | null = null;

  onMount(async () => {
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window');
      win = getCurrentWindow();
    } catch {}
  });

  function minimize() { win?.minimize(); }

  async function close() {
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      await invoke('hide_main');
    } catch {}
  }
</script>

<div class="titlebar" data-tauri-drag-region>
  <div class="tb-left"></div>
  <div class="tb-right">
    <button class="tb-btn" title="Minimize" aria-label="Minimize window" onclick={minimize}>
      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true">
        <path d="M5 12h14"/>
      </svg>
    </button>
    <button class="tb-btn close" title="Close" aria-label="Close window" onclick={close}>
      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true">
        <path d="M6 6l12 12M6 18 18 6"/>
      </svg>
    </button>
  </div>
</div>

<style>
  .titlebar {
    height: 32px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 10px;
    background: transparent;
    flex-shrink: 0;
  }

  .tb-left, .tb-right {
    display: flex;
    align-items: center;
    gap: 2px;
  }

  .tb-btn {
    width: 24px;
    height: 24px;
    padding: 0;
    display: grid;
    place-items: center;
    background: transparent;
    border: 0;
    border-radius: 6px;
    color: var(--ink-mute);
    cursor: pointer;
  }

  .tb-btn:hover { background: var(--control-active); color: var(--ink-strong); }
  .tb-btn.close:hover { background: var(--danger); color: var(--on-accent); }

</style>
