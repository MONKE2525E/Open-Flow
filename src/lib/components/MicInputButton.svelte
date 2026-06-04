<script lang="ts">
  import { invoke } from '../tauri';

  let { onResult }: { onResult: (text: string) => void } = $props();

  type MicState = 'idle' | 'recording' | 'loading';
  let micState = $state<MicState>('idle');
  let error = $state('');

  async function toggle() {
    error = '';
    if (micState === 'idle') {
      try {
        await invoke('start_input_recording');
        micState = 'recording';
      } catch (e) {
        const msg = String(e);
        error = msg.includes('Already recording') ? 'Hotkey recording is active'
              : msg.includes('Microphone access is blocked') ? 'Enable microphone permission in System Settings'
              : msg.includes('Accessibility permission is required') ? 'Enable Accessibility permission in System Settings'
              : 'Could not start mic';
      }
    } else if (micState === 'recording') {
      micState = 'loading';
      try {
        const text = await invoke<string>('stop_and_transcribe_input');
        onResult(text);
        micState = 'idle';
      } catch (e) {
        const msg = String(e);
        error = msg.includes('too short') ? 'Too short — try again'
              : msg.includes('No API key') ? 'No API key configured'
              : msg.includes('Microphone access is blocked') ? 'Enable microphone permission in System Settings'
              : msg.includes('Accessibility permission is required') ? 'Enable Accessibility permission in System Settings'
              : 'Transcription failed';
        micState = 'idle';
      }
    }
  }
</script>

<div class="mic-wrap">
  <button
    class="mic-btn"
    class:recording={micState === 'recording'}
    onclick={toggle}
    disabled={micState === 'loading'}
    aria-label={micState === 'recording' ? 'Stop and transcribe' : 'Transcribe with microphone'}
    title={micState === 'recording' ? 'Click to stop and transcribe' : 'Click to record'}
  >
    {#if micState === 'loading'}
      <span class="spin"></span>
    {:else if micState === 'recording'}
      <svg width="11" height="11" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
        <rect x="5" y="5" width="14" height="14" rx="2"/>
      </svg>
    {:else}
      <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
        <rect x="9" y="3" width="6" height="11" rx="3"/>
        <path d="M5 10a7 7 0 0 0 14 0M12 19v3M8 22h8"/>
      </svg>
    {/if}
  </button>
  {#if error}
    <p class="mic-error">{error}</p>
  {/if}
</div>

<style>
  .mic-wrap {
    position: relative;
    flex-shrink: 0;
  }

  .mic-btn {
    width: 30px;
    height: 30px;
    background: var(--bg-elev);
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    display: grid;
    place-items: center;
    color: var(--ink-mute);
    cursor: pointer;
    transition: background 0.12s, color 0.12s, border-color 0.12s;
    flex-shrink: 0;
  }

  .mic-btn:hover:not(:disabled) {
    background: var(--control-active);
    color: var(--ink-strong);
    border-color: var(--line-strong);
  }

  .mic-btn.recording {
    background: var(--danger-bg);
    border-color: var(--danger-line);
    color: var(--danger);
    animation: pulse 1.2s ease-in-out infinite;
  }

  .mic-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .spin {
    display: block;
    width: 11px;
    height: 11px;
    border: 1.5px solid var(--line);
    border-top-color: var(--ink-soft);
    border-radius: 50%;
    animation: spin 0.6s linear infinite;
  }

  .mic-error {
    position: absolute;
    top: calc(100% + 4px);
    right: 0;
    white-space: nowrap;
    font-size: 10.5px;
    color: var(--danger);
    background: var(--danger-bg);
    border: 1px solid var(--danger-line);
    border-radius: 5px;
    padding: 3px 7px;
    z-index: 10;
    pointer-events: none;
  }

  @keyframes spin { to { transform: rotate(360deg); } }
  @keyframes pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.6; } }
</style>
