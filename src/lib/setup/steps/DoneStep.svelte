<script lang="ts">
  import { onMount } from 'svelte';
  import { isMac } from '../../platform';

  const hkKey1 = isMac ? 'fn' : 'Ctrl';
  const hkKey2 = isMac ? 'Control' : 'Windows';

  let {
    providerName,
    cleanupName,
    toneName,
    languageLabel,
    appearanceName,
    hasKey,
  }: {
    providerName: string;
    cleanupName: string;
    toneName: string;
    languageLabel: string;
    appearanceName: string;
    hasKey: boolean;
  } = $props();

  let checkAnimating = $state(false);
  onMount(() => {
    const t = setTimeout(() => { checkAnimating = true; }, 200);
    return () => clearTimeout(t);
  });
</script>

<div class="step done-step">
  <div class="done-check-wrap">
    <svg class="done-check" class:animate={checkAnimating} width="64" height="64" viewBox="0 0 64 64" fill="none">
      <circle cx="32" cy="32" r="28" stroke="var(--accent-soft)" stroke-width="6"/>
      <circle cx="32" cy="32" r="28" stroke="var(--accent)" stroke-width="6"
        stroke-dasharray="176"
        stroke-dashoffset={checkAnimating ? '0' : '176'}
        stroke-linecap="round"
        style="transition: stroke-dashoffset 0.6s cubic-bezier(0.4,0,0.2,1); transform: rotate(-90deg); transform-origin: 32px 32px;"
      />
      <polyline
        points="20,33 28,41 44,24"
        stroke="var(--accent)"
        stroke-width="4.5"
        stroke-linecap="round"
        stroke-linejoin="round"
        stroke-dasharray="36"
        stroke-dashoffset={checkAnimating ? '0' : '36'}
        style="transition: stroke-dashoffset 0.4s 0.5s cubic-bezier(0.4,0,0.2,1);"
      />
    </svg>
  </div>
  <h2 class="done-title">You're all set.</h2>
  <p class="done-sub">
    Hold <kbd>{hkKey1}</kbd> + <kbd>{hkKey2}</kbd> anywhere to start dictating.
    Verenu lives in your system tray and is always ready.
  </p>

  {#if !hasKey}
    <div class="done-warning">No API key set — add one in Settings → API Keys before dictating.</div>
  {/if}

  <div class="done-summary">
    <div class="summary-item">
      <span class="summary-label">Provider</span>
      <span class="summary-val">{providerName}</span>
    </div>
    <div class="summary-item">
      <span class="summary-label">Cleanup</span>
      <span class="summary-val">{cleanupName}</span>
    </div>
    <div class="summary-item">
      <span class="summary-label">Tone</span>
      <span class="summary-val">{toneName}</span>
    </div>
    <div class="summary-item">
      <span class="summary-label">Language</span>
      <span class="summary-val">{languageLabel}</span>
    </div>
    <div class="summary-item">
      <span class="summary-label">Theme</span>
      <span class="summary-val">{appearanceName}</span>
    </div>
  </div>
  <p class="done-note">Everything can be changed in Settings or the Style page.</p>
</div>

<style>
  .done-step { align-items: center; text-align: center; max-width: 440px; }

  .done-check-wrap { margin-bottom: 4px; }

  .done-title { font-family: var(--serif); font-size: 26px; font-weight: 500; color: var(--ink-strong); margin: 0; }

  .done-sub { font-size: 13.5px; color: var(--ink-mute); margin: 0; line-height: 1.5; }

  .done-warning {
    padding: 9px 12px;
    border-radius: var(--r-sm);
    border: 1px solid var(--warning-line);
    background: var(--warning-bg);
    color: var(--ink-soft);
    font-size: 12.5px;
    line-height: 1.45;
  }

  .done-summary {
    display: flex;
    gap: 16px;
    background: var(--paper-2);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    padding: 14px 20px;
    flex-wrap: wrap;
    justify-content: center;
  }

  .summary-item { display: flex; flex-direction: column; align-items: center; gap: 2px; }

  .summary-label {
    font-size: 10.5px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.07em;
    color: var(--ink-faint);
  }

  .summary-val { font-size: 13px; font-weight: 500; color: var(--ink-soft); }

  .done-note { font-size: 12px; color: var(--ink-faint); margin: 0; }
</style>
