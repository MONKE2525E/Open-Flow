<script lang="ts">
  import { onMount } from 'svelte';

  type PillState = 'idle' | 'recording' | 'processing' | 'handsfree' | 'error';
  let state: PillState = 'idle';
  let errorMsg = '';

  const BARS = 12;
  const DOTS_PROC = 18;

  // Level from Rust is already 0–1 (raw_rms × 15, capped).
  // Gate: ignore anything below 4% of full scale (background noise).
  const GATE = 0.04;

  // Per-bar gain coefficients — bell curve so middle bars are taller, fixed at mount.
  const barGains: number[] = Array.from({ length: BARS }, (_, i) => {
    const center = Math.sin((i / (BARS - 1)) * Math.PI) * 0.35;
    return 0.45 + center + Math.random() * 0.2;
  });

  let barHeights: number[] = Array(BARS).fill(2);
  let smoothed = 0;
  let lastLevelTime = 0;
  let noiseArr: number[] = Array.from({ length: BARS }, () => Math.random());
  let lastNoiseT = 0;
  let rafId: number;

  function animateBars(time: number) {
    // Passive decay kicks in after 150ms of silence (between words is ~80–120ms,
    // so this only fires on deliberate pauses).
    if (time - lastLevelTime > 150) {
      smoothed = Math.max(0, smoothed * 0.972);
    }

    // Shift per-bar noise every ~140ms for organic independent movement
    if (time - lastNoiseT > 140) {
      noiseArr = noiseArr.map(v => v * 0.65 + Math.random() * 0.35);
      lastNoiseT = time;
    }

    // Level is already 0–1; no extra division needed
    if (smoothed < GATE) {
      barHeights = Array(BARS).fill(2);
    } else {
      const active = (smoothed - GATE) / (1 - GATE);
      barHeights = barGains.map((gain, i) => {
        const energy = active * gain * (0.45 + noiseArr[i] * 0.55);
        return 2 + energy * 14;
      });
    }

    rafId = requestAnimationFrame(animateBars);
  }

  onMount(() => {
    rafId = requestAnimationFrame(animateBars);

    (async () => {
      const { listen } = await import('@tauri-apps/api/event');

      await listen<string>('pill-state', (ev) => {
        state = (ev.payload as PillState) || 'idle';
        if (state !== 'recording' && state !== 'handsfree') smoothed = 0;
      });

      await listen<string>('open-flow:error', (ev) => {
        errorMsg = ev.payload ?? 'Failed';
      });

      await listen<number>('audio-level', (ev) => {
        const level = ev.payload ?? 0;
        if (level > smoothed) {
          smoothed = smoothed * 0.1 + level * 0.9;  // near-instant rise
        } else {
          smoothed = smoothed * 0.88 + level * 0.12; // ~1s fall at 20Hz
        }
        lastLevelTime = performance.now();
      });
    })();

    return () => cancelAnimationFrame(rafId);
  });

  async function confirmHandless() {
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('stop_handless_mode').catch(() => {});
    state = 'idle';
  }

  async function cancelHandless() {
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('stop_recording').catch(() => {});
    state = 'idle';
  }
</script>

<div class="wrap">
  {#if state === 'recording'}
    <div class="pill recording">
      {#each barHeights as h, i (i)}
        <div class="bar" style="height: {h}px"></div>
      {/each}
    </div>

  {:else if state === 'processing'}
    <div class="pill processing">
      <div class="dots">
        {#each { length: DOTS_PROC } as _, i (i)}
          <i style="animation-delay:{i * 0.08}s"></i>
        {/each}
      </div>
      <div class="spinner"></div>
    </div>

  {:else if state === 'error'}
    <div class="pill error">
      <div class="err-icon">!</div>
      <span class="err-text">Failed</span>
    </div>

  {:else if state === 'handsfree'}
    <div class="pill handsfree">
      <button class="hf-btn cancel" onclick={cancelHandless} aria-label="Cancel">
        <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round">
          <path d="M6 6l12 12M6 18 18 6"/>
        </svg>
      </button>
      <div class="bars-hf">
        {#each barHeights as h, i (i)}
          <div class="bar" style="height: {h}px"></div>
        {/each}
      </div>
      <button class="hf-btn confirm" onclick={confirmHandless} aria-label="Confirm">
        <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
          <path d="M20 6L9 17l-5-5"/>
        </svg>
      </button>
    </div>
  {/if}
</div>

<style>
  :global(*, *::before, *::after) { box-sizing: border-box; }
  :global(html, body, #pill-root) {
    margin: 0; padding: 0;
    background: transparent;
    overflow: hidden;
    width: 220px; height: 60px;
  }

  .wrap {
    width: 220px; height: 60px;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .pill {
    background: #0d0a08;
    color: white;
    border-radius: 999px;
    height: 34px;
    display: flex;
    align-items: center;
    justify-content: center;
    box-shadow: 0 8px 22px rgba(13,10,8,0.55), 0 0 0 1px rgba(255,255,255,0.07) inset;
  }

  /* Recording: snug wrap — 12 bars × 3px + 11 gaps × 2px + 14px padding = 72px */
  .pill.recording {
    gap: 2px;
    padding: 0 7px;
    width: 72px;
  }

  .bar {
    width: 3px;
    background: white;
    border-radius: 999px;
    flex-shrink: 0;
    /* Instant response — no CSS transition so bars snap cleanly */
  }

  /* Processing */
  .pill.processing { width: 120px; padding: 0 12px; gap: 8px; }

  .dots { display: flex; align-items: center; gap: 3px; flex: 1; justify-content: center; }
  .dots i {
    width: 2.5px; height: 2.5px;
    background: #ada299;
    border-radius: 50%; display: block; flex-shrink: 0;
  }
  .pill.processing .dots i { animation: dotfade 1.5s infinite; }
  @keyframes dotfade {
    0%, 100% { opacity: 0.3; }
    50%       { opacity: 1; }
  }

  .spinner {
    width: 11px; height: 11px; flex-shrink: 0;
    border-radius: 50%;
    border: 1.5px solid #4a433a;
    border-top-color: white;
    animation: spin 0.75s linear infinite;
  }
  @keyframes spin { to { transform: rotate(360deg); } }

  /* Error */
  .pill.error { width: 110px; gap: 7px; padding: 0 14px; background: #1a0806; }
  .err-icon {
    width: 15px; height: 15px; flex-shrink: 0;
    border-radius: 50%;
    background: #c44632;
    display: grid; place-items: center;
    font-size: 10px; font-weight: 700; color: white;
  }
  .err-text { font-size: 11px; font-weight: 500; color: #f8a090; white-space: nowrap; }

  /* Handsfree: 5px pad + 18px btn + 4px + bars(58px) + 4px + 18px btn + 5px = 112px       */
  /* Bars start at ~55px from window left — close to recording pill's 74px+7px bar start    */
  .pill.handsfree { width: 112px; padding: 0 5px; gap: 4px; }

  .bars-hf { display: flex; align-items: center; gap: 2px; flex: 1; justify-content: center; }

  .hf-btn {
    width: 18px; height: 18px;
    background: transparent; border: 0;
    display: grid; place-items: center;
    flex-shrink: 0; cursor: pointer;
    border-radius: 4px;
    padding: 0;
    transition: opacity 0.15s;
  }
  .hf-btn.cancel  { color: rgba(255,255,255,0.45); }
  .hf-btn.confirm { color: #d97757; }
  .hf-btn.cancel:hover  { color: rgba(255,255,255,0.85); }
  .hf-btn.confirm:hover { color: #f4a07a; }
</style>
