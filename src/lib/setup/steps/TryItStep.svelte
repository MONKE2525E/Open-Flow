<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke, listen } from '../../tauri';
  import { isMac } from '../../platform';

  const hkKey1 = isMac ? 'fn' : 'Ctrl';
  const hkKey2 = isMac ? 'Control' : 'Windows';
  const setupTryHotkeyCodes = new Set(['ControlLeft', 'ControlRight', 'MetaLeft', 'MetaRight']);

  let sampleText = $state('');
  let errorMessage = $state('');
  let textareaEl = $state<HTMLTextAreaElement | null>(null);
  let localRecording = false;
  let localStartInFlight = false;
  let pressedHotkeyCodes = new Set<string>();

  let status = $derived(sampleText.trim().length > 0 ? 'success' : 'waiting');

  function tryItFieldFocused() {
    return typeof document !== 'undefined' && document.activeElement === textareaEl;
  }

  function isSetupTryChord() {
    const ctrlHeld = pressedHotkeyCodes.has('ControlLeft') || pressedHotkeyCodes.has('ControlRight');
    const metaHeld = pressedHotkeyCodes.has('MetaLeft') || pressedHotkeyCodes.has('MetaRight');
    return ctrlHeld && metaHeld;
  }

  async function startLocalRecording() {
    if (localRecording || localStartInFlight) return;
    localStartInFlight = true;
    errorMessage = '';
    try {
      await invoke('start_setup_try_recording');
      localRecording = true;
    } catch (err) {
      if (!String(err).includes('Already recording')) {
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
    if (isMac || !tryItFieldFocused() || !setupTryHotkeyCodes.has(event.code)) return;
    pressedHotkeyCodes.add(event.code);
    if (!isSetupTryChord()) return;
    event.preventDefault();
    event.stopPropagation();
    void startLocalRecording();
  }

  function handleSetupTryKeyup(event: KeyboardEvent) {
    if (isMac || !setupTryHotkeyCodes.has(event.code)) return;
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

    (async () => {
      unlistenTranscribed = await listen<string>('verenu:transcribed', (ev) => {
        errorMessage = '';
        // Real Ctrl+V into the focused textarea should have already landed via bind:value.
        // If it didn't (focus lost mid-paste, etc.), fall back to the event payload directly.
        setTimeout(() => {
          if (!sampleText.trim() && ev.payload) sampleText = ev.payload;
        }, 150);
      });
      unlistenError = await listen<string>('verenu:error', (ev) => {
        errorMessage = ev.payload || 'Something went wrong with that recording.';
      });
    })();
    window.addEventListener('keydown', handleSetupTryKeydown, { capture: true });
    window.addEventListener('keyup', handleSetupTryKeyup, { capture: true });

    return () => {
      if (localRecording) void stopLocalRecording();
      unlistenTranscribed?.();
      unlistenError?.();
      window.removeEventListener('keydown', handleSetupTryKeydown, { capture: true });
      window.removeEventListener('keyup', handleSetupTryKeyup, { capture: true });
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
    <kbd>{hkKey1}</kbd> <span>+</span> <kbd>{hkKey2}</kbd>
    <p>Hold the keys, say a sentence, then release.</p>
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
