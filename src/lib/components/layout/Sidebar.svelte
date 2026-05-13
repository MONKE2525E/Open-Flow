<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { currentPage, settingsOpen } from '../../stores';
  import { icons } from '../../icons';
  import { tweened } from 'svelte/motion';
  import { expoOut } from 'svelte/easing';
  import { fly } from 'svelte/transition';

  let rawMemoryMb = $state(0);
  let memoryDir = $state(1);
  let memoryMb = tweened(0, { duration: 800, easing: expoOut });

  onMount(() => {
    const refresh = async () => {
      try {
        const next = await invoke<number>('get_memory_mb');
        if (next !== rawMemoryMb) {
          memoryDir = next > rawMemoryMb ? 1 : -1;
          rawMemoryMb = next;
          memoryMb.set(next);
        }
      } catch { /* dev mode */ }
    };
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
        <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">{@html icons[item.icon]}</svg>
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
      <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">{@html icons.settings}</svg>
      <span>Settings</span>
    </div>
  </div>

  <div class="local-bar">
    <div class="local-bar-row">
      <span class="local-dot"></span>
      <span>Running locally</span>
      <div class="meta-wrapper">
        {#key rawMemoryMb}
          <span class="meta" in:fly={{ y: memoryDir * 10, duration: 400, easing: expoOut }} out:fly={{ y: -memoryDir * 10, duration: 400, easing: expoOut }}>
            {rawMemoryMb} MB
          </span>
        {/key}
      </div>
    </div>
    <div class="local-meter-thin"><span style="width:{Math.min($memoryMb / 200 * 100, 100)}%"></span></div>
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
    width: 23px;
    height: 18px;
    display: flex;
    align-items: flex-end;
    gap: 2px;
  }

  .brand-mark span {
    width: 3px;
    background: #d97757;
    border-radius: 999px;
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

  .meta-wrapper {
    margin-left: auto;
    position: relative;
    display: grid;
    overflow: hidden;
    height: 14px;
    align-items: center;
  }

  .meta {
    grid-area: 1 / 1;
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
