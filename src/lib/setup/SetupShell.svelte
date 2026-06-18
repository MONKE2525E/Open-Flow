<script lang="ts">
  import { isMac } from '../platform';

  type HeaderInfo = { title: string; subtitle: string; name: string } | null;

  let {
    step,
    totalSteps,
    header = null,
    onDotClick,
    onMinimize,
    onClose,
    left,
    right,
    children,
  }: {
    step: number;
    totalSteps: number;
    header?: HeaderInfo;
    onDotClick: (index: number) => void;
    onMinimize: () => void;
    onClose: () => void;
    left?: import('svelte').Snippet;
    right?: import('svelte').Snippet;
    children?: import('svelte').Snippet;
  } = $props();
</script>

<div class="setup-overlay">
  {#if !isMac}
    <div class="setup-titlebar" data-tauri-drag-region>
      <div></div>
      <div class="tb-right">
        <button class="tb-btn" title="Minimize" onclick={onMinimize}>
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
            <path d="M5 12h14"/>
          </svg>
        </button>
        <button class="tb-btn close" title="Close" onclick={onClose}>
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
            <path d="M6 6l12 12M6 18 18 6"/>
          </svg>
        </button>
      </div>
    </div>
  {/if}

  <div class="setup-content">
    {#if header}
      <div class="setup-header">
        <div class="progress">
          {#each Array.from({ length: totalSteps }) as _, i}
            <button
              class="dot"
              class:active={i + 1 === step}
              class:done={i + 1 < step}
              onclick={() => onDotClick(i + 1)}
              aria-label="Step {i + 1}"
            ></button>
          {/each}
        </div>
        <p class="step-label">Step {step} of {totalSteps} · {header.name}</p>
        <h2>{header.title}</h2>
        <p class="step-sub">{header.subtitle}</p>
      </div>
    {/if}

    <div class="setup-body" class:no-header={!header}>
      {@render children?.()}
    </div>
  </div>

  <div class="setup-actionbar">
    <div class="actionbar-left">{@render left?.()}</div>
    <div class="actionbar-right">{@render right?.()}</div>
  </div>
</div>

<style>
  /* ── Overlay / shell ───────────────────────────────────────────────── */
  .setup-overlay {
    position: fixed;
    inset: 0;
    z-index: 100;
    background: var(--paper);
    display: flex;
    flex-direction: column;
    align-items: center;
    overflow: hidden;
  }

  .setup-titlebar {
    width: 100%;
    height: 32px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 10px;
    flex-shrink: 0;
  }

  .tb-right { display: flex; align-items: center; gap: 2px; }

  .tb-btn {
    width: 24px;
    height: 24px;
    display: grid;
    place-items: center;
    background: transparent;
    border: 0;
    border-radius: 6px;
    color: var(--ink-mute);
    cursor: pointer;
  }

  .tb-btn:hover { background: var(--paper-2); color: var(--ink-strong); }
  .tb-btn.close:hover { background: var(--danger); color: var(--on-accent); }

  /* ── Header: stepper + title, fixed spot — never moves between steps ── */
  .setup-header {
    width: 100%;
    max-width: 560px;
    padding: 0 28px;
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .progress { display: flex; gap: 6px; }

  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--line-strong);
    border: none;
    padding: 0;
    cursor: default;
    transition: background 0.25s;
  }

  .dot.active {
    width: 20px;
    border-radius: 4px;
    background: var(--accent);
    cursor: default;
  }

  .dot.done {
    background: var(--accent);
    opacity: 0.45;
    cursor: pointer;
  }

  .step-label {
    margin: 4px 0 0;
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--ink-faint);
  }

  .setup-header h2 {
    font-family: var(--serif);
    font-size: 22px;
    font-weight: 500;
    color: var(--ink-strong);
    margin: 4px 0 0;
    line-height: 1.25;
  }

  .step-sub {
    font-size: 13px;
    color: var(--ink-mute);
    margin: 0;
    line-height: 1.5;
  }

  /* ── Scrollable content ───────────────────────────────────────────── */
  .setup-content {
    flex: 1;
    width: 100%;
    overflow-y: auto;
    overflow-x: hidden;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 24px;
    padding: 32px 0 24px;
  }

  /* ── Step body — fills the space below the fixed header ─────────────
     Steps with a header stretch (default) so content sits right under
     the header with a small, consistent gap. Header-less steps (Intro,
     Done) get margin:auto on .step instead, to read as a centered hero/
     closing screen — see .setup-body.no-header below. */
  .setup-body {
    flex: 1;
    min-height: 0;
    width: 100%;
    display: flex;
    justify-content: center;
  }

  .setup-body.no-header :global(.step) {
    margin: auto 0;
  }

  /* ── Pinned action bar ────────────────────────────────────────────── */
  .setup-actionbar {
    width: 100%;
    max-width: 560px;
    min-height: 80px;
    flex-shrink: 0;
    box-sizing: border-box;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 28px 22px;
  }

  .actionbar-left, .actionbar-right { display: flex; align-items: center; gap: 12px; }

  /* ── Shared design primitives, used by step components too ──────────
     Scoped by .setup-overlay ancestry rather than Svelte's per-file hash,
     since step components render these classes in their own templates. */
  :global(.setup-overlay .step) {
    width: 100%;
    max-width: 560px;
    padding: 0 28px;
    display: flex;
    flex-direction: column;
    gap: 24px;
  }

  :global(.setup-overlay .btn-primary) {
    background: var(--accent);
    color: var(--on-accent);
    border: none;
    border-radius: var(--r-sm);
    padding: 9px 22px;
    font-family: var(--sans);
    font-size: 13.5px;
    font-weight: 500;
    cursor: pointer;
    transition: opacity 0.15s, transform 0.1s;
  }

  :global(.setup-overlay .btn-primary:hover) { opacity: 0.88; }
  :global(.setup-overlay .btn-primary:active) { transform: scale(0.98); }
  :global(.setup-overlay .btn-primary:disabled) { opacity: 0.45; cursor: not-allowed; }

  :global(.setup-overlay .btn-primary.btn-lg) {
    padding: 11px 32px;
    font-size: 14.5px;
    border-radius: var(--r-md);
  }

  :global(.setup-overlay .btn-skip) {
    background: transparent;
    border: none;
    color: var(--ink-faint);
    font-family: var(--sans);
    font-size: 12.5px;
    cursor: pointer;
    padding: 0;
    transition: color 0.15s;
  }

  :global(.setup-overlay .btn-skip:hover) { color: var(--ink-mute); }

  :global(.setup-overlay .btn-ghost) {
    background: transparent;
    border: 1px solid var(--line-strong);
    color: var(--ink-mute);
    cursor: pointer;
    transition: background 0.15s, color 0.15s, border-color 0.15s;
  }

  :global(.setup-overlay .btn-ghost:hover) {
    background: var(--paper-2);
    color: var(--ink-strong);
    border-color: var(--accent);
  }

  :global(.setup-overlay .btn-ghost:disabled) { opacity: 0.5; cursor: not-allowed; }

  :global(.setup-overlay .btn-primary--glow) { animation: glow-pulse 0.85s ease-out forwards; }

  @keyframes glow-pulse {
    0%   { box-shadow: 0 0 0 0   color-mix(in srgb, var(--accent) 45%, transparent); }
    55%  { box-shadow: 0 0 0 7px color-mix(in srgb, var(--accent) 0%,  transparent); }
    100% { box-shadow: 0 0 0 0   transparent; }
  }
</style>
