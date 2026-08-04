<script lang="ts">
  import { onMount } from 'svelte';
  import { hotkeyLabels } from '../../hotkey.svelte';
  import { reducedMotionEnabled } from '../../motion';

  let {
    providerName,
    cleanupName,
    toneName,
    languageLabel,
    micGain,
    usesHeadphones,
    hasKey,
    presetName,
  }: {
    providerName: string;
    presetName: string;
    cleanupName: string;
    toneName: string;
    languageLabel: string;
    micGain: number | null;
    usesHeadphones: boolean;
    hasKey: boolean;
  } = $props();

  const keyLabels = $derived(hotkeyLabels());
  const cleanupOff = $derived(cleanupName === 'Off');

  // The wizard no longer asks about these individually — it turns them all on.
  // Naming them here is the disclosure: auto-learn watches the focused field for
  // corrections and app context hint reads text around the caret.
  const smartProcessing = 'Cleanup, noise reduction, contextual caps, auto-spacing, caps-lock detection, app context hint, and auto-learn are all on.';

  let checkAnimating = $state(false);
  onMount(() => {
    if (reducedMotionEnabled()) {
      checkAnimating = true;
      return;
    }
    const t = setTimeout(() => { checkAnimating = true; }, 200);
    return () => clearTimeout(t);
  });

  const ringStyle = $derived(
    reducedMotionEnabled()
      ? 'transform: rotate(-90deg); transform-origin: 32px 32px;'
      : 'transition: stroke-dashoffset 0.6s cubic-bezier(0.4,0,0.2,1); transform: rotate(-90deg); transform-origin: 32px 32px;'
  );
  const checkStyle = $derived(
    reducedMotionEnabled() ? '' : 'transition: stroke-dashoffset 0.4s 0.5s cubic-bezier(0.4,0,0.2,1);'
  );
</script>

<div class="step done-step">
  <div class="done-check-wrap">
    <svg class="done-check" width="64" height="64" viewBox="0 0 64 64" fill="none">
      <circle cx="32" cy="32" r="28" stroke="var(--accent-soft)" stroke-width="6"/>
      <circle cx="32" cy="32" r="28" stroke="var(--accent)" stroke-width="6"
        stroke-dasharray="176"
        stroke-dashoffset={checkAnimating ? '0' : '176'}
        stroke-linecap="round"
        style={ringStyle}
      />
      <polyline
        points="20,33 28,41 44,24"
        stroke="var(--accent)"
        stroke-width="4.5"
        stroke-linecap="round"
        stroke-linejoin="round"
        stroke-dasharray="36"
        stroke-dashoffset={checkAnimating ? '0' : '36'}
        style={checkStyle}
      />
    </svg>
  </div>
  <h2 class="done-title">You're all set.</h2>
  <p class="done-sub">
    Hold {#each keyLabels as k, i}{#if i > 0}<span class="done-plus">+</span>{/if}<kbd>{k}</kbd>{/each}
    anywhere to start dictating. Verenu lives in your system tray and is always ready.
  </p>

  {#if !hasKey}
    <div class="done-warning">No API key set — add one in Settings → API Keys before dictating.</div>
  {/if}

  <div class="done-summary">
    <div class="summary-group">
      <span class="summary-label">Provider</span>
      <span class="summary-val">{presetName || providerName}</span>
      <span class="summary-meta">{presetName ? providerName : hasKey ? 'Key saved' : 'No key yet'}</span>
    </div>
    <div class="summary-group">
      <span class="summary-label">Writing</span>
      <span class="summary-val">{cleanupOff ? 'Cleanup off' : `${cleanupName} cleanup`}</span>
      <span class="summary-meta">{cleanupOff ? 'Text injected as transcribed' : `${toneName} tone`}</span>
    </div>
    <div class="summary-group">
      <span class="summary-label">Audio</span>
      <span class="summary-val">{languageLabel}</span>
      <span class="summary-meta">
        {usesHeadphones ? 'Headphones' : 'Speakers'}{micGain !== null ? ` · mic ${micGain.toFixed(1)}×` : ''}
      </span>
    </div>
  </div>

  <p class="done-defaults">{smartProcessing} Everything here can be changed in Settings or the Style page.</p>
</div>

<style>
  .done-step { align-items: center; text-align: center; max-width: 480px; }

  .done-check-wrap { margin-bottom: 4px; }

  .done-title { font-family: var(--serif); font-size: 26px; font-weight: 500; color: var(--ink-strong); margin: 0; }

  .done-sub { font-size: 13.5px; color: var(--ink-mute); margin: 0; line-height: 1.7; }

  .done-sub kbd {
    font-family: var(--mono);
    font-size: 11.5px;
    background: var(--bg-elev);
    border: 1px solid var(--line-strong);
    border-radius: 5px;
    padding: 2px 7px;
    color: var(--ink-soft);
  }

  .done-plus { color: var(--ink-faint); padding: 0 3px; }

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
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 4px;
    width: 100%;
    background: var(--paper-2);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    padding: 14px 8px;
  }

  .summary-group {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 3px;
    padding: 0 10px;
    min-width: 0;
  }

  .summary-group + .summary-group { border-left: 1px solid var(--line); }

  .summary-label {
    font-size: 10.5px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.07em;
    color: var(--ink-faint);
  }

  .summary-val { font-size: 13px; font-weight: 500; color: var(--ink-strong); }

  .summary-meta { font-size: 11px; color: var(--ink-mute); line-height: 1.35; }

  .done-defaults {
    font-size: 11.5px;
    color: var(--ink-mute);
    margin: 0;
    line-height: 1.5;
    max-width: 400px;
  }

  @media (max-width: 700px) {
    .done-summary { grid-template-columns: 1fr; }
    .summary-group + .summary-group { border-left: none; border-top: 1px solid var(--line); padding-top: 10px; }
  }
</style>
