<script lang="ts">
  import { onMount, tick } from 'svelte';

  type PillState = 'idle' | 'recording' | 'processing' | 'handsfree' | 'error';
  let state: PillState = 'idle';
  let errorMsg = '';
  let errOpen = false;
  let errWidth = 0;
  let errTextEl: HTMLSpanElement | null = null;
  let errorTimer: ReturnType<typeof setTimeout> | null = null;
  let showHfButtons = false;
  let hfTimer: ReturnType<typeof setTimeout> | null = null;
  let prevState: PillState = 'idle';
  let dying = false;
  let dyingTimer: ReturnType<typeof setTimeout> | null = null;

  const BARS = 12;

  // Snap CSS-px lengths to a whole number of device pixels. On fractional DPI
  // scaling (e.g. 1.25×/1.5×) a hardcoded 3px bar maps to a fractional device
  // width, and Chromium rounds each bar's edges independently — so neighboring
  // bars rasterize at different physical widths and the row looks uneven. When
  // every bar width/gap is an exact integer device-px, all edges land on the
  // grid and every bar renders identically.
  //
  // `dpr` MUST track the monitor the pill is actually on. The pill window is
  // created once and reused, so it first mounts on the user's primary display
  // and is later moved to other monitors (which may have a different scale).
  // A stale dpr is worse than none: snapping 3px with the *wrong* dpr produces
  // a fractional CSS width (e.g. 3.2px) that then rounds unevenly at the real
  // scale. WebView2 does not reliably fire matchMedia on a cross-monitor DPI
  // change, so we also re-read devicePixelRatio live each animation frame
  // (see refreshDpr / animateBars).
  //
  // `dpr` is passed explicitly into snap() so the reactive statements below name
  // it as a dependency. Svelte's `$:` tracking is syntactic — if `snap` closed
  // over `dpr` instead, `barW`/`barGap` would be computed once and never update
  // when the pill moves to a monitor with a different scale.
  let dpr = typeof window !== 'undefined' ? (window.devicePixelRatio || 1) : 1;
  const snap = (px: number, d: number) => Math.round(px * d) / d;
  $: barW = snap(3, dpr);
  $: barGap = snap(2, dpr);

  // matchMedia watcher for cross-monitor DPI changes. Kept at component scope
  // (not inside onMount) so refreshDpr can re-arm it: a (resolution: Xdppx)
  // query only fires when *that* resolution's match state flips, so once dpr
  // moves the watcher must be rebound to the new value or it goes stale and
  // stops catching subsequent changes.
  let mq: MediaQueryList | null = null;
  const onDprChange = () => { dpr = window.devicePixelRatio || 1; armDprWatch(); };
  function armDprWatch() {
    if (typeof window === 'undefined') return;
    mq?.removeEventListener('change', onDprChange);
    mq = window.matchMedia(`(resolution: ${window.devicePixelRatio}dppx)`);
    mq.addEventListener('change', onDprChange);
  }

  // Re-read the live devicePixelRatio; assigning only on change keeps Svelte
  // from re-running barW/barGap (and the bar template) every frame, and re-arms
  // the matchMedia watcher so it stays in sync with the current DPI.
  function refreshDpr() {
    const live = window.devicePixelRatio || 1;
    if (live !== dpr) {
      dpr = live;
      armDprWatch();
    }
  }

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

    // Keep bar snapping aligned to whichever monitor the pill is currently on.
    refreshDpr();

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
      errOpen = false;
      errWidth = 0;
      errorMsg = '';
      if (errorTimer) {
        clearTimeout(errorTimer);
        errorTimer = null;
      }
    }, 200);
  }

  // Error pill dimensions in px — mirror .pill.error's CSS (width / gap / max-width).
  const ERROR_COLLAPSED_WIDTH = 42;
  const ERROR_GAP = 8;
  const ERROR_MAX_WIDTH = 356;

  // Opens the error pill: render collapsed (icon-only), measure the message
  // text, then grow to its natural width so the CSS width transition has a
  // starting value to animate from. pill-error and pill-state arrive as
  // separate events in unspecified order, so openError() can be invoked
  // again before or after a prior call finishes. The task id lets a stale
  // call bail out instead of clobbering a newer one's measurement, and
  // wasOpen skips the collapse phase on a re-trigger so an already-open pill
  // resizes in place instead of visibly collapsing and re-expanding.
  let openErrorTaskId = 0;
  async function openError() {
    const taskId = ++openErrorTaskId;
    const wasOpen = errOpen;
    if (!wasOpen) {
      errOpen = false;
      errWidth = ERROR_COLLAPSED_WIDTH;
    }
    await tick();
    if (taskId !== openErrorTaskId || state !== 'error') return;

    const applyWidth = () => {
      const textW = errTextEl?.scrollWidth ?? 0;
      const errWidthNatural = textW > 0
        ? Math.min(ERROR_COLLAPSED_WIDTH + ERROR_GAP + textW, ERROR_MAX_WIDTH)
        : ERROR_COLLAPSED_WIDTH;
      errWidth = errWidthNatural;
      errOpen = true;
    };

    if (wasOpen) {
      applyWidth();
    } else {
      requestAnimationFrame(() => {
        if (taskId !== openErrorTaskId || state !== 'error') return;
        applyWidth();
      });
    }
  }

  onMount(() => {
    const unlisteners: Array<() => void> = [];
    let mounted = true;

    // Arm the cross-monitor DPI watcher (defined at component scope above so
    // refreshDpr can re-arm it as the pill moves between displays).
    armDprWatch();

    (async () => {
      const { listen } = await import('@tauri-apps/api/event');

      const l1 = await listen<string>('pill-state', (ev) => {
        const incoming = (ev.payload as PillState) || 'idle';
        if (hfTimer !== null) { clearTimeout(hfTimer); hfTimer = null; }

        if (incoming === 'idle' && (state === 'recording' || state === 'handsfree' || state === 'error')) {
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
          refreshDpr(); // align snapping to the current monitor before first paint
          startRaf();
        } else {
          stopRaf();
        }
        if (state !== 'recording' && state !== 'handsfree') smoothed = 0;
        if (state === 'error') {
          openError();
          if (errorTimer) clearTimeout(errorTimer);
          errorTimer = setTimeout(() => {
            errorTimer = null;
            if (state === 'error') goIdle();
          }, 2000);
        } else {
          if (errorTimer) {
            clearTimeout(errorTimer);
            errorTimer = null;
          }
          errorMsg = '';
          errOpen = false;
          errWidth = 0;
        }
      });
      if (!mounted) { l1(); return; }
      unlisteners.push(l1);

      const l2 = await listen<string>('pill-error', (ev) => {
        errorMsg = ev.payload ?? 'Something went wrong';
        if (state === 'error') {
          openError();
        }
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
      mq?.removeEventListener('change', onDprChange);
      cancelAnimationFrame(rafId);
      if (errorTimer) clearTimeout(errorTimer);
      if (dyingTimer) clearTimeout(dyingTimer);
      if (hfTimer) clearTimeout(hfTimer);
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
    <div class="pill recording" class:dying={dying} style="--bar-w:{barW}px; --bar-gap:{barGap}px">
      {#each barHeights as h, i (i)}
        <div class="bar" style="height: {snap(h, dpr)}px"></div>
      {/each}
    </div>

  {:else if state === 'processing'}
    <div class="pill processing" class:from-rec={prevState === 'recording'} class:from-hf={prevState === 'handsfree'}>
      <div class="scan-line"></div>
    </div>

  {:else if state === 'error'}
    <div class="pill error" class:err-open={errOpen} class:dying={dying}
         style={errWidth ? `width:${errWidth}px` : ''}>
      <svg class="err-icon" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <circle cx="12" cy="12" r="10"/><path d="M12 8v4M12 16h.01"/>
      </svg>
      <span class="err-text" bind:this={errTextEl}>{errorMsg || 'Something went wrong'}</span>
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
      <div class="bars-hf" style="--bar-w:{barW}px; --bar-gap:{barGap}px">
        {#each barHeights as h, i (i)}
          <div class="bar" style="height: {snap(h, dpr)}px"></div>
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
    width: 100vw; height: 44px;
    font-family: var(--sans);
  }

  .wrap {
    width: 100vw; height: 44px;
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
  .pill.handsfree.dying,
  .pill.error.dying {
    animation: pillOut 0.18s cubic-bezier(0.4, 0, 1, 1) both;
    pointer-events: none;
  }

  /* Skip entry animation for seamless continuations from recording */
  .pill.no-anim { animation: none; }

  /* Recording: snug wrap — 12 bars × 3px + 11 gaps × 2px + 14px padding = 72px */
  /* 0.25s delay keeps it invisible during a fast double-click handsfree activation */
  .pill.recording {
    gap: var(--bar-gap, 2px);
    padding: 0 7px;
    width: 72px;
    animation: pillIn 0.22s cubic-bezier(0.34, 1.56, 0.64, 1) 0.25s both;
  }

  .bar {
    width: var(--bar-w, 3px);
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

  /* Error: rounded rectangle, dark red-tinted, expands horizontally to reveal the message */
  .pill.error {
    width: 42px;
    gap: 8px;
    padding: 0 14px;
    background: var(--pill-error-bg);
    color: var(--pill-error-fg);
    border-radius: 14px;
    box-shadow: 0 0 0 1px var(--pill-error-border),
                0 6px 18px rgba(0,0,0,0.28);
    max-width: 356px;
    overflow: hidden;
    transition: width 0.30s cubic-bezier(0.34, 1.56, 0.64, 1);
  }
  .err-icon {
    flex-shrink: 0;
    color: var(--pill-error-fg);
  }
  .err-text {
    font-size: 11.5px; font-weight: 500;
    color: var(--pill-error-fg);
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
    opacity: 0;
    transition: opacity 0.18s ease 0.08s;
  }
  .pill.error.err-open .err-text { opacity: 1; }

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

  .bars-hf { display: flex; align-items: center; gap: var(--bar-gap, 2px); flex: 1; justify-content: center; }

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
