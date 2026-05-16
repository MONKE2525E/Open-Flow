<script lang="ts">
  import { onMount } from 'svelte';
  import { currentPage, settingsOpen, accentColor, appearanceMode, setupComplete } from './lib/stores';
  import TitleBar from './lib/components/layout/TitleBar.svelte';
  import Sidebar from './lib/components/layout/Sidebar.svelte';
  import Home from './lib/views/Home.svelte';
  import Dictionary from './lib/views/Dictionary.svelte';
  import Snippets from './lib/views/Snippets.svelte';
  import Style from './lib/views/Style.svelte';
  import Settings from './lib/views/Settings.svelte';
  import DictationPill from './lib/components/layout/DictationPill.svelte';
  import Setup from './lib/views/Setup.svelte';
  import { fly } from 'svelte/transition';
  import { expoOut } from 'svelte/easing';

  type EffectiveTheme = 'light' | 'dark';

  const accentMap: Record<EffectiveTheme, Record<string, [string, string, string]>> = {
    light: {
      terracotta: ['oklch(0.62 0.14 40)',  'oklch(0.94 0.03 40)',   'oklch(0.42 0.12 40)'],
      moss:       ['oklch(0.55 0.1 145)',  'oklch(0.94 0.03 145)',  'oklch(0.4 0.1 145)' ],
      slate:      ['oklch(0.45 0.04 250)', 'oklch(0.94 0.015 250)', 'oklch(0.35 0.05 250)'],
      ink:        ['oklch(0.18 0.01 60)',  'oklch(0.92 0.005 70)',  'oklch(0.18 0.01 60)' ],
    },
    dark: {
      terracotta: ['oklch(0.70 0.13 42)',  '#3a241d',              '#f0a987'],
      moss:       ['oklch(0.72 0.10 145)', '#1f3022',              '#a7d99f'],
      slate:      ['oklch(0.72 0.04 250)', '#202532',              '#b7c4e3'],
      ink:        ['oklch(0.82 0.01 70)',  '#2a241d',              '#f2e6d5'],
    },
  };

  function systemTheme(): EffectiveTheme {
    return window.matchMedia?.('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
  }

  function effectiveTheme(mode: 'system' | 'light' | 'dark'): EffectiveTheme {
    return mode === 'system' ? systemTheme() : mode;
  }

  function applyTheme() {
    const theme = effectiveTheme($appearanceMode);
    document.documentElement.dataset.theme = theme;

    const accents = accentMap[theme];
    const [a, b, c] = accents[$accentColor] ?? accents.terracotta;
    document.documentElement.style.setProperty('--accent', a);
    document.documentElement.style.setProperty('--accent-soft', b);
    document.documentElement.style.setProperty('--accent-ink', c);
  }

  $: {
    $appearanceMode;
    $accentColor;
    if (typeof document !== 'undefined') applyTheme();
  }

  // Error toast
  let errorToast = '';
  let toastTimer: ReturnType<typeof setTimeout>;

  onMount(() => {
    let cleanupFn: (() => void) | undefined;

    (async () => {
      try {
        const { invoke } = await import('@tauri-apps/api/core');
        const [done, appearance] = await Promise.all([
          invoke<boolean | null>('get_setting', { key: 'setup_complete' }),
          invoke<'system' | 'light' | 'dark' | null>('get_setting', { key: 'appearance_mode' }),
        ]);
        setupComplete.set(done === true);
        if (appearance === 'light' || appearance === 'dark' || appearance === 'system') {
          appearanceMode.set(appearance);
        }
      } catch {
        setupComplete.set(false);
      }

      try {
        const { listen } = await import('@tauri-apps/api/event');
        const unlisten = await listen<string>('open-flow:error', (ev) => {
          errorToast = ev.payload ?? 'Something went wrong';
          clearTimeout(toastTimer);
          toastTimer = setTimeout(() => { errorToast = ''; }, 5000);
        });
        cleanupFn = unlisten;
      } catch {}
    })();

    const media = window.matchMedia?.('(prefers-color-scheme: dark)');
    const onSystemThemeChange = () => {
      if ($appearanceMode === 'system') applyTheme();
    };
    media?.addEventListener?.('change', onSystemThemeChange);

    return () => {
      if (cleanupFn) cleanupFn();
      media?.removeEventListener?.('change', onSystemThemeChange);
    };
  });
</script>

<svelte:head>
  <link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Fraunces:opsz,wght@9..144,400;9..144,500;9..144,600&family=Inter+Tight:wght@400;450;500;600&family=JetBrains+Mono:wght@400;500&display=swap">
</svelte:head>

<div class="app">
  {#if $setupComplete === false}
    <Setup />
  {/if}
  <TitleBar />
  <div class="body">
    <Sidebar />
    <div class="content scroll-styled">
      {#key $currentPage}
        <div class="page-wrapper" in:fly={{ y: 8, duration: 400, delay: 150, easing: expoOut }} out:fly={{ y: -8, duration: 150, easing: expoOut }}>
          {#if $currentPage === 'home'}
            <Home />
          {:else if $currentPage === 'dictionary'}
            <Dictionary />
          {:else if $currentPage === 'snippets'}
            <Snippets />
          {:else if $currentPage === 'style'}
            <Style />
          {/if}
        </div>
      {/key}
    </div>
  </div>
  <Settings />
  <DictationPill />

  {#if errorToast}
    <div class="error-toast" role="alert">
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" style="flex-shrink:0">
        <circle cx="12" cy="12" r="10"/><path d="M12 8v4M12 16h.01"/>
      </svg>
      <span>{errorToast}</span>
      <button class="toast-close" onclick={() => { errorToast = ''; clearTimeout(toastTimer); }}>✕</button>
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
    padding: 0 14px 14px 14px;
    gap: 14px;
  }

  .content {
    flex: 1;
    background: transparent;
    overflow-y: scroll;
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
    animation: toastIn 0.2s ease;
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
</style>
