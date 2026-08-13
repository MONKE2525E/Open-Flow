<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { getProfileLabel } from './lib/appMappings';

  type PillState = 'idle' | 'recording' | 'processing' | 'loading_local_model' | 'handsfree' | 'error' | 'cancelled' | 'paste_failed' | 'copied';
  let state: PillState = 'idle';
  let errorMsg = '';
  let errOpen = false;
  let errWidth = 0;
  let errTextEl: HTMLSpanElement | null = null;
  let errorTimer: ReturnType<typeof setTimeout> | null = null;
  let showHfButtons = false;
  let hfTimer: ReturnType<typeof setTimeout> | null = null;
  let cancelOpen = false;
  let showCancelBtn = false;
  let cancelBtnTimer: ReturnType<typeof setTimeout> | null = null;
  let cancelDismissTimer: ReturnType<typeof setTimeout> | null = null;
  let showCopyBtn = false;
  let copyBtnTimer: ReturnType<typeof setTimeout> | null = null;
  let pasteFailedDismissTimer: ReturnType<typeof setTimeout> | null = null;
  let copiedPillTimer: ReturnType<typeof setTimeout> | null = null;
  let copied = false;
  let copiedTimer: ReturnType<typeof setTimeout> | null = null;
  let prevState: PillState = 'idle';
  let dying = false;
  let dyingTimer: ReturnType<typeof setTimeout> | null = null;

  // Resolved tone profile for the current dictation (label form, e.g. "Casual"),
  // emitted by the backend from the pipeline's own resolution.
  let profileLabel: string | null = null;

  // Processing sub-stage ("Transcribing…" / "Cleaning…" / "Pasting…"), driven
  // by real `pill-stage` events from the pipeline. All three rows stay mounted
  // in a fixed stack (see .stage-roll) and the active one is selected by index,
  // so a stage change is a pure transform roll — the text never remounts, which
  // is what makes the transition flicker-free. A stage only replaces the
  // previous one once it has been visible a minimum time, so a sub-400ms stage
  // (e.g. cleanup resolving instantly when disabled) can't flash; the newest
  // pending stage always wins and terminal states clear everything.
  const STAGE_MIN_MS = 400;
  const STAGE_ROWS = ['Transcribing…', 'Cleaning…', 'Pasting…'] as const;
  const STAGE_INDEX: Record<string, number> = {
    transcribing: 0,
    cleaning: 1,
    pasting: 2,
  };
  const STAGE_ROW_H = 14;
  let stageIndex = -1;
  let pendingStageIndex: number | null = null;
  let stageTimer: ReturnType<typeof setTimeout> | null = null;
  let stageShownAt = 0;

  function onPillStage(stageName: string) {
    const idx = STAGE_INDEX[stageName];
    if (idx === undefined || state === 'idle' || stageIndex === idx) return;
    if (stageIndex === -1 || performance.now() - stageShownAt >= STAGE_MIN_MS) {
      stageIndex = idx;
      stageShownAt = performance.now();
      pendingStageIndex = null;
      return;
    }
    pendingStageIndex = idx;
    if (stageTimer) return;
    const remaining = STAGE_MIN_MS - (performance.now() - stageShownAt);
    stageTimer = setTimeout(() => {
      stageTimer = null;
      if (pendingStageIndex !== null) {
        stageIndex = pendingStageIndex;
        pendingStageIndex = null;
        stageShownAt = performance.now();
      }
    }, remaining);
  }

  function clearStage() {
    if (stageTimer) {
      clearTimeout(stageTimer);
      stageTimer = null;
    }
    pendingStageIndex = null;
    stageIndex = -1;
  }

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
    // Use the already-fallback-guarded `dpr` (set right before every call) so a
    // falsy devicePixelRatio can't produce `(resolution: undefineddppx)`.
    mq = window.matchMedia(`(resolution: ${dpr}dppx)`);
    mq.addEventListener('change', onDprChange);
  }

  // Re-read the live devicePixelRatio; assigning only on change keeps Svelte
  // from re-running barW/barGap (and the bar template) every frame, and re-arms
  // the matchMedia watcher so it stays in sync with the current DPI.
  function refreshDpr() {
    if (typeof window === 'undefined') return;
    const live = window.devicePixelRatio || 1;
    if (live !== dpr) {
      dpr = live;
      armDprWatch();
    }
  }

  // Level from Rust is already 0–1 (raw_rms × mic_gain × 15, capped).
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
      clearStage();
      profileLabel = null;
      smoothed = 0;
      errOpen = false;
      errWidth = 0;
      errorMsg = '';
      if (errorTimer) {
        clearTimeout(errorTimer);
        errorTimer = null;
      }
      cancelOpen = false;
      showCancelBtn = false;
      if (cancelBtnTimer) {
        clearTimeout(cancelBtnTimer);
        cancelBtnTimer = null;
      }
      if (cancelDismissTimer) {
        clearTimeout(cancelDismissTimer);
        cancelDismissTimer = null;
      }
      showCopyBtn = false;
      copied = false;
      if (copyBtnTimer) {
        clearTimeout(copyBtnTimer);
        copyBtnTimer = null;
      }
      if (pasteFailedDismissTimer) {
        clearTimeout(pasteFailedDismissTimer);
        pasteFailedDismissTimer = null;
      }
      if (copiedTimer) {
        clearTimeout(copiedTimer);
        copiedTimer = null;
      }
      if (copiedPillTimer) {
        clearTimeout(copiedPillTimer);
        copiedPillTimer = null;
      }
    }, 200);
  }

  // Error pill dimensions in px — mirror .pill.error's CSS (width / gap / max-width).
  const ERROR_COLLAPSED_WIDTH = 42;
  const ERROR_GAP = 8;
  const ERROR_MAX_WIDTH = 356;
  // Retry button (18px) + the flex gap in front of it — reserved
  // unconditionally since the button always shows once the pill is open.
  const ERROR_RETRY_BTN_WIDTH = 26;

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
        ? Math.min(ERROR_COLLAPSED_WIDTH + ERROR_GAP + textW + ERROR_RETRY_BTN_WIDTH, ERROR_MAX_WIDTH)
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

        // A fresh recording starts a brand-new dictation: drop the previous
        // one's profile/stage. Terminal states are no longer "in progress", so
        // they clear too (idle is handled by goIdle).
        if (incoming === 'recording') {
          profileLabel = null;
          clearStage();
        } else if (
          incoming === 'error' ||
          incoming === 'cancelled' ||
          incoming === 'paste_failed' ||
          incoming === 'copied'
        ) {
          clearStage();
          profileLabel = null;
        }

        if (incoming === 'idle' && state !== 'idle') {
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
          }, 10000);
        } else if (state !== 'copied') {
          // Don't clear errorMsg on 'copied' — show_copied_pill carries its
          // confirmation text through the pill-error event, which fires just
          // before this pill-state one.
          if (errorTimer) {
            clearTimeout(errorTimer);
            errorTimer = null;
          }
          errorMsg = '';
          errOpen = false;
          errWidth = 0;
        }

        if (state === 'cancelled') {
          cancelOpen = false;
          showCancelBtn = false;
          requestAnimationFrame(() => {
            if (state === 'cancelled') cancelOpen = true;
          });
          if (cancelBtnTimer) clearTimeout(cancelBtnTimer);
          cancelBtnTimer = setTimeout(() => {
            cancelBtnTimer = null;
            if (state === 'cancelled') showCancelBtn = true;
          }, 200);
          if (cancelDismissTimer) clearTimeout(cancelDismissTimer);
          cancelDismissTimer = setTimeout(() => {
            cancelDismissTimer = null;
            // Just hide the toast — the capture itself stays resumable from
            // Home for the full backend window. Only the explicit dismiss
            // button actually discards it (see dismissCancelled()).
            if (state === 'cancelled') goIdle();
          }, 10000);
        } else {
          if (cancelBtnTimer) {
            clearTimeout(cancelBtnTimer);
            cancelBtnTimer = null;
          }
          if (cancelDismissTimer) {
            clearTimeout(cancelDismissTimer);
            cancelDismissTimer = null;
          }
          cancelOpen = false;
          showCancelBtn = false;
        }

        if (state === 'paste_failed') {
          showCopyBtn = false;
          copied = false;
          // A stale copiedTimer from a previous copy click could fire goIdle()
          // on the fresh paste_failed state — clear it alongside the others.
          if (copiedTimer) {
            clearTimeout(copiedTimer);
            copiedTimer = null;
          }
          if (copyBtnTimer) clearTimeout(copyBtnTimer);
          copyBtnTimer = setTimeout(() => {
            copyBtnTimer = null;
            if (state === 'paste_failed') showCopyBtn = true;
          }, 1200);
          if (pasteFailedDismissTimer) clearTimeout(pasteFailedDismissTimer);
          pasteFailedDismissTimer = setTimeout(() => {
            pasteFailedDismissTimer = null;
            if (state === 'paste_failed') goIdle();
          }, 10000);
        } else {
          if (copyBtnTimer) {
            clearTimeout(copyBtnTimer);
            copyBtnTimer = null;
          }
          if (pasteFailedDismissTimer) {
            clearTimeout(pasteFailedDismissTimer);
            pasteFailedDismissTimer = null;
          }
          if (copiedTimer) {
            clearTimeout(copiedTimer);
            copiedTimer = null;
          }
          showCopyBtn = false;
        }

        if (state === 'copied') {
          if (copiedPillTimer) clearTimeout(copiedPillTimer);
          copiedPillTimer = setTimeout(() => {
            copiedPillTimer = null;
            if (state === 'copied') goIdle();
          }, 5000);
        } else if (copiedPillTimer) {
          clearTimeout(copiedPillTimer);
          copiedPillTimer = null;
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

      const l5 = await listen<string>('pill-profile', (ev) => {
        const profile = ev.payload;
        profileLabel = profile && profile !== 'unknown' ? getProfileLabel(profile) : null;
      });
      if (!mounted) { l5(); return; }
      unlisteners.push(l5);

      const l6 = await listen<string>('pill-stage', (ev) => {
        onPillStage(ev.payload);
      });
      if (!mounted) { l6(); return; }
      unlisteners.push(l6);

      // Fired when the cancelled capture is resumed or dismissed from
      // *another* window (Home's banner) — if this toast is still showing,
      // it's now stale, so drop it without re-invoking dismiss.
      const l4 = await listen('verenu:cancelled-capture-cleared', () => {
        if (state === 'cancelled') goIdle();
      });
      if (!mounted) { l4(); return; }
      unlisteners.push(l4);
    })();

    return () => {
      mounted = false;
      mq?.removeEventListener('change', onDprChange);
      cancelAnimationFrame(rafId);
      if (errorTimer) clearTimeout(errorTimer);
      if (dyingTimer) clearTimeout(dyingTimer);
      if (hfTimer) clearTimeout(hfTimer);
      if (cancelBtnTimer) clearTimeout(cancelBtnTimer);
      if (cancelDismissTimer) clearTimeout(cancelDismissTimer);
      if (copyBtnTimer) clearTimeout(copyBtnTimer);
      if (pasteFailedDismissTimer) clearTimeout(pasteFailedDismissTimer);
      if (copiedTimer) clearTimeout(copiedTimer);
      if (copiedPillTimer) clearTimeout(copiedPillTimer);
      clearStage();
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

  async function retryFailed() {
    if (errorTimer) { clearTimeout(errorTimer); errorTimer = null; }
    const { invoke } = await import('@tauri-apps/api/core');
    // Don't go idle before the call — Rust emits 'processing' on success so
    // the pill morphs from error to processing. If the retry fails, only
    // fall back to idle while still in the error state — the pill may have
    // moved on to an active state (recording/handsfree) during the invoke.
    await invoke('retry_transcription').catch(() => {
      if (state === 'error') goIdle();
    });
  }

  async function continueCancelled() {
    if (cancelDismissTimer) { clearTimeout(cancelDismissTimer); cancelDismissTimer = null; }
    const { invoke } = await import('@tauri-apps/api/core');
    // Don't force idle on success — Rust emits 'handsfree' next so the pill
    // morphs directly from cancelled to handsfree, mirroring confirmHandless.
    await invoke('resume_cancelled_capture').catch(() => { goIdle(); });
  }

  async function dismissCancelled() {
    if (cancelDismissTimer) { clearTimeout(cancelDismissTimer); cancelDismissTimer = null; }
    goIdle();
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('dismiss_cancelled_capture').catch(() => {});
  }

  async function copyPasteFailure() {
    const { invoke } = await import('@tauri-apps/api/core');
    try {
      await invoke('copy_paste_failure_to_clipboard');
      copied = true;
      if (pasteFailedDismissTimer) { clearTimeout(pasteFailedDismissTimer); pasteFailedDismissTimer = null; }
      if (copiedTimer) clearTimeout(copiedTimer);
      copiedTimer = setTimeout(() => {
        copiedTimer = null;
        if (state === 'paste_failed') goIdle();
      }, 1500);
    } catch {
      // Nothing left to copy (expired/already copied) — just let the toast
      // run out its normal auto-dismiss timer.
    }
  }
</script>

<!-- --bar-w/--bar-gap live on .wrap so every pill state (incl. processing, which
     has no bars of its own) inherits the DPI-snapped values for its width calc. -->
<div class="wrap" style="--bar-w:{barW}px; --bar-gap:{barGap}px">
  {#if profileLabel && (state === 'recording' || state === 'processing' || state === 'handsfree' || state === 'loading_local_model')}
    <span class="pill-profile">{profileLabel}</span>
  {/if}

  {#if state === 'recording'}
    <div class="pill recording" class:dying={dying}>
      {#each barHeights as h, i (i)}
        <div class="bar" style="height: {snap(h, dpr)}px"></div>
      {/each}
    </div>

  {:else if state === 'processing'}
    <div class="pill processing"
         class:from-rec={prevState === 'recording'}
         class:from-hf={prevState === 'handsfree'}
         class:from-loading={prevState === 'loading_local_model'}
         class:dying={dying}>
      <div class="stage-counter" aria-hidden="true">
        <div class="stage-roll" style="transform: translateY({stageIndex * -STAGE_ROW_H}px)">
          {#each STAGE_ROWS as label, i (label)}
            <span class="stage-row">{label}</span>
          {/each}
        </div>
      </div>
      <div class="spinner"></div>
    </div>

  {:else if state === 'loading_local_model'}
    <div class="pill loading-local" class:from-processing={prevState === 'processing'} class:dying={dying}>
      <div class="loading-spinner"></div>
      <span>Loading model</span>
    </div>

  {:else if state === 'error'}
    <div class="pill error" class:err-open={errOpen} class:dying={dying}
         style={errWidth ? `width:${errWidth}px` : ''}>
      <svg class="err-icon" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <circle cx="12" cy="12" r="10"/><path d="M12 8v4M12 16h.01"/>
      </svg>
      <span class="err-text" bind:this={errTextEl}>{errorMsg || 'Something went wrong'}</span>
      {#if errOpen}
        <button class="hf-btn err-retry" onclick={retryFailed} aria-label="Retry">
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M21 12a9 9 0 1 1-2.64-6.36"/><path d="M21 4v5h-5"/>
          </svg>
        </button>
      {/if}
    </div>

  {:else if state === 'cancelled'}
    <div class="pill cancelled" class:cancel-open={cancelOpen} class:dying={dying}>
      <button class="hf-btn cancel" onclick={dismissCancelled} aria-label="Dismiss">
        <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round">
          <path d="M6 6l12 12M6 18 18 6"/>
        </svg>
      </button>
      <span class="cancel-text">Cancelled</span>
      {#if showCancelBtn}
        <button class="hf-btn confirm" onclick={continueCancelled} aria-label="Undo — keep recording">
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M9 14 4 9l5-5"/><path d="M4 9h10.5a5.5 5.5 0 0 1 5.5 5.5v0a5.5 5.5 0 0 1-5.5 5.5H11"/>
          </svg>
        </button>
      {/if}
    </div>

  {:else if state === 'paste_failed'}
    <div class="pill paste-failed" class:copy-open={showCopyBtn} class:dying={dying}>
      <svg class="err-icon" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <circle cx="12" cy="12" r="10"/><path d="M12 8v4M12 16h.01"/>
      </svg>
      <span class="paste-failed-text">Not pasted</span>
      {#if showCopyBtn}
        <button class="hf-btn copy-btn" onclick={copyPasteFailure} aria-label="Copy to clipboard">
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
            {#if copied}
              <polyline points="20 6 9 17 4 12"/>
            {:else}
              <rect x="9" y="9" width="13" height="13" rx="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/>
            {/if}
          </svg>
        </button>
      {/if}
    </div>

  {:else if state === 'copied'}
    <div class="pill copied" class:dying={dying}>
      <svg class="copied-icon" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
        <polyline points="20 6 9 17 4 12"/>
      </svg>
      <span class="copied-text">{errorMsg || 'Copied last dictation to clipboard'}</span>
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
    width: 100vw; height: 100vh;
    font-family: var(--sans);
  }

  .wrap {
    width: 100vw; height: 100vh;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  /* Resolved tone profile, shown as a small floating tag beside the pill so
     the otherwise-invisible style setting stays legible without crowding or
     offsetting the pill capsule itself. Fades in softly so its appearance
     reads as the pill growing, not a new element popping in. */
  .pill-profile {
    font-size: 10.5px;
    font-weight: 500;
    letter-spacing: 0.02em;
    color: var(--pill-muted);
    background: var(--pill-bg);
    border-radius: 999px;
    padding: 2px 8px;
    white-space: nowrap;
    text-shadow: 0 1px 2px rgba(0,0,0,0.35);
    box-shadow: 0 2px 6px rgba(0,0,0,0.18);
    animation: chipIn 0.18s ease-out both;
  }
  @keyframes chipIn {
    from { opacity: 0; transform: translateY(-3px); }
    to   { opacity: 1; transform: translateY(0); }
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
  .pill.error.dying,
  .pill.cancelled.dying,
  .pill.paste-failed.dying,
  .pill.copied.dying,
  .pill.processing.dying,
  .pill.loading-local.dying {
    animation: pillOut 0.18s cubic-bezier(0.4, 0, 1, 1) both;
    pointer-events: none;
  }

  /* Skip entry animation for seamless continuations from recording */
  .pill.no-anim { animation: none; }

  /* Recording: snug wrap — 12 bars + 11 gaps + 14px padding. Width is derived
     from the snapped --bar-w/--bar-gap so the wrap stays snug at fractional DPI
     (where snapped bars sum to more than the integer-scale 72px). */
  /* 0.25s delay keeps it invisible during a fast double-click handsfree activation */
  .pill.recording {
    gap: var(--bar-gap, 2px);
    padding: 0 7px;
    width: calc(12 * var(--bar-w, 3px) + 11 * var(--bar-gap, 2px) + 14px);
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
  .pill.processing { width: 140px; padding: 0 12px; gap: 7px; }
  .pill.loading-local { width: 144px; padding: 0 14px; gap: 9px; }

  /* Recording→processing: grow in width */
  .pill.processing.from-rec {
    animation: processIn 0.32s cubic-bezier(0.34, 1.56, 0.64, 1) both;
  }
  @keyframes processIn {
    /* start from the recording pill's DPI-snapped width so there's no jump */
    from { width: calc(12 * var(--bar-w, 3px) + 11 * var(--bar-gap, 2px) + 14px); }
    to   { width: 140px; }
  }

  /* Handsfree→processing: pill shrinks slightly */
  .pill.processing.from-hf {
    animation: processFromHf 0.25s ease-out both;
  }
  @keyframes processFromHf {
    /* start from the expanded handsfree pill's DPI-snapped width (bars + 54px of
       buttons/padding/gaps) so there's no jump at fractional DPI */
    from { width: calc(12 * var(--bar-w, 3px) + 11 * var(--bar-gap, 2px) + 54px); }
    to   { width: 140px; }
  }

  /* Processing→loading model: grow smoothly into the wider pill instead of
     popping in fresh, so a cold local model load reads as one continuous
     motion rather than two disconnected "new pill appeared" pops. */
  .pill.loading-local.from-processing {
    animation: loadingLocalIn 0.3s cubic-bezier(0.34, 1.56, 0.64, 1) both;
  }
  @keyframes loadingLocalIn {
    from { width: 140px; }
    to   { width: 144px; }
  }
  /* Comma-separated, not a separate rule: the entrance fade and the
     spinner's/label's own continuous animation (set below) both apply to
     the same element's `animation` property, and a second declaration would
     replace rather than layer on top of the first. */
  .pill.loading-local.from-processing .loading-spinner {
    animation: scanIn 0.16s ease 0.16s both, spin 0.8s linear infinite;
  }
  .pill.loading-local.from-processing span {
    animation: scanIn 0.16s ease 0.16s both, loadingPulse 1.6s ease-in-out 0.4s infinite alternate;
  }

  /* Loading model→processing: shrink back down once the model is warm and
     transcription/cleanup resumes, mirroring the handsfree→processing shrink. */
  .pill.processing.from-loading {
    animation: processFromLoading 0.26s ease-out both;
  }
  @keyframes processFromLoading {
    from { width: 144px; }
    to   { width: 140px; }
  }

  /* If the pipeline exits idle mid-transition (e.g. cancelled right as a
     local model starts loading), `dying` and a `from-*` entrance class can
     both be true on the same pill at once. Equal specificity would leave it
     to source order, which is fragile — this 4-class override guarantees
     the exit fade always wins over an in-progress entrance animation. */
  .pill.processing.from-rec.dying,
  .pill.processing.from-hf.dying,
  .pill.processing.from-loading.dying,
  .pill.loading-local.from-processing.dying {
    animation: pillOut 0.18s cubic-bezier(0.4, 0, 1, 1) both;
  }

  /* Stage counter: all three stage labels stay mounted in one vertical stack
     and the active one is picked by a translateY roll — no remounting, no
     opacity flashes, just a mechanical-counter roll between stages. The clip
     window is exactly one row tall; the stack's width is the widest row, so
     the pill's width never changes between stages. */
  .stage-counter {
    height: 14px;
    overflow: hidden;
    display: flex;
  }
  .stage-roll {
    display: flex;
    flex-direction: column;
    will-change: transform;
    transition: transform 0.22s cubic-bezier(0.22, 1, 0.36, 1);
  }
  /* A soft white light sweeps across the letters while the stage is active —
     the scan lives on the text itself (background-clip), replacing the old
     separate moving line so nothing flickers past the label. */
  .stage-row {
    height: 14px;
    line-height: 14px;
    font-size: 11px;
    font-weight: 500;
    white-space: nowrap;
    color: transparent;
    background-image: linear-gradient(
      90deg,
      var(--pill-muted-strong) 0%,
      var(--pill-muted-strong) 44%,
      rgba(255, 255, 255, 0.95) 50%,
      var(--pill-muted-strong) 56%,
      var(--pill-muted-strong) 100%
    );
    background-size: 200% 100%;
    background-clip: text;
    -webkit-background-clip: text;
    animation: stage-scan 2.2s ease-in-out infinite alternate;
  }
  @keyframes stage-scan {
    from { background-position: -110% 0; }
    to   { background-position: 10% 0; }
  }
  @keyframes scanIn {
    from { opacity: 0; }
    to   { opacity: 1; }
  }

  .loading-spinner {
    width: 12px;
    height: 12px;
    border-radius: 50%;
    border: 2px solid rgba(255,255,255,0.16);
    border-top-color: var(--accent);
    animation: spin 0.8s linear infinite;
    flex-shrink: 0;
  }

  .pill.loading-local span {
    font-size: 11.5px;
    font-weight: 600;
    letter-spacing: 0.1px;
    white-space: nowrap;
    /* Gentle breathing pulse reads as "still working" without being
       distracting — starts after the entrance settles (see from-processing
       above), not from the moment the pill first appears. */
    animation: loadingPulse 1.6s ease-in-out 0.4s infinite alternate;
  }

  @keyframes spin { to { transform: rotate(360deg); } }
  @keyframes loadingPulse {
    from { opacity: 0.6; }
    to   { opacity: 1; }
  }

  /* Error: dark red-tinted stadium pill, expands horizontally to reveal the message */
  .pill.error {
    width: 42px;
    gap: 8px;
    padding: 0 14px;
    background: var(--pill-error-bg);
    color: var(--pill-error-fg);
    border-radius: 999px;
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

  .hf-btn.err-retry {
    color: var(--pill-error-fg);
    animation: hfBtnIn 0.12s cubic-bezier(0.34, 1.56, 0.64, 1) 0.1s both;
  }
  .hf-btn.err-retry:hover { background: rgba(255,255,255,0.14); }

  /* Cancelled: neutral stadium pill (not the red error palette) — starts as
     a dismiss (X) button only, expands to reveal the "Cancelled" label and
     an Undo button that resumes recording hands-free with the cancelled
     audio prepended. Both buttons are real, clickable controls (not status
     icons) — dismiss discards the capture for good, Undo keeps it going. */
  .pill.cancelled {
    width: 42px;
    gap: 8px;
    padding: 0 12px;
    border-radius: 999px;
    overflow: hidden;
    transition: width 0.3s cubic-bezier(0.34, 1.56, 0.64, 1),
                padding 0.3s cubic-bezier(0.34, 1.56, 0.64, 1);
  }
  .pill.cancelled.cancel-open {
    width: 158px;
    padding: 0 8px;
  }
  .cancel-text {
    font-size: 11.5px; font-weight: 500;
    white-space: nowrap; overflow: hidden;
    opacity: 0;
    flex: 1;
    text-align: center;
    transition: opacity 0.18s ease 0.08s;
  }
  .pill.cancelled.cancel-open .cancel-text { opacity: 1; }

  /* Paste failed: same red-tinted stadium pill as .pill.error, message-first
     (no collapsed-icon entry — the message is a fixed short string, not a
     dynamic one to measure), then widens to reveal a Copy button after a
     beat so "paste failed" registers before the fallback action appears. */
  .pill.paste-failed {
    gap: 8px;
    padding: 0 14px;
    background: var(--pill-error-bg);
    color: var(--pill-error-fg);
    border-radius: 999px;
    box-shadow: 0 0 0 1px var(--pill-error-border),
                0 6px 18px rgba(0,0,0,0.28);
    width: 128px;
    overflow: hidden;
    transition: width 0.3s cubic-bezier(0.34, 1.56, 0.64, 1),
                padding 0.3s cubic-bezier(0.34, 1.56, 0.64, 1);
  }
  .pill.paste-failed.copy-open {
    width: 158px;
    padding: 0 8px 0 14px;
  }
  .paste-failed-text {
    font-size: 11.5px; font-weight: 500;
    color: var(--pill-error-fg);
    white-space: nowrap; overflow: hidden;
    flex: 1;
    text-align: center;
  }
  .hf-btn.copy-btn {
    color: var(--pill-error-fg);
    animation: hfBtnIn 0.12s cubic-bezier(0.34, 1.56, 0.64, 1) both;
  }
  .hf-btn.copy-btn:hover { background: rgba(255,255,255,0.14); }

  /* Copied confirmation: neutral (not error-red) stadium pill for the global
     Ctrl+Alt+C / ⌥⌘C shortcut — fixed short message, auto-sized, no buttons. */
  .pill.copied {
    gap: 8px;
    padding: 0 14px;
    white-space: nowrap;
  }
  .copied-icon { color: var(--accent); flex-shrink: 0; }
  .copied-text {
    font-size: 11.5px; font-weight: 500;
    white-space: nowrap;
  }

  /* Handsfree: starts compact (mirrors recording — same DPI-snapped width so the
     recording→handsfree continuation doesn't jump), expands to 112px after 450ms */
  .pill.handsfree {
    width: calc(12 * var(--bar-w, 3px) + 11 * var(--bar-gap, 2px) + 14px);
    padding: 0 7px;
    gap: 2px;
    transition: width 0.2s cubic-bezier(0.34, 1.56, 0.64, 1),
                padding 0.18s ease,
                gap 0.18s ease;
  }
  /* Expanded width = snapped bars + 54px (two 18px buttons + 10px padding + two
     4px gaps) so the bars never overflow / push the buttons out at fractional DPI. */
  .pill.handsfree.hf-expanded { width: calc(12 * var(--bar-w, 3px) + 11 * var(--bar-gap, 2px) + 54px); padding: 0 5px; gap: 4px; }

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
    /* Gentle fade + tiny rise, deliberately no scale — a pop reads as the UI
       switching elements on and off, a settle reads as the pill growing. */
    from { opacity: 0; transform: translateY(3px); }
    to   { opacity: 1; transform: translateY(0); }
  }
  .hf-btn.cancel  { animation: hfBtnIn 0.16s ease-out both; }
  .hf-btn.confirm { animation: hfBtnIn 0.16s ease-out 0.03s both; }
</style>
