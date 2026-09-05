<script lang="ts">
  import { appStore } from '../../stores';
</script>

{#if appStore.pillState !== 'idle'}
  <div class="pill" class:recording={appStore.pillState === 'recording'} class:processing={appStore.pillState === 'processing'} class:handsfree={appStore.pillState === 'handsfree'}>

    {#if appStore.pillState === 'recording'}
      <div class="bars">
        {#each { length: 18 } as _, i (i)}
          <i style="animation-delay:{i * 0.047}s"></i>
        {/each}
      </div>

    {:else if appStore.pillState === 'processing'}
      <div class="dots">
        {#each { length: 22 } as _, i (i)}
          <i style="animation-delay:{i * 0.073}s"></i>
        {/each}
      </div>
      <div class="spinner"></div>

    {:else if appStore.pillState === 'handsfree'}
      <button class="hf-btn cancel" aria-label="Cancel" onclick={async () => {
        const { invoke } = await import('@tauri-apps/api/core');
        await invoke('stop_recording').catch(() => {});
        appStore.pillState = 'idle';
      }}>
        <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round">
          <path d="M6 6l12 12M6 18 18 6"/>
        </svg>
      </button>
      <div class="bars">
        {#each { length: 18 } as _, i (i)}
          <i style="animation-delay:{i * 0.047}s"></i>
        {/each}
      </div>
      <button class="hf-btn confirm" aria-label="Confirm" onclick={async () => {
        const { invoke } = await import('@tauri-apps/api/core');
        await invoke('stop_handless_mode').catch(() => {});
        appStore.pillState = 'idle';
      }}>
        <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
          <path d="M20 6L9 17l-5-5"/>
        </svg>
      </button>
    {/if}

  </div>
{/if}

<style>
  .pill {
    position: fixed;
    bottom: 20px;
    left: 50%;
    transform: translateX(-50%);
    z-index: 999;
    background: var(--pill-bg);
    color: var(--pill-fg);
    border-radius: 999px;
    height: 30px;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0 3px;
    box-shadow: 0 0 0 1px rgba(255,255,255,0.05) inset;
  }

  .pill.recording  { width: 132px; }
  .pill.processing { width: 152px; padding-right: 8px; gap: 6px; }
  .pill.handsfree  { width: 112px; padding: 0 5px; gap: 4px; }

  /* Waveform bars */
  .bars {
    display: flex;
    align-items: center;
    gap: 1.5px;
    height: 16px;
  }

  .bars i {
    display: block;
    width: 2px;
    background: var(--pill-bar);
    border-radius: 1px;
    animation: barwave 0.85s infinite ease-in-out;
  }

  @keyframes barwave {
    0%, 100% { height: 3px; }
    50%       { height: 14px; }
  }

  /* Processing dots */
  .dots {
    display: flex;
    align-items: center;
    gap: 3px;
    flex: 1;
    justify-content: center;
  }

  .dots i {
    width: 2px;
    height: 2px;
    background: var(--arm-400);
    border-radius: 50%;
    display: block;
    flex-shrink: 0;
  }

  .pill.processing .dots i { animation: dotfade 1.6s infinite; }

  @keyframes dotfade {
    0%, 100% { opacity: 0.35; }
    50%       { opacity: 1;    }
  }

  /* Spinner */
  .spinner {
    width: 11px;
    height: 11px;
    border-radius: 50%;
    border: 1.5px solid var(--arm-700);
    border-top-color: var(--pill-fg);
    animation: spin 0.75s linear infinite;
    flex-shrink: 0;
  }

  @keyframes spin { to { transform: rotate(360deg); } }

  /* Handsfree buttons */
  .hf-btn {
    width: 18px; height: 18px;
    background: transparent; border: 0;
    display: grid; place-items: center;
    flex-shrink: 0; cursor: pointer;
    border-radius: 4px; padding: 0;
    transition: opacity 0.15s;
  }
  .hf-btn.cancel  { color: var(--pill-muted); }
  .hf-btn.confirm { color: var(--accent); }
  .hf-btn.cancel:hover  { color: var(--pill-muted-strong); }
  .hf-btn.confirm:hover { color: var(--accent-ink); }
</style>
