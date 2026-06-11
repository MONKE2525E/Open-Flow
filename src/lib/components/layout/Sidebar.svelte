<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '../../tauri';
  import { appStore } from '../../stores';
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
    { id: 'dictionary', label: 'Dictionary', icon: 'book',     locked: false },
    { id: 'snippets',   label: 'Snippets',   icon: 'scissors', locked: false },
    { id: 'style',      label: 'Style',      icon: 'type',     locked: false },
  ] as const;

  function nav(id: string) {
    if (id === 'settings') { appStore.settingsOpen = true; return; }
    appStore.currentPage = id as typeof appStore.currentPage;
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
    <div class="brand-name">Verenu</div>
  </div>

  <div class="nav-section">
    {#each navItems as item (item.id)}
      <button
        type="button"
        class="nav-item"
        class:active={appStore.currentPage === item.id}
        disabled={item.locked}
        onclick={() => nav(item.id)}
      >
        <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width={appStore.currentPage === item.id ? '2.2' : '1.6'} stroke-linecap="round" stroke-linejoin="round">{@html icons[item.icon]}</svg>
        <span>{item.label}</span>
        {#if item.locked}
          <span class="lock-tag">Soon</span>
        {/if}
      </button>
    {/each}
  </div>

  <div class="sidebar-spacer"></div>

  <div class="sidebar-foot">
    <button
      type="button"
      class="nav-item"
      onclick={() => nav('settings')}
    >
      <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">{@html icons.settings}</svg>
      <span>Settings</span>
    </button>
  </div>

  <div class="local-bar">
    <div class="local-bar-row">
      <span class="local-dot"></span>
      <span>Running locally</span>
      <div class="meta-wrapper">
        <span class="meta">
          {#each String(rawMemoryMb).split('') as digit, i (i)}
            <span class="digit-slot">
              {#key digit}
                <span
                  class="digit-char"
                  in:fly={{ y: memoryDir * 10, duration: 400, easing: expoOut }}
                  out:fly={{ y: -memoryDir * 10, duration: 400, easing: expoOut }}
                >{digit}</span>
              {/key}
            </span>
          {/each}<span class="meta-unit"> MB</span>
        </span>
      </div>
    </div>
    <div class="local-meter-thin"><span style="width:{Math.min($memoryMb / 200 * 100, 100)}%; background:{$memoryMb >= 150 ? 'var(--accent)' : 'var(--arm-300, #9caa8e)'}"></span></div>
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
    background: var(--accent);
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
    border: 0;
    background: transparent;
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
    text-align: left;
    width: 100%;
  }

  .nav-item :global(svg) { opacity: 0.75; flex-shrink: 0; }

  .nav-item:hover { color: var(--ink-strong); background: var(--control-hover); }
  .nav-item:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }

  .nav-item.active {
    color: var(--ink);
    font-weight: 500;
    background: var(--control-active);
  }
  .nav-item.active :global(svg) { opacity: 1; }

  .nav-item:disabled {
    color: var(--ink-faint);
    cursor: default;
    opacity: 1;
  }
  .nav-item:disabled:hover { background: transparent; color: var(--ink-faint); }
  .nav-item:disabled :global(svg) { opacity: 0.5; }

  .lock-tag {
    margin-left: auto;
    font-family: var(--sans);
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
    background: var(--control-active);
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
    display: flex;
    align-items: center;
  }

  .meta {
    display: inline-flex;
    align-items: center;
    font-family: var(--mono);
    font-size: 10px;
    color: var(--ink-mute);
    font-variant-numeric: tabular-nums;
  }

  .digit-slot {
    position: relative;
    display: inline-block;
    overflow: hidden;
    width: 1ch;
    height: 14px;
  }

  .digit-char {
    position: absolute;
    top: 0;
    left: 0;
    width: 100%;
    text-align: center;
    line-height: 14px;
  }

  .meta-unit {
    font-family: var(--mono);
    font-size: 10px;
    color: var(--ink-mute);
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
    border-radius: 999px;
    transition: background 0.4s ease;
  }
</style>
