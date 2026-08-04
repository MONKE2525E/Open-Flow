<script lang="ts">
  import { onMount } from 'svelte';
  import { isMac } from '../../platform';
  import { hotkeyLabels } from '../../hotkey.svelte';
  import LogoMark from '../../components/layout/LogoMark.svelte';

  const platformTagline = isMac ? 'macOS' : 'Windows';
  const keyLabels = $derived(hotkeyLabels());

  let introReady = $state(false);
  onMount(() => {
    const t = setTimeout(() => { introReady = true; }, 60);
    return () => clearTimeout(t);
  });
</script>

<div class="step intro-step">
  <div class="intro-brand" class:ready={introReady}>
    <div class="intro-lockup">
      <div class="intro-mark">
        <LogoMark />
      </div>
      <div class="intro-wordmark">
        <h1 class="brand-name">Verenu</h1>
        <p class="brand-tagline">open-source AI dictation for {platformTagline}</p>
      </div>
    </div>
  </div>

  <div class="how-it-works" class:ready={introReady}>
    <p class="how-label">How it works</p>
    <div class="how-steps">
      <div class="how-step">
        <div class="how-num">1</div>
        <div>
          <strong>Hold {#each keyLabels as k, i}{#if i > 0}<span class="how-plus">+</span>{/if}<kbd>{k}</kbd>{/each}</strong>
          <p>Start recording. A floating pill shows your audio level.</p>
        </div>
      </div>
      <div class="how-step">
        <div class="how-num">2</div>
        <div>
          <strong>Release to transcribe</strong>
          <p>Your speech is sent to the AI provider and converted to text.</p>
        </div>
      </div>
      <div class="how-step">
        <div class="how-num">3</div>
        <div>
          <strong>Text appears instantly</strong>
          <p>Cleaned text is injected into whatever app you're focused on.</p>
        </div>
      </div>
    </div>
  </div>

  <p class="intro-note" class:ready={introReady}>Takes about 2 minutes · You can change anything later</p>
</div>

<style>
  .intro-step {
    align-items: center;
    text-align: center;
    max-width: 480px;
  }

  .intro-brand {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    opacity: 0;
    transform: translateY(14px);
    transition: opacity 0.5s ease, transform 0.5s ease;
  }

  .intro-brand.ready { opacity: 1; transform: none; }

  .intro-lockup { display: flex; align-items: center; gap: 16px; }

  .intro-mark {
    width: 48px;
    height: 40px;
    flex-shrink: 0;
    color: var(--accent);
  }

  .intro-mark :global(svg) { display: block; }

  .intro-wordmark { display: flex; flex-direction: column; gap: 2px; text-align: left; }

  .brand-name {
    font-family: var(--serif);
    font-size: 28px;
    font-weight: 500;
    color: var(--ink-strong);
    margin: 0;
    letter-spacing: -0.3px;
    line-height: 1.1;
  }

  .brand-tagline { font-size: 13px; color: var(--ink-mute); margin: 0; line-height: 1.3; }

  .how-it-works {
    background: var(--paper-2);
    border: 1px solid var(--line);
    border-radius: var(--r-lg);
    padding: 18px 22px;
    text-align: left;
    opacity: 0;
    transform: translateY(10px);
    transition: opacity 0.5s 0.15s ease, transform 0.5s 0.15s ease;
  }

  .how-it-works.ready { opacity: 1; transform: none; }

  .how-label {
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--ink-faint);
    margin: 0 0 14px;
  }

  .how-steps { display: flex; flex-direction: column; gap: 14px; }

  .how-step { display: flex; gap: 14px; align-items: flex-start; }

  /* Bare numeral on the title's baseline — no disc behind it. */
  .how-num {
    width: 14px;
    color: var(--accent);
    font-size: 12px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    display: flex;
    align-items: center;
    min-height: 24px;
    flex-shrink: 0;
  }

  .how-step strong {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 5px;
    min-height: 24px;
    font-size: 13px;
    color: var(--ink-soft);
    margin-bottom: 2px;
  }

  .how-step kbd {
    font-family: var(--mono);
    font-size: 11px;
    font-weight: 500;
    background: var(--bg-elev);
    border: 1px solid var(--line-strong);
    border-radius: 5px;
    padding: 2px 6px;
    color: var(--ink-soft);
  }

  .how-plus { color: var(--ink-faint); font-weight: 400; }

  .how-step p { margin: 0; font-size: 12.5px; color: var(--ink-mute); line-height: 1.4; }

  .intro-note {
    font-size: 12px;
    color: var(--ink-faint);
    margin: 0;
    opacity: 0;
    transform: translateY(8px);
    transition: opacity 0.5s 0.28s ease, transform 0.5s 0.28s ease;
  }

  .intro-note.ready { opacity: 1; transform: none; }

  @media (prefers-reduced-motion: reduce) {
    .intro-brand, .how-it-works, .intro-note {
      opacity: 1;
      transform: none;
      transition: none;
    }
  }
</style>
