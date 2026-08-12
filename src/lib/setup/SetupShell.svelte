<script lang="ts">
  type HeaderInfo = { title: string; subtitle: string; name: string } | null;

  let {
    step,
    totalSteps,
    header = null,
    onDotClick,
    left,
    right,
    children,
  }: {
    step: number;
    totalSteps: number;
    header?: HeaderInfo;
    onDotClick: (index: number) => void;
    left?: import('svelte').Snippet;
    right?: import('svelte').Snippet;
    children?: import('svelte').Snippet;
  } = $props();
</script>

<div class="setup-overlay">
  <div class="setup-content">
    {#if header}
      <div class="setup-header">
        <div class="progress">
          {#each Array.from({ length: totalSteps }) as _, i}
            <button
              class="dot"
              class:active={i + 1 === step}
              class:done={i + 1 < step}
              disabled={i + 1 > step}
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
    /* One column width for header, body and action bar so nothing steps out
       of line. Individual steps widen it locally when they genuinely need to. */
    --setup-col: 620px;

    position: fixed;
    inset: 0;
    z-index: 100;
    background: var(--paper);
    display: flex;
    flex-direction: column;
    align-items: center;
    overflow: hidden;
  }

  /* ── Header: stepper + title, fixed spot — never moves between steps ── */
  .setup-header {
    width: 100%;
    max-width: var(--setup-col);
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
     Content sits directly under the header; the action bar stays pinned to
     the bottom. A grid (rather than flex) so the outgoing and incoming steps
     can occupy the same cell during a transition instead of briefly stacking
     and shoving the layout down. Header-less steps (Intro, Done) get
     margin:auto on .step to read as a centered hero — see .no-header below. */
  .setup-body {
    flex: 1;
    min-height: 0;
    width: 100%;
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    align-content: start;
    justify-items: center;
  }

  .setup-body > :global(*) { grid-column: 1; grid-row: 1; }

  .setup-body.no-header { align-content: center; }

  .setup-body.no-header :global(.step) {
    margin: auto 0;
  }

  /* ── Pinned action bar ────────────────────────────────────────────── */
  .setup-actionbar {
    width: 100%;
    max-width: var(--setup-col);
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
    max-width: var(--setup-col);
    padding: 0 28px;
    display: flex;
    flex-direction: column;
    gap: 24px;
  }

  /* ── Shared selection card ──────────────────────────────────────────
     One affordance for every choice in the wizard: providers, cleanup
     intensity, tone, language, headphones. Previously each step invented
     its own (outlined radio here, solid accent disc with a white tick
     there), which is what made the tick read as out-of-theme. */
  :global(.setup-overlay .pick-card) {
    background: var(--bg-elev);
    border: 1.5px solid var(--line);
    border-radius: var(--r-sm);
    padding: 11px 13px;
    text-align: left;
    cursor: pointer;
    display: flex;
    flex-direction: column;
    gap: 3px;
    font-family: var(--sans);
    transition: border-color 0.16s ease, background 0.16s ease, transform 0.12s ease;
  }

  :global(.setup-overlay .pick-card:hover:not(.selected)) {
    border-color: var(--line-strong);
    background: var(--paper-2);
  }

  :global(.setup-overlay .pick-card.selected) {
    border-color: var(--accent);
    background: var(--accent-soft);
  }

  :global(.setup-overlay .pick-card:active) { transform: scale(0.985); }

  :global(.setup-overlay .pick-card:focus-visible) {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }

  :global(.setup-overlay .pick-radio) {
    width: 14px;
    height: 14px;
    border-radius: 50%;
    border: 2px solid var(--line-strong);
    flex-shrink: 0;
    position: relative;
    transition: border-color 0.16s ease;
  }

  :global(.setup-overlay .pick-radio.checked) { border-color: var(--accent); }

  :global(.setup-overlay .pick-radio.checked::after) {
    content: '';
    position: absolute;
    inset: 2px;
    border-radius: 50%;
    background: var(--accent);
    animation: pick-dot 0.16s ease-out;
  }

  @keyframes pick-dot {
    from { transform: scale(0.3); opacity: 0; }
    to   { transform: scale(1);   opacity: 1; }
  }

  /* Uppercase group label above a set of pick-cards. */
  :global(.setup-overlay .group-label) {
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--ink-faint);
    margin: 0;
  }

  @media (prefers-reduced-motion: reduce) {
    :global(.setup-overlay .pick-card) { transition: none; }
    :global(.setup-overlay .pick-radio.checked::after) { animation: none; }
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
  :global(.setup-overlay .btn-skip:disabled) { opacity: 0.4; cursor: not-allowed; }

  :global(.setup-overlay .btn-back) {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    background: transparent;
    border: none;
    color: var(--ink-faint);
    font-family: var(--sans);
    font-size: 12.5px;
    cursor: pointer;
    padding: 4px 6px 4px 0;
    transition: color 0.15s;
  }

  :global(.setup-overlay .btn-back:hover) { color: var(--ink-strong); }
  :global(.setup-overlay .btn-back:disabled) { opacity: 0.4; cursor: not-allowed; }

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
