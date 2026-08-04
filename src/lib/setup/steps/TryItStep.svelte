<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke, listen } from '../../tauri';
  import { isMac } from '../../platform';
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
      if (!destroyed && !String(err).includes('Already recording')) {
        errorMessage = String(err || 'Failed to start recording.');
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
      errorMessage = String(err || 'Failed to stop recording.');
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
      errorMessage = ev.payload || 'Something went wrong with that recording.';
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
    <p>Hold {keyLabels.length > 1 ? 'the keys' : 'the key'}, say a sentence, then release.</p>
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
    <p class="tryit-error">{errorMessage}</p>
  {:else if status === 'success'}
    <div class="tryit-success">
      <span class="status-icon">OK</span>
      <span>It works. That's the whole pipeline: transcription, cleanup, and injection.</span>
      <button class="tryit-reset" onclick={reset}>Try again</button>
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

  .tryit-callout kbd {
    font-family: var(--mono);
    font-size: 12px;
    background: var(--bg-elev);
    border: 1px solid var(--line-strong);
    border-radius: 6px;
    padding: 3px 8px;
  }

  .tryit-callout span { color: var(--ink-faint); font-size: 12px; }

  .tryit-callout p {
    margin: 0 0 0 4px;
    font-size: 13px;
    color: var(--ink-soft);
    flex-basis: 100%;
  }

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
  .tryit-field.filled { border-color: var(--accent); background: var(--accent-soft); }

  .tryit-hint { font-size: 12px; color: var(--ink-faint); margin: 0; }

  .tryit-error { font-size: 12.5px; color: var(--danger); margin: 0; }

  .tryit-success {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12.5px;
    color: var(--ink-soft);
  }

  .status-icon {
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--accent-soft);
    color: var(--accent-ink);
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 10px;
    font-weight: 700;
    flex-shrink: 0;
  }

  .tryit-reset {
    margin-left: auto;
    background: transparent;
    border: 1px solid var(--line-strong);
    border-radius: 6px;
    padding: 3px 10px;
    font-size: 11.5px;
    color: var(--ink-mute);
    cursor: pointer;
    flex-shrink: 0;
  }

  .tryit-reset:hover { color: var(--ink-strong); border-color: var(--accent); }
</style>
