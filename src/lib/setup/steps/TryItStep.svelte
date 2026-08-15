<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke, listen } from '../../tauri';
  import { isMac } from '../../platform';
  import { formatIpcError } from '../../stores.svelte';
  import { hotkeyCodes, hotkeyLabels, hotkeyWatchCodes, matchesHotkey } from '../../hotkey.svelte';

  const keyLabels = $derived(hotkeyLabels());
  const watchCodes = $derived(hotkeyWatchCodes());

  let sampleText = $state('');
  let errorMessage = $state('');
  let textareaEl = $state<HTMLTextAreaElement | null>(null);
  let localRecording = false;
  let localStartInFlight = false;
  let pressedHotkeyCodes = new Set<string>();
  let destroyed = false;

  let status = $derived(sampleText.trim().length > 0 ? 'success' : 'waiting');
  const errorTitle = $derived(
    /no speech|didn.t hear|too quiet/i.test(errorMessage)
      ? "We didn't hear any speech"
      : 'That recording did not go through',
  );
  const errorDetail = $derived(
    `${errorMessage.replace(/[.!?]+$/, '')}. Check your microphone, then hold the hotkey until you finish speaking.`,
  );

  function tryItFieldFocused() {
    return typeof document !== 'undefined' && document.activeElement === textareaEl;
  }

  function isSetupTryChord() {
    return matchesHotkey(pressedHotkeyCodes);
  }

  async function startLocalRecording() {
    if (localRecording || localStartInFlight || destroyed) return;
    localStartInFlight = true;
    errorMessage = '';
    try {
      await invoke('start_setup_try_recording');
      if (destroyed) {
        void invoke('stop_setup_try_recording');
        return;
      }
      localRecording = true;
    } catch (err) {
      const message = formatIpcError(err);
      if (!destroyed && !message.toLowerCase().includes('already recording')) {
        errorMessage = message;
      }
    } finally {
      localStartInFlight = false;
    }
  }

  async function stopLocalRecording() {
    if (!localRecording) return;
    localRecording = false;
    try {
      await invoke('stop_setup_try_recording');
    } catch (err) {
      errorMessage = formatIpcError(err);
    }
  }

  function handleSetupTryKeydown(event: KeyboardEvent) {
    if (isMac || !tryItFieldFocused() || !watchCodes.has(event.code)) return;
    pressedHotkeyCodes.add(event.code);
    if (!isSetupTryChord()) return;
    event.preventDefault();
    event.stopPropagation();
    void startLocalRecording();
  }

  function handleSetupTryKeyup(event: KeyboardEvent) {
    if (isMac || !watchCodes.has(event.code)) return;
    const wasChord = isSetupTryChord();
    pressedHotkeyCodes.delete(event.code);
    if (!wasChord) return;
    event.preventDefault();
    event.stopPropagation();
    void stopLocalRecording();
  }

  onMount(() => {
    textareaEl?.focus();

    let unlistenTranscribed: (() => void) | undefined;
    let unlistenError: (() => void) | undefined;

    listen<string>('verenu:transcribed', (ev) => {
      if (destroyed) return;
      errorMessage = '';
      // start_setup_try_recording runs the pipeline in event-only mode (no real
      // clipboard/Ctrl+V injection), so this event is the only way text reaches the
      // textarea. Always take the latest payload so trying multiple dictations in a
      // row keeps showing the most recent result instead of requiring "Try again".
      if (ev.payload) sampleText = ev.payload;
    }).then((unsub) => {
      if (destroyed) unsub();
      else unlistenTranscribed = unsub;
    });

    listen<string>('verenu:error', (ev) => {
      if (destroyed) return;
      errorMessage = ev.payload
        ? formatIpcError(ev.payload)
        : 'Something went wrong with that recording.';
    }).then((unsub) => {
      if (destroyed) unsub();
      else unlistenError = unsub;
    });

    // The OS sends keyup to whatever window has focus, not necessarily ours — if focus
    // is lost mid-chord (alt-tab, an OS popup, a system dialog), our keyup handler never
    // fires and the chord/recording would otherwise get stuck active. Blur is the backstop.
    function handleBlur() {
      if (localRecording) void stopLocalRecording();
      pressedHotkeyCodes.clear();
    }

    window.addEventListener('keydown', handleSetupTryKeydown, { capture: true });
    window.addEventListener('keyup', handleSetupTryKeyup, { capture: true });
    window.addEventListener('blur', handleBlur);

    return () => {
      destroyed = true;
      if (localRecording || localStartInFlight) void invoke('stop_setup_try_recording');
      unlistenTranscribed?.();
      unlistenError?.();
      window.removeEventListener('keydown', handleSetupTryKeydown, { capture: true });
      window.removeEventListener('keyup', handleSetupTryKeyup, { capture: true });
      window.removeEventListener('blur', handleBlur);
      pressedHotkeyCodes.clear();
    };
  });

  function reset() {
    sampleText = '';
    errorMessage = '';
    pressedHotkeyCodes.clear();
    textareaEl?.focus();
  }
</script>

<div class="step">
  <div class="tryit-callout">
    {#each keyLabels as k, i}
      {#if i > 0}<span>+</span>{/if}<kbd>{k}</kbd>
    {/each}
    <p><strong>1.</strong> Focus the field &nbsp; <strong>2.</strong> Hold the hotkey and speak &nbsp; <strong>3.</strong> Release to finish</p>
    {#if isMac && hotkeyCodes()[0] === 'F5'}
      <p class="tryit-note">Nothing happening? F5 may be opening macOS Dictation — turn it off in System Settings → Keyboard → Dictation, or hold Fn with F5.</p>
    {/if}
  </div>

  <textarea
    class="tryit-field"
    class:filled={status === 'success'}
    bind:value={sampleText}
    bind:this={textareaEl}
    placeholder="Click here first, then hold the hotkey and speak..."
    rows="3"
  ></textarea>

  {#if errorMessage}
    <div class="tryit-feedback tryit-error" role="alert">
      <span class="feedback-icon" aria-hidden="true">
        <svg width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="9"/><path d="M12 7v6"/><path d="M12 17h.01"/></svg>
      </span>
      <span class="feedback-copy">
        <strong>{errorTitle}</strong>
        <span>{errorDetail}</span>
      </span>
      <button class="btn-ghost btn-compact tryit-reset" onclick={reset}>Try again</button>
    </div>
  {:else if status === 'success'}
    <div class="tryit-feedback tryit-success" role="status">
      <span class="feedback-icon" aria-hidden="true">
        <svg width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="9"/><path d="m8 12 2.6 2.6L16.5 9"/></svg>
      </span>
      <span class="feedback-copy">
        <strong>Everything works</strong>
        <span>Transcription, cleanup, and text insertion all completed successfully.</span>
      </span>
      <button class="btn-ghost btn-compact tryit-reset" onclick={reset}>Try again</button>
    </div>
  {:else}
    <p class="tryit-hint">Nothing happens until you hold the hotkey; this field doesn't auto-fill.</p>
  {/if}
</div>

<style>
  .tryit-callout {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
    background: var(--paper-2);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    padding: 14px 16px;
  }

  .tryit-callout span { color: var(--ink-faint); padding: 0 3px; }

  .tryit-callout p {
    margin: 0 0 0 4px;
    font-size: 13px;
    color: var(--ink-soft);
    flex-basis: 100%;
  }

  .tryit-callout p strong { color: var(--accent-ink); font-weight: 650; }

  .tryit-callout .tryit-note {
    margin-top: 6px;
    font-size: 11.5px;
    line-height: 1.5;
    color: var(--ink-mute);
  }

  .tryit-field {
    width: 100%;
    resize: none;
    border: 1.5px solid var(--line-strong);
    border-radius: var(--r-sm);
    background: var(--bg-elev);
    color: var(--ink);
    font-family: var(--sans);
    font-size: 13.5px;
    line-height: 1.5;
    padding: 12px 14px;
    outline: none;
    transition: border-color 0.2s, background 0.2s;
  }

  .tryit-field:focus { border-color: var(--accent); }
  .tryit-field.filled { border-color: var(--success-line); background: var(--success-bg); }

  .tryit-hint { font-size: 12px; color: var(--ink-faint); margin: 0; }

  .tryit-feedback {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 12px;
    border-radius: var(--r-sm);
  }

  .tryit-success {
    border: 1px solid var(--success-line);
    background: var(--success-bg);
  }

  .tryit-error {
    border: 1px solid var(--danger-line);
    background: var(--danger-bg);
  }

  .feedback-icon { display: flex; flex-shrink: 0; }
  .tryit-success .feedback-icon { color: var(--success); }
  .tryit-error .feedback-icon { color: var(--danger); }

  .feedback-copy {
    display: flex;
    flex: 1;
    min-width: 0;
    flex-direction: column;
    gap: 2px;
    text-align: left;
  }
  .feedback-copy strong { color: var(--ink-strong); font-size: 12px; font-weight: 650; }
  .feedback-copy > span { color: var(--ink-mute); font-size: 11.5px; line-height: 1.4; }

  .tryit-reset {
    margin-left: auto;
    flex-shrink: 0;
  }

  @media (max-width: 680px) {
    .tryit-feedback { align-items: flex-start; }
  }
</style>
