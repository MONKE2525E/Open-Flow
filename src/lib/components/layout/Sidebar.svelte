<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { currentPage, settingsOpen } from '../../stores';

  let memoryMb = $state(0);

  onMount(() => {
    const refresh = async () => { memoryMb = await invoke<number>('get_memory_mb'); };
    refresh();
    const id = setInterval(refresh, 2000);
    return () => clearInterval(id);
  });

  const navItems = [
    { id: 'home',       label: 'Home',       icon: 'home',     locked: false },
    { id: 'dictionary', label: 'Dictionary', icon: 'book',     locked: true  },
    { id: 'snippets',   label: 'Snippets',   icon: 'scissors', locked: true  },
    { id: 'style',      label: 'Style',      icon: 'type',     locked: false },
  ] as const;

  // 24×24 viewBox SVG paths
  const paths: Record<string, string> = {
    home:     `<path d="M3 11l9-8 9 8"/><path d="M5 10v10h14V10"/>`,
    book:     `<path d="M4 5a2 2 0 0 1 2-2h12v18H6a2 2 0 0 1-2-2z"/><path d="M8 7h8M8 11h6"/>`,
    scissors: `<circle cx="6" cy="6" r="3"/><circle cx="6" cy="18" r="3"/><path d="M20 4 8.12 15.88M14.47 14.48 20 20M8.12 8.12 12 12"/>`,
    type:     `<path d="M4 6V4h16v2"/><path d="M9 20h6M12 4v16"/>`,
    settings: `<circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.6 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.6a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/>`,
  };

  function nav(id: string) {
    if (id === 'settings') { $settingsOpen = true; return; }
    $currentPage = id as typeof $currentPage;
  }
</script>

<aside class="sidebar">
  <div class="brand">
    <div class="brand-mark">
      <span style="height:35%"></span>
      <span style="height:70%"></span>
      <span style="height:100%"></span>
      <span style="height:55%"></span>
      <span style="height:25%"></span>
    </div>
    <div class="brand-name">Open Flow</div>
  </div>

  <div class="nav-section">
    {#each navItems as item (item.id)}
      <div
        class="nav-item"
        class:active={$currentPage === item.id}
        class:locked={item.locked}
        role="button"
        tabindex={item.locked ? -1 : 0}
        onclick={() => !item.locked && nav(item.id)}
        onkeydown={(e) => e.key === 'Enter' && !item.locked && nav(item.id)}
      >
        <!-- svelte-ignore html-self-closing -->
        <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">{@html paths[item.icon]}</svg>
        <span>{item.label}</span>
        {#if item.locked}
          <span class="lock-tag">Soon</span>
        {/if}
      </div>
    {/each}
  </div>

  <div class="sidebar-spacer"></div>

  <div class="sidebar-foot">
    <div
      class="nav-item"
      role="button"
      tabindex="0"
      onclick={() => nav('settings')}
      onkeydown={(e) => e.key === 'Enter' && nav('settings')}
    >
      <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">{@html paths.settings}</svg>
      <span>Settings</span>
    </div>
  </div>

  <div class="local-bar">
    <div class="local-bar-row">
      <span class="local-dot"></span>
      <span>Running locally</span>
      <span class="meta">{memoryMb} MB</span>
    </div>
    <div class="local-meter-thin"><span style="width:{Math.min(memoryMb / 200 * 100, 100)}%"></span></div>
  </div>
</aside>

<style>
  .sidebar {
    width: 220px;
    background: var(--bg-elev);
    border-radius: var(--r-md);
    border: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    flex-shrink: 0;
    overflow: hidden;
  }

  .brand {
    padding: 16px 18px 14px;
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .brand-mark {
    width: 18px;
    height: 18px;
    display: flex;
    align-items: flex-end;
    gap: 2px;
  }

  .brand-mark span {
    width: 2px;
    background: var(--accent);
    border-radius: 1px;
    display: block;
  }

  .brand-name {
    font-family: var(--serif);
    font-size: 17px;
    letter-spacing: -0.015em;
    font-weight: 500;
    color: var(--ink);
    white-space: nowrap;
  }

  .nav-section {
    padding: 4px 8px;
  }

  .nav-item {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 7px 10px;
    border-radius: 7px;
    color: var(--ink-soft);
    cursor: pointer;
    font-size: 13px;
    font-weight: 450;
    user-select: none;
    position: relative;
  }

  .nav-item :global(svg) { opacity: 0.75; flex-shrink: 0; }

  .nav-item:hover { color: var(--ink-strong); background: var(--amber-50); }

  .nav-item.active {
    color: var(--ink);
    font-weight: 500;
    background: var(--amber-100);
  }
  .nav-item.active :global(svg) { opacity: 1; }

  .nav-item.locked {
    color: var(--ink-faint);
    cursor: default;
  }
  .nav-item.locked:hover { background: transparent; color: var(--ink-faint); }
  .nav-item.locked :global(svg) { opacity: 0.5; }

  .lock-tag {
    margin-left: auto;
    font-family: var(--mono);
    font-size: 9px;
    color: var(--ink-mute);
    padding: 1px 6px;
    border-radius: 999px;
    font-weight: 500;
    letter-spacing: 0.04em;
    border: 1px solid var(--line);
  }

  .sidebar-spacer { flex: 1; }

  .sidebar-foot {
    padding: 6px 8px 8px;
    border-top: 1px solid var(--line-soft);
    margin: 0 8px;
  }

  .local-bar {
    margin: 4px 8px 10px;
    padding: 9px 10px;
    border-radius: 8px;
    background: var(--amber-100);
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .local-bar-row {
    display: flex;
    align-items: center;
    gap: 7px;
    font-size: 11px;
    color: var(--ink-soft);
  }

  .local-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--arm-700);
    flex-shrink: 0;
    display: block;
  }

  .meta {
    margin-left: auto;
    font-family: var(--mono);
    font-size: 10px;
    color: var(--ink-mute);
    font-variant-numeric: tabular-nums;
  }

  .local-meter-thin {
    height: 2px;
    background: var(--amber-200);
    border-radius: 999px;
    overflow: hidden;
  }

  .local-meter-thin span {
    display: block;
    height: 100%;
    background: var(--jap-300);
    border-radius: 999px;
  }
</style>
