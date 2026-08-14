<script lang="ts">
  import { tick } from 'svelte';
  import { fade, fly } from 'svelte/transition';
  import { expoOut } from 'svelte/easing';
  import { motionMs } from '../../motion';

  let {
    enabled = $bindable(),
    phrase = $bindable(),
  }: { enabled: boolean; phrase: string } = $props();

  let phraseInput = $state<HTMLInputElement | null>(null);

  const phraseError = $derived.by(() => {
    const normalized = phrase.trim().replace(/\s+/g, ' ');
    const length = [...normalized].length;
    return enabled && (length < 5 || length > 80 || !/[\p{L}\p{N}]/u.test(normalized));
  });

  async function choose(enabledValue: boolean) {
    enabled = enabledValue;
    if (!enabledValue) return;
    await tick();
    phraseInput?.focus();
  }
</script>

<div class="step clipboard-phrase-step">
  <div class="clipboard-choice-grid" role="radiogroup" aria-label="Clipboard phrase">
    <button
      type="button"
      class="pick-card clipboard-choice"
      class:selected={enabled}
      role="radio"
      aria-checked={enabled}
      onclick={() => choose(true)}
    >
      <div class="clipboard-choice-head">
        <span class="clipboard-choice-icon" aria-hidden="true">
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><rect x="7" y="4" width="10" height="14" rx="2"/><path d="M5 8H4a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h8a2 2 0 0 0 2-2v-1"/></svg>
        </span>
        <div class="clipboard-choice-copy"><span class="clipboard-choice-title">Turn it on</span><span class="clipboard-choice-sub">Say a phrase to paste text</span></div>
        <div class="pick-radio" class:checked={enabled}></div>
      </div>
    </button>
    <button
      type="button"
      class="pick-card clipboard-choice"
      class:selected={!enabled}
      role="radio"
      aria-checked={!enabled}
      onclick={() => choose(false)}
    >
      <div class="clipboard-choice-head">
        <span class="clipboard-choice-icon muted" aria-hidden="true">
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"><path d="M12 6v6l4 2"/><circle cx="12" cy="12" r="9"/></svg>
        </span>
        <div class="clipboard-choice-copy"><span class="clipboard-choice-title">Not now</span><span class="clipboard-choice-sub">You can enable it later</span></div>
        <div class="pick-radio" class:checked={!enabled}></div>
      </div>
    </button>
  </div>

  {#if enabled}
    <div class="phrase-reveal" in:fly={{ y: 10, duration: motionMs(220), easing: expoOut }} out:fade={{ duration: motionMs(120) }}>
      <div class="phrase-reveal-head">
        <span class="phrase-reveal-label">Phrase to say</span>
        <span class="phrase-reveal-hint">Current clipboard text will be inserted</span>
      </div>
      <input
        bind:this={phraseInput}
        class="ui-input clipboard-phrase-input"
        bind:value={phrase}
        aria-invalid={phraseError ? 'true' : undefined}
        aria-describedby={phraseError ? 'clipboard-phrase-error' : undefined}
        autocomplete="off"
        spellcheck="false"
      />
      {#if phraseError}
        <p id="clipboard-phrase-error" class="phrase-error" role="alert">Use 5 to 80 characters and include a letter or number.</p>
      {/if}
    </div>
  {/if}

  <p class="clipboard-note">Clipboard text is pasted exactly as copied. It never goes to cleanup or History.</p>
</div>

<style>
  .clipboard-phrase-step { gap: 14px; }

  .clipboard-choice-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; }
  .clipboard-choice { min-height: 82px; justify-content: center; }
  .clipboard-choice-head { display: flex; align-items: center; gap: 10px; width: 100%; }
  .clipboard-choice-icon { display: grid; place-items: center; color: var(--accent-ink); flex-shrink: 0; }
  .clipboard-choice-icon.muted { color: var(--ink-faint); }
  .clipboard-choice.selected .clipboard-choice-icon { animation: clipboard-icon-settle 0.32s cubic-bezier(0.16, 1, 0.3, 1); }
  .clipboard-choice-copy { display: flex; flex: 1; min-width: 0; flex-direction: column; gap: 2px; text-align: left; }
  .clipboard-choice-title { color: var(--ink-strong); font-size: 13px; font-weight: 500; }
  .clipboard-choice-sub { color: var(--ink-mute); font-size: 11.5px; line-height: 1.35; }

  .phrase-reveal {
    position: relative;
    overflow: hidden;
    padding: 13px 14px 14px;
    background: color-mix(in srgb, var(--accent-soft) 44%, var(--paper-2));
    border: 1px solid color-mix(in srgb, var(--accent) 34%, var(--line));
    border-radius: var(--r-md);
  }
  .phrase-reveal::before {
    content: '';
    position: absolute;
    inset: 0 auto 0 0;
    width: 38%;
    pointer-events: none;
    background: linear-gradient(90deg, transparent, color-mix(in srgb, var(--accent) 13%, transparent), transparent);
    transform: translateX(-130%);
    animation: phrase-sweep 0.62s 0.06s cubic-bezier(0.22, 1, 0.36, 1) both;
  }
  .phrase-reveal-head { display: flex; justify-content: space-between; align-items: baseline; gap: 12px; margin-bottom: 8px; }
  .phrase-reveal-label { color: var(--ink-strong); font-size: 12px; font-weight: 600; }
  .phrase-reveal-hint { color: var(--ink-faint); font-size: 10.5px; text-align: right; }
  .clipboard-phrase-input {
    width: 100%;
    box-sizing: border-box;
    background: var(--bg-elev);
    animation: phrase-input-arrive 0.28s 0.08s cubic-bezier(0.16, 1, 0.3, 1) both;
  }
  .clipboard-phrase-input:focus { box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 20%, transparent); }
  .phrase-error { margin: 7px 0 0; color: var(--danger); font-size: 11px; }
  .clipboard-note { margin: 0; color: var(--ink-mute); font-size: 12px; line-height: 1.5; }

  @keyframes clipboard-icon-settle {
    from { transform: scale(0.72) rotate(-9deg); opacity: 0.35; }
    68% { transform: scale(1.08) rotate(1deg); opacity: 1; }
    to { transform: scale(1) rotate(0); opacity: 1; }
  }
  @keyframes phrase-sweep {
    from { transform: translateX(-130%); }
    to { transform: translateX(360%); }
  }
  @keyframes phrase-input-arrive {
    from { opacity: 0; transform: translateY(5px); }
    to { opacity: 1; transform: translateY(0); }
  }

  @media (prefers-reduced-motion: reduce) {
    .clipboard-choice.selected .clipboard-choice-icon,
    .phrase-reveal::before,
    .clipboard-phrase-input { animation: none; }
    .clipboard-phrase-input:focus { box-shadow: none; }
  }
  @media (max-width: 520px) {
    .clipboard-choice-grid { grid-template-columns: 1fr; }
    .phrase-reveal-head { align-items: flex-start; flex-direction: column; gap: 2px; }
    .phrase-reveal-hint { text-align: left; }
  }
</style>
