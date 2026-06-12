<script lang="ts">
  import { onMount } from 'svelte';

  type PillState = 'idle' | 'recording' | 'processing' | 'handsfree' | 'error';
  let state: PillState = 'idle';
  let errorMsg = '';
  let showHfButtons = false;
  let hfTimer: ReturnType<typeof setTimeout> | null = null;
  let prevState: PillState = 'idle';
  let dying = false;
  let dyingTimer: ReturnType<typeof setTimeout> | null = null;

  const BARS = 12;

  // Level from Rust is already 0–1 (raw_rms × 15, capped).
  // Gate: ignore anything below 4% of full scale (background noise).
  const GATE = 0.04;

  // Per-bar gain coefficients — bell curve so middle bars are taller, fixed at mount.
  const barGains: number[] = Array.from({ length: BARS }, (_, i) => {
    const center = Math.sin((i / (BARS - 1)) * Math.PI) * 0.35;
    return 0.45 + center + Math.random() * 0.2;
  });

  let barHeights: number[] = Array(BARS).fill(2);
  let targetLevel = 0;
  let smoothed = 0;
  let lastLevelTime = 0;
  let targetNoiseArr: number[] = Array.from({ length: BARS }, () => Math.random());
  let currentNoiseArr: number[] = [...targetNoiseArr];
  let lastNoiseT = 0;
  let rafId = 0;
  const PEAK_FLOOR = 0.07;
  let adaptivePeak = PEAK_FLOOR;

  let lastAnimTime = 0;

  function animateBars(time: number) {
    if (!lastAnimTime) lastAnimTime = time;
    const dt = Math.min(time - lastAnimTime, 50);
    lastAnimTime = time;

    // Extremely smooth, premium rise and fall (high inertia)
    // Rise: ~12% per 16ms frame (takes ~100-150ms to swell up smoothly)
    const riseRate = 1 - Math.pow(0.88, dt / 16.66);
    // Fall: ~3% per 16ms frame (takes almost a full second to melt down, very lingering)
    const fallRate = 1 - Math.pow(0.97, dt / 16.66);
    
    if (targetLevel > smoothed) {
      smoothed += (targetLevel - smoothed) * riseRate;
    } else {
      smoothed += (targetLevel - smoothed) * fallRate;
    }

    if (smoothed > adaptivePeak) adaptivePeak = smoothed;

    // Peak decays very slowly to maintain a steady visual baseline
    adaptivePeak = Math.max(PEAK_FLOOR, adaptivePeak * Math.pow(0.9997, dt / 16.66));

    // Slow down the organic "lava lamp" noise shift (was 140ms, now 400ms)
    if (time - lastNoiseT > 400) {
      targetNoiseArr = targetNoiseArr.map(v => v * 0.7 + Math.random() * 0.3);
      lastNoiseT = time;
    }

    // Interpolate noise very slowly for a gentle breathing ripple
    const noiseLerpRate = 1 - Math.pow(0.95, dt / 16.66);
    for (let i = 0; i < BARS; i++) {
      currentNoiseArr[i] += (targetNoiseArr[i] - currentNoiseArr[i]) * noiseLerpRate;
    }

    // Gate on raw smoothed to suppress background noise; normalize against the
    // adaptive peak so quiet mics still drive bars to full height.
    if (smoothed < GATE) {
      barHeights = Array(BARS).fill(3);
    } else {
      const normalized = Math.min(smoothed / adaptivePeak, 1.0);
      // Ease the volume mapping (pow 1.5) so small noises are gentle
      const eased = Math.pow(normalized, 1.5);
      
      barHeights = barGains.map((gain, i) => {
        // Reduced noise influence (from 0.55 to 0.4) so it's less chaotic, more structured
        const energy = eased * gain * (0.6 + currentNoiseArr[i] * 0.4);
        return 3 + energy * 13;
      });
    }

    rafId = requestAnimationFrame(animateBars);
  }

  function startRaf() {
    if (rafId === 0) { lastAnimTime = 0; rafId = requestAnimationFrame(animateBars); }
  }
  function stopRaf() {
    if (rafId !== 0) { cancelAnimationFrame(rafId); rafId = 0; barHeights = Array(BARS).fill(3); }
  }


  function goIdle() {
    if (dying) return;
    dying = true;
    dyingTimer = setTimeout(() => {
      dying = false;
      dyingTimer = null;
      prevState = state;
      state = 'idle';
      smoothed = 0;
    }, 200);
  }

  onMount(() => {
    const unlisteners: Array<() => void> = [];
    let mounted = true;

    (async () => {
      const { listen } = await import('@tauri-apps/api/event');

      const l1 = await listen<string>('pill-state', (ev) => {
        const incoming = (ev.payload as PillState) || 'idle';
        if (hfTimer !== null) { clearTimeout(hfTimer); hfTimer = null; }

        if (incoming === 'idle' && (state === 'recording' || state === 'handsfree')) {
          stopRaf();
          goIdle();
          return;
        }

        if (dyingTimer !== null) { clearTimeout(dyingTimer); dyingTimer = null; dying = false; }

        if (incoming === 'handsfree') {
          showHfButtons = false;
          hfTimer = setTimeout(() => { showHfButtons = true; hfTimer = null; }, 150);
        }
        prevState = state;
        state = incoming;
        if (state === 'recording' || state === 'handsfree') {
          startRaf();
        } else {
          stopRaf();
        }
        if (state !== 'recording' && state !== 'handsfree') smoothed = 0;
      });
      if (!mounted) { l1(); return; }
      unlisteners.push(l1);

      const l2 = await listen<string>('verenu:error', (ev) => {
        errorMsg = ev.payload ?? 'Failed';
      });
      if (!mounted) { l2(); return; }
      unlisteners.push(l2);

      const l3 = await listen<number>('audio-level', (ev) => {
        targetLevel = ev.payload ?? 0;
        lastLevelTime = performance.now();
      });
      if (!mounted) { l3(); return; }
      unlisteners.push(l3);
    })();

    return () => {
      mounted = false;
      cancelAnimationFrame(rafId);
      unlisteners.forEach(u => u());
    };
  });

  async function confirmHandless() {
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('stop_handless_mode').catch(() => {});
    // Don't set state = 'idle' here — let Rust emit 'processing' next so the pill
    // morphs directly from handsfree to processing without disappearing first.
  }

  async function cancelHandless() {
    const { invoke } = await import('@tauri-apps/api/core');
    goIdle();
    await invoke('stop_recording').catch(() => {});
  }
</script>

<div class="wrap">
  {#if state === 'recording'}
    <div class="pill recording" class:dying={dying}>
      {#each barHeights as h, i (i)}
        <div class="bar" style="height: {h}px"></div>
      {/each}
    </div>

  {:else if state === 'processing'}
    <div class="pill processing" class:from-rec={prevState === 'recording'} class:from-hf={prevState === 'handsfree'}>
      <div class="scan-line"></div>
    </div>

  {:else if state === 'error'}
    <div class="pill error">
      <div class="err-icon">!</div>
      <span class="err-text">Failed</span>
    </div>

  {:else if state === 'handsfree'}
    <div class="pill handsfree" class:dying={dying} class:hf-expanded={showHfButtons && !dying} class:no-anim={prevState === 'recording'}>
      {#if showHfButtons}
        <button class="hf-btn cancel" onclick={cancelHandless} aria-label="Cancel">
          <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round">
            <path d="M6 6l12 12M6 18 18 6"/>
          </svg>
        </button>
      {/if}
      <div class="bars-hf">
        {#each barHeights as h, i (i)}
          <div class="bar" style="height: {h}px"></div>
        {/each}
      </div>
      {#if showHfButtons}
        <button class="hf-btn confirm" onclick={confirmHandless} aria-label="Confirm">
          <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
            <path d="M20 6L9 17l-5-5"/>
          </svg>
        </button>
      {/if}
    </div>
  {/if}
</div>

<style>
  :global(*, *::before, *::after) { box-sizing: border-box; }
  :global(html, body, #pill-root) {
    margin: 0; padding: 0;
    background: transparent;
    overflow: hidden;
    width: 140px; height: 44px;
  }

  .wrap {
    width: 140px; height: 44px;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .pill {
    background: var(--pill-bg);
    color: var(--pill-fg);
    border-radius: 999px;
    height: 34px;
    display: flex;
    align-items: center;
    justify-content: center;
    box-shadow: 0 0 0 1px rgba(255,255,255,0.07) inset;
    animation: pillIn 0.22s cubic-bezier(0.34, 1.56, 0.64, 1) both;
  }

  @keyframes pillIn {
    from { transform: translateY(8px) scale(0.92); opacity: 0; }
    to   { transform: translateY(0) scale(1); opacity: 1; }
  }

  @keyframes pillOut {
    from { transform: translateY(0) scale(1); opacity: 1; }
    to   { transform: translateY(6px) scale(0.88); opacity: 0; }
  }

  .pill.recording.dying,
  .pill.handsfree.dying {
    animation: pillOut 0.18s cubic-bezier(0.4, 0, 1, 1) both;
    pointer-events: none;
  }

  /* Skip entry animation for seamless continuations from recording */
  .pill.no-anim { animation: none; }

  /* Recording: snug wrap — 12 bars × 3px + 11 gaps × 2px + 14px padding = 72px */
  /* 0.25s delay keeps it invisible during a fast double-click handsfree activation */
  .pill.recording {
    gap: 2px;
    padding: 0 7px;
    width: 72px;
    animation: pillIn 0.22s cubic-bezier(0.34, 1.56, 0.64, 1) 0.25s both;
  }

  .bar {
    width: 3px;
    background: var(--pill-bar);
    border-radius: 999px;
    flex-shrink: 0;
    /* Instant response — no CSS transition so bars snap cleanly */
  }

  /* Processing */
  .pill.processing { width: 100px; padding: 0 14px; }

  /* Recording→processing: grow in width */
  .pill.processing.from-rec {
    animation: processIn 0.32s cubic-bezier(0.34, 1.56, 0.64, 1) both;
  }
  @keyframes processIn {
    from { width: 72px; }
    to   { width: 100px; }
  }
  /* Scan line fades in after the pill has grown */
  .pill.processing.from-rec .scan-line {
    animation: scanIn 0.18s ease 0.18s both;
  }

  /* Handsfree→processing: pill shrinks slightly */
  .pill.processing.from-hf {
    animation: processFromHf 0.25s ease-out both;
  }
  @keyframes processFromHf {
    from { width: 112px; }
    to   { width: 100px; }
  }
  .pill.processing.from-hf .scan-line {
    animation: scanIn 0.18s ease 0.08s both;
  }

  /* Scan line: dim track with a bright light sweeping back and forth */
  .scan-line {
    flex: 1;
    height: 2px;
    border-radius: 999px;
    background: rgba(255,255,255,0.12);
    position: relative;
    overflow: hidden;
  }
  .scan-line::after {
    content: '';
    position: absolute;
    top: 0; left: -40%;
    width: 80%; height: 100%;
    background: linear-gradient(90deg, transparent 0%, rgba(255,255,255,0.9) 50%, transparent 100%);
    border-radius: 999px;
    animation: scan 1.1s ease-in-out infinite alternate;
  }
  @keyframes scan {
    from { left: -40%; }
    to   { left: 60%; }
  }
  @keyframes scanIn {
    from { opacity: 0; }
    to   { opacity: 1; }
  }

  /* Error */
  .pill.error { width: 110px; gap: 7px; padding: 0 14px; background: var(--danger-bg); }
  .err-icon {
    width: 15px; height: 15px; flex-shrink: 0;
    border-radius: 50%;
    background: var(--danger);
    display: grid; place-items: center;
    font-size: 10px; font-weight: 700; color: var(--on-accent);
  }
  .err-text { font-size: 11px; font-weight: 500; color: var(--danger); white-space: nowrap; }

  /* Handsfree: starts compact (mirrors recording), expands to 112px after 450ms */
  .pill.handsfree {
    width: 72px;
    padding: 0 7px;
    gap: 2px;
    transition: width 0.2s cubic-bezier(0.34, 1.56, 0.64, 1),
                padding 0.18s ease,
                gap 0.18s ease;
  }
  .pill.handsfree.hf-expanded { width: 112px; padding: 0 5px; gap: 4px; }

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
  .hf-btn.cancel  { color: var(--pill-muted); }
  .hf-btn.confirm { color: var(--accent); }
  .hf-btn.cancel:hover  { color: var(--pill-muted-strong); }
  .hf-btn.confirm:hover { color: var(--accent-ink); }

  @keyframes hfBtnIn {
    from { opacity: 0; transform: scale(0.7); }
    to   { opacity: 1; transform: scale(1); }
  }
  .hf-btn.cancel  { animation: hfBtnIn 0.12s cubic-bezier(0.34, 1.56, 0.64, 1) both; }
  .hf-btn.confirm { animation: hfBtnIn 0.12s cubic-bezier(0.34, 1.56, 0.64, 1) 0.02s both; }
</style>
