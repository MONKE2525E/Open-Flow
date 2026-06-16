<script lang="ts">
  import { onMount } from 'svelte';
  import { appStore } from './lib/stores';
  import { cleanupPromptEditor } from './lib/stores.svelte';
  import { isMac } from './lib/platform';
  import TitleBar from './lib/components/layout/TitleBar.svelte';
  import Sidebar from './lib/components/layout/Sidebar.svelte';
  import Home from './lib/views/Home.svelte';
  import Dictionary from './lib/views/Dictionary.svelte';
  import Snippets from './lib/views/Snippets.svelte';
  import Style from './lib/views/Style.svelte';
  import Settings from './lib/views/Settings.svelte';
  import CleanupPromptModal from './lib/components/settings/CleanupPromptModal.svelte';
  import DictationPill from './lib/components/layout/DictationPill.svelte';
  import Setup from './lib/views/Setup.svelte';
  import { invoke, listen } from './lib/tauri';
  import { fly } from 'svelte/transition';
  import { expoOut } from 'svelte/easing';
  import { MOTION_MS, MOTION_PX, NAV_ORDER, directionFromOrder, motionMs, motionPx, pageSwap, reducedMotionEnabled } from './lib/motion';

  type EffectiveTheme = 'light' | 'dark';

  function systemTheme(): EffectiveTheme {
    return window.matchMedia?.('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
  }

  function effectiveTheme(mode: 'system' | 'light' | 'dark'): EffectiveTheme {
    return mode === 'system' ? systemTheme() : mode;
  }

  function applyTheme() {
    const theme = effectiveTheme(appStore.appearanceMode);
    document.documentElement.dataset.theme = theme;
  }

  $effect(() => {
    appStore.appearanceMode;
    if (typeof document !== 'undefined') applyTheme();
  });

  // Error toast
  let errorToast = $state('');
  let toastTimer: ReturnType<typeof setTimeout>;
  let pageDir = $state<1 | -1>(1);
  let prevPage = $state<string>('home');
  let contentEl = $state<HTMLDivElement | null>(null);

  $effect(() => {
    const next = appStore.currentPage;
    pageDir = directionFromOrder(prevPage, next, NAV_ORDER);
    prevPage = next;
  });

  $effect(() => {
    appStore.currentPage;
    requestAnimationFrame(() => {
      contentEl?.scrollTo({ top: 0, behavior: reducedMotionEnabled() ? 'auto' : 'smooth' });
    });
  });

  async function pingConnectivity() {
    try {
      const online = await invoke<boolean>('check_connectivity');
      appStore.isOnline = online;
    } catch {
      appStore.isOnline = false;
    }
  }

  onMount(() => {
    let cleanupFn: (() => void) | undefined;

    (async () => {
      try {
        const [done, appearance, forceSetupOnLaunch] = await Promise.all([
          invoke<boolean | null>('get_setting', { key: 'setup_complete' }),
          invoke<'system' | 'light' | 'dark' | null>('get_setting', { key: 'appearance_mode' }),
          invoke<boolean | null>('get_setting', { key: 'force_setup_on_launch' }),
        ]);
        appStore.setupComplete = forceSetupOnLaunch ? false : done === true;
        if (appearance === 'light' || appearance === 'dark' || appearance === 'system') {
          appStore.appearanceMode = appearance;
        }
      } catch {
        appStore.setupComplete = false;
      }

      const unlisten = await listen<string>('verenu:error', (ev) => {
        errorToast = ev.payload ?? 'Something went wrong';
        clearTimeout(toastTimer);
        toastTimer = setTimeout(() => { errorToast = ''; }, 5000);
      });
      cleanupFn = unlisten;
    })();

    const media = window.matchMedia?.('(prefers-color-scheme: dark)');
    const onSystemThemeChange = () => {
      if (appStore.appearanceMode === 'system') applyTheme();
    };
    media?.addEventListener?.('change', onSystemThemeChange);

    pingConnectivity();
    let connectivityTimer = setInterval(pingConnectivity, 20_000);
    const handleVisibility = () => {
      clearInterval(connectivityTimer);
      if (!document.hidden) {
        pingConnectivity();
        connectivityTimer = setInterval(pingConnectivity, 20_000);
      }
    };
    document.addEventListener('visibilitychange', handleVisibility);

    return () => {
      if (cleanupFn) cleanupFn();
      media?.removeEventListener?.('change', onSystemThemeChange);
      clearInterval(connectivityTimer);
      document.removeEventListener('visibilitychange', handleVisibility);
    };
  });
</script>

<div class="app">
  {#if appStore.setupComplete === false}
    <Setup />
  {/if}
  {#if !isMac}
    <TitleBar />
  {/if}
  <div class="body">
    <Sidebar />
    <div class="content scroll-styled" bind:this={contentEl}>
      {#key appStore.currentPage}
        <div
          class="page-wrapper"
          in:pageSwap={{ axis: 'y', distance: pageDir * motionPx(MOTION_PX.page), duration: motionMs(MOTION_MS.panel) }}
          out:pageSwap={{ axis: 'y', distance: -pageDir * motionPx(MOTION_PX.page), duration: motionMs(MOTION_MS.base + 40) }}
        >
          {#if appStore.currentPage === 'home'}
            <Home />
          {:else if appStore.currentPage === 'dictionary'}
            <Dictionary />
          {:else if appStore.currentPage === 'snippets'}
            <Snippets />
          {:else if appStore.currentPage === 'style'}
            <Style />
          {/if}
        </div>
      {/key}
    </div>
  </div>
  <Settings />
  {#if cleanupPromptEditor.open}
    <CleanupPromptModal />
  {/if}
  <DictationPill />

  {#if errorToast}
    <div class="error-toast" role="alert" style:bottom={!appStore.isOnline ? '66px' : '18px'}>
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" style="flex-shrink:0">
        <circle cx="12" cy="12" r="10"/><path d="M12 8v4M12 16h.01"/>
      </svg>
      <span>{errorToast}</span>
      <button class="toast-close" onclick={() => { errorToast = ''; clearTimeout(toastTimer); }}>✕</button>
    </div>
  {/if}
  {#if !appStore.isOnline}
    <div class="offline-toast" role="status" transition:fly={{ y: 6, duration: 180, easing: expoOut }}>
      <span class="offline-dot"></span>
      No internet connection
    </div>
  {/if}
</div>

<style>
  :global(:root[data-theme="legacy-unused"]) {
    /* Soft-amber — paper / surfaces */
    --amber-50: #f9f7f3;
    --amber-100: #f1ebe3;
    --amber-200: #d8c9b5;

    /* Japonica — accent (sparingly) */
    --jap-50: #fcf4f0;
    --jap-100: #f8e6dc;
    --jap-200: #f0cbb8;
    --jap-300: #e6a78b;
    --jap-400: #d97757;
    --jap-500: #cc5e3e;
    --jap-600: #c44632;
    --jap-700: #a3352b;

    /* Armadillo — text */
    --arm-200: #e8e5e3;
    --arm-300: #d8d3cf;
    --arm-400: #ada299;
    --arm-500: #7e7266;
    --arm-600: #5b554a;
    --arm-700: #4a433a;
    --arm-800: #2b2422;
    --arm-900: #1e1915;
    --arm-950: #0d0a08;

    /* Surfaces */
    --paper: var(--amber-50);
    --paper-2: var(--amber-100);
    --bg-elev: #ffffff;

    /* Ink */
    --ink: var(--arm-950);
    --ink-strong: var(--arm-800);
    --ink-soft: var(--arm-700);
    --ink-mute: var(--arm-500);
    --ink-faint: var(--arm-400);

    --line: var(--arm-200);
    --line-soft: #efeae3;
    --line-strong: var(--arm-300);

    --accent: var(--jap-400);
    --accent-ink: var(--jap-700);
    --accent-soft: var(--jap-100);

    --serif: 'Fraunces', Georgia, serif;
    --sans: 'Inter Tight', ui-sans-serif, system-ui, sans-serif;
    --mono: 'JetBrains Mono', ui-monospace, monospace;

    --r-sm: 8px;
    --r-md: 12px;
    --r-lg: 16px;

    --page-pad-x: clamp(18px, 3vw, 42px);
    --page-pad-y: clamp(16px, 2.4vw, 30px);
    --page-max: 1160px;
    --page-readable: 680px;
  }

  :global(*) {
    box-sizing: border-box;
  }

  :global(html, body) {
    margin: 0;
    padding: 0;
  }

  :global(body) {
    font-family: var(--sans);
    color: var(--ink-soft);
    background: var(--paper);
    font-size: 13.5px;
    line-height: 1.5;
    -webkit-font-smoothing: antialiased;
    font-feature-settings: 'ss01', 'cv11';
  }

  :global(button) {
    font-family: inherit;
    cursor: pointer;
  }

  :global(kbd) {
    font-family: var(--mono);
    font-size: 11px;
    background: var(--paper);
    border: 1px solid var(--line-strong);
    border-radius: 5px;
    padding: 1px 6px;
    color: var(--ink);
    font-weight: 500;
  }

  .app {
    width: 100%;
    height: 100vh;
    background: var(--paper);
    display: flex;
    flex-direction: column;
    font-family: var(--sans);
    position: relative;
  }

  .body {
    flex: 1;
    display: flex;
    min-height: 0;
    padding: 0 0 14px 14px;
    gap: 14px;
  }

  .content {
    flex: 1;
    background: transparent;
    overflow-y: auto;
    overflow-x: hidden;
    scrollbar-gutter: stable;
    position: relative;
    display: grid;
    justify-items: center;
    min-width: 0;
  }

  .content::-webkit-scrollbar-thumb { border: 3px solid var(--paper); }

  .page-wrapper {
    grid-area: 1 / 1;
    width: 100%;
    max-width: 100%;
    min-width: 0;
    min-height: calc(100% + 1px);
  }

  .error-toast {
    position: absolute;
    bottom: 18px;
    left: 50%;
    transform: translateX(-50%);
    background: var(--danger-bg);
    border: 1px solid var(--danger-line);
    border-radius: 8px;
    padding: 9px 14px;
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12.5px;
    color: var(--danger);
    box-shadow: var(--shadow-popover);
    z-index: 20;
    max-width: 480px;
    animation: toastIn 0.18s cubic-bezier(0.22, 1, 0.36, 1);
    transition: bottom 0.15s ease;
  }

  @keyframes toastIn {
    from { opacity: 0; transform: translateX(-50%) translateY(6px); }
    to   { opacity: 1; transform: translateX(-50%) translateY(0); }
  }

  .toast-close {
    background: transparent;
    border: none;
    color: var(--danger);
    opacity: 0.6;
    font-size: 11px;
    cursor: pointer;
    margin-left: 4px;
    padding: 0;
    line-height: 1;
  }
  .toast-close:hover { opacity: 1; }

  .offline-toast {
    position: absolute;
    bottom: 18px;
    left: 0;
    right: 0;
    margin-inline: auto;
    width: fit-content;
    background: var(--danger-bg);
    border: 1px solid var(--danger-line);
    border-radius: 8px;
    padding: 9px 14px;
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12.5px;
    color: var(--danger);
    box-shadow: var(--shadow-popover);
    z-index: 20;
    max-width: 480px;
  }

  .offline-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: currentColor;
    flex-shrink: 0;
    animation: dot-pulse 2s ease-in-out infinite;
  }

  @keyframes dot-pulse {
    0%, 100% { opacity: 1; }
    50%       { opacity: 0.35; }
  }
</style>
