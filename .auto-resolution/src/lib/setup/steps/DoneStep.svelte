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
  const isLocal = $derived(providerName === 'On this device' || providerName === 'Local');

  // The wizard no longer asks about these individually — it turns them all on.
  // Naming them here is the disclosure: auto-learn watches the focused field for
  // corrections and app context hint reads text around the caret.
  const smartProcessing = 'Noise reduction, spacing, capitalization, app context, and correction learning are on.';

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
  <p class="done-sub">Verenu is ready in your system tray. Use the same three-step rhythm anywhere you can type.</p>

  {#if !hasKey}
    <div class="done-warning">No API key set — add one in Settings → API Keys before dictating.</div>
  {/if}

  <div class="done-quickstart" aria-label="How to dictate">
    <div class="quick-step">
      <span class="quick-number"><span>1</span></span>
      <span><strong>Hold</strong><small>{#each keyLabels as k, i}{#if i > 0}<span class="done-plus"> + </span>{/if}<kbd>{k}</kbd>{/each}</small></span>
    </div>
    <div class="quick-arrow" aria-hidden="true">→</div>
    <div class="quick-step"><span class="quick-number"><span>2</span></span><span><strong>Speak</strong><small>Keep holding</small></span></div>
    <div class="quick-arrow" aria-hidden="true">→</div>
    <div class="quick-step"><span class="quick-number"><span>3</span></span><span><strong>Release</strong><small>Text appears</small></span></div>
  </div>

  <div class="done-summary" aria-label="Your setup">
    <div class="summary-group">
      <span class="summary-label">Provider</span>
      <span class="summary-val">{presetName || providerName}</span>
      <span class="summary-meta">{isLocal ? 'Runs locally' : presetName ? providerName : hasKey ? 'Key saved' : 'No key yet'}</span>
    </div>
    <div class="summary-group">
      <span class="summary-label">Writing</span>
      <span class="summary-val">{cleanupOff ? 'Cleanup off' : `${cleanupName} cleanup`}</span>
      <span class="summary-meta">{cleanupOff ? 'Text injected as transcribed' : `${toneName} tone`}</span>
    </div>
    <div class="summary-group">
      <span class="summary-label">Language</span>
      <span class="summary-val">{languageLabel}</span>
      <span class="summary-meta">Spoken language</span>
    </div>
    <div class="summary-group">
      <span class="summary-label">Audio</span>
      <span class="summary-val">{usesHeadphones ? 'Headphones' : 'Speakers'}</span>
      <span class="summary-meta">{micGain != null ? `Mic tuned to ${micGain.toFixed(1)}×` : 'Default mic gain'}</span>
    </div>
  </div>

  <div class="done-footer">
    <span>{smartProcessing}</span>
    <span class="done-change"><strong>Settings</strong> changes models and audio <i>·</i> <strong>Style</strong> changes cleanup and tone</span>
  </div>
</div>

<style>
  .done-step { align-items: center; text-align: center; max-width: 600px; gap: 13px; }

  .done-check-wrap { margin-bottom: 4px; }

  .done-title { font-family: var(--sans); font-size: 23px; font-weight: 600; color: var(--ink-strong); margin: 0; }

  .done-sub { font-size: 13px; color: var(--ink-mute); margin: 0; line-height: 1.55; }

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

  .done-quickstart {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr) auto minmax(0, 1fr);
    align-items: center;
    width: 100%;
    padding: 11px 14px;
    border: 1px solid color-mix(in srgb, var(--accent) 34%, var(--line));
    border-radius: var(--r-md);
    background: color-mix(in srgb, var(--accent-soft) 42%, var(--paper-2));
  }
  .quick-step { display: grid; grid-template-columns: 24px minmax(0, 1fr); align-items: center; gap: 9px; min-width: 0; text-align: left; }
  .quick-step > span:last-child { display: flex; flex-direction: column; gap: 2px; }
  .quick-number { display: grid; place-items: center; width: 24px; height: 24px; box-sizing: border-box; padding: 0; border: 1px solid color-mix(in srgb, var(--accent) 48%, var(--line)); border-radius: 50%; color: var(--accent-ink); font-size: 10.5px; font-weight: 700; font-variant-numeric: tabular-nums; line-height: 1; background: var(--bg-elev); }
  .quick-number > span { display: block; width: 1ch; line-height: 1; text-align: center; }
  .quick-step strong { color: var(--ink-strong); font-size: 11.5px; font-weight: 650; }
  .quick-step small { color: var(--ink-mute); font-size: 10px; white-space: nowrap; }
  .quick-step kbd { font-size: 9px; padding: 1px 4px; }
  .quick-arrow { padding: 0 10px; color: var(--accent); opacity: .72; }

  .done-summary {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 0;
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
    font-weight: 500;
    text-transform: none;
    letter-spacing: 0;
    color: var(--ink-faint);
  }

  .summary-val { font-size: 13px; font-weight: 500; color: var(--ink-strong); }

  .summary-meta { font-size: 11px; color: var(--ink-mute); line-height: 1.35; }

  .done-footer {
    display: flex;
    flex-direction: column;
    gap: 3px;
    font-size: 10.5px;
    color: var(--ink-mute);
    line-height: 1.45;
    max-width: 500px;
  }
  .done-change { color: var(--ink-faint); }
  .done-change strong { color: var(--ink-mute); font-weight: 600; }
  .done-change i { padding: 0 4px; font-style: normal; }

  @media (max-height: 660px) {
    .done-step { gap: 9px; }
    .done-check { width: 50px; height: 50px; }
    .done-title { font-size: 23px; }
    .done-summary { padding: 10px 8px; }
  }

  @media (max-width: 700px) {
    .done-summary { grid-template-columns: repeat(2, 1fr); }
    .summary-group:nth-child(3) { border-left: none; }
    .summary-group:nth-child(n + 3) { border-top: 1px solid var(--line); padding-top: 8px; margin-top: 8px; }
    .quick-arrow { padding: 0 3px; }
  }
</style>
