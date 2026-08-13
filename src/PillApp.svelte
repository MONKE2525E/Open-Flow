<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { getProfileLabel } from './lib/appMappings';

  type PillState = 'idle' | 'recording' | 'processing' | 'loading_local_model' | 'handsfree' | 'error' | 'cancelled' | 'paste_failed' | 'copied';
  let state: PillState = 'idle';
  let errorMsg = '';
  let errOpen = false;
  let errWidth = 0;
  let errHeight = 34;
  let errLines = 1;
  let errScroll = false;
  let errTextEl: HTMLSpanElement | null = null;
  let errSizerEl: HTMLSpanElement | null = null;
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
  // by real `pill-stage` events from the pipeline. Rows for the stages that
  // have occurred stay mounted in a stack (see .stage-roll) and the active one
  // is selected by position, so a stage change is a pure transform roll — the
  // text never remounts, which is what keeps the transition flicker-free. Only
  // stages that actually ran are mounted, so a skipped one is never scrolled
  // through (see seenStages). A stage only replaces the
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
  // 16, not 14: at 11px/weight-600 the row's own clip window (.stage-counter,
  // overflow:hidden) was cutting into descenders — the tail of "g" in
  // "Cleaning…" and the low-sitting "…" dots. A tight 14px line-height for an
  // 11px bold face doesn't leave enough room below the baseline; must stay in
  // sync with .stage-counter/.stage-row's height and line-height in the CSS
  // section below, since this drives the roll's translateY math.
  const STAGE_ROW_H = 16;
  let stageIndex = -1;
  let pendingStageIndex: number | null = null;
  let stageTimer: ReturnType<typeof setTimeout> | null = null;
  let stageShownAt = 0;

  // Which stages this dictation has actually reached, in pipeline order. Rows
  // are mounted as their stage occurs rather than all three up front: with a
  // fixed three-row stack, rolling from "Transcribing…" to "Pasting…" scrolled
  // *through* the "Cleaning…" row, so a run with cleanup disabled still flashed
  // a stage the backend correctly never emitted.
  let seenStages: number[] = [];

  function commitStage(idx: number) {
    if (!seenStages.includes(idx)) {
      seenStages = [...seenStages, idx].sort((a, b) => a - b);
    }
    stageIndex = idx;
    stageShownAt = performance.now();
  }

  function onPillStage(stageName: string) {
    const idx = STAGE_INDEX[stageName];
    if (
      idx === undefined ||
      (state !== 'processing' && state !== 'loading_local_model') ||
      stageIndex === idx
    ) return;
    if (stageIndex === -1 || performance.now() - stageShownAt >= STAGE_MIN_MS) {
      commitStage(idx);
      pendingStageIndex = null;
      return;
    }
    pendingStageIndex = idx;
    if (stageTimer) return;
    const remaining = STAGE_MIN_MS - (performance.now() - stageShownAt);
    stageTimer = setTimeout(() => {
      stageTimer = null;
      if (pendingStageIndex !== null) {
        commitStage(pendingStageIndex);
        pendingStageIndex = null;
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
    seenStages = [];
  }

  // Natural width of each stage label, measured from the hidden sizer rather
  // than hardcoded, since the widths depend on the user's font rendering and
  // not just the string. The capsule sizes itself to whichever label is showing
  // instead of standing at one width sized for the longest — at a fixed width
  // "Pasting…" sat in a wide field of empty pill. Keyed by label, not index:
  // the roll mounts rows as stages occur so positions shift, but a label's
  // width never does.
  let stageWidthByLabel: Record<string, number> = {};
  // Only used for the frame between the pill mounting and the sizer being
  // measured. Kept close to the real widest label ("Transcribing…" ≈ 70px at
  // 11px/600) so that frame picks the same quantized window as the measured
  // value — an over-large guess cost an extra native resize on every entry
  // into processing, purely to correct the guess.
  const STAGE_FALLBACK_W = 72;

  function readStageWidths(node: HTMLElement) {
    const next = { ...stageWidthByLabel };
    let changed = false;
    node.querySelectorAll<HTMLElement>('[data-stage-label]').forEach((row) => {
      const label = row.dataset.stageLabel;
      const w = Math.ceil(row.getBoundingClientRect().width);
      if (label && w > 0 && next[label] !== w) {
        next[label] = w;
        changed = true;
      }
    });
    if (changed) stageWidthByLabel = next;
  }

  function measureStageRows(node: HTMLElement) {
    readStageWidths(node);
    // Re-measure once webfonts land. The first pass runs at mount, which can be
    // while the fallback face is still active; the real face is usually wider,
    // and sizing the pill to the fallback measurement clipped the tail of the
    // longest label ("Transcribing…" lost its ellipsis).
    document.fonts?.ready
      .then(() => {
        if (node.isConnected) readStageWidths(node);
      })
      .catch(() => {});
    // Anything else that changes text metrics (a cross-monitor DPI change)
    // resizes the roll, so re-measure from that too. This cannot feed back:
    // .stage-roll is flex-shrink:0, so its width tracks its own content and
    // never the pill width we derive from it.
    const ro =
      typeof ResizeObserver !== 'undefined'
        ? new ResizeObserver(() => readStageWidths(node))
        : null;
    ro?.observe(node);
    return {
      destroy() {
        ro?.disconnect();
      },
    };
  }
  // Widest label overall — drives the steady window width during processing.
  $: stageMaxW = Object.keys(stageWidthByLabel).length
    ? Math.max(...Object.values(stageWidthByLabel))
    : STAGE_FALLBACK_W;
  // Visible capsule width follows the stage on screen. Before the first stage
  // arrives it sits at the widest, so the pill only ever narrows into it.
  $: stageW = stageWidthByLabel[STAGE_ROWS[stageIndex] ?? ''] ?? stageMaxW;

  // Rows actually mounted, and where the current stage sits among them.
  $: activeStageRows = seenStages.map((i) => STAGE_ROWS[i]);
  $: rollPos = Math.max(0, seenStages.indexOf(stageIndex));

  const BARS = 12;

  // --- Content-fit window sizing -------------------------------------------
  // The pill window's native size follows the visible content (capsule +
  // profile label floating above it + expanded error text) so the transparent
  // click-capture zone around the pill never exceeds the pill itself.
  // Measured here (CSS px == logical points on the pill's monitor) and pushed
  // to the backend, which resizes the window center-anchored horizontally and
  // bottom-anchored vertically, so the pill never moves while it grows.
  // `offsetWidth/offsetHeight` are layout dimensions, so the entrance/exit
  // scale transforms don't pollute the measurement.
  const PILL_PAD_W = 20; // shadow bleed margin (10px per side)
  const PILL_PAD_H = 20; // shadow + entrance-transform bleed (10px top + bottom)
  const MIN_PILL_WINDOW_W = 96; // smallest capsule (recording) + PAD_W, on-step
  const MIN_PILL_WINDOW_H = 54; // bare capsule + PAD_H
  // Native window widths are quantized to this step. A CSS width transition
  // fires the ResizeObserver every animation frame, and issuing a native
  // SetWindowPos per frame is what made WebView2 present stale/clipped frames
  // mid-morph — the "half the pill is missing" artifact. Rounding up to a step
  // turns a ~15-resize transition into 2-3, while keeping the click-capture
  // zone within one step of the real pill instead of the old fixed 380px band.
  const PILL_STEP_W = 24;
  const windowWidthFor = (contentW: number) =>
    Math.max(Math.ceil((contentW + PILL_PAD_W) / PILL_STEP_W) * PILL_STEP_W, MIN_PILL_WINDOW_W);
  const windowHeightFor = (contentH: number) =>
    Math.max(Math.round(contentH + PILL_PAD_H), MIN_PILL_WINDOW_H);
  let clusterEl: HTMLDivElement | null = null;
  let lastSentWidth = 0;
  let lastSentHeight = 0;
  // Deferred "final size" report: growth is sent immediately (so content is
  // never clipped mid-transition), but shrinking waits for ~100ms of quiet —
  // a shrinking pill's ResizeObserver fires every animation frame, and chasing
  // it with a native resize per frame is exactly the flicker this avoids.
  let settleTimer: ReturnType<typeof setTimeout> | null = null;

  // At most one set_pill_size call is ever in flight — content-fit resizing
  // fires on every animation frame of a width transition, and firing an
  // invoke per frame let concurrent/out-of-order native resizes visibly
  // stutter the window. A newer size while one is in flight just overwrites
  // the pending slot, so only the latest value gets sent once the current
  // call resolves.
  let pillSizeInFlight = false;
  let pillSizePending: { w: number; h: number } | null = null;

  function sendPillSize(w: number, h: number) {
    pillSizeInFlight = true;
    import('@tauri-apps/api/core')
      .then(({ invoke }) => invoke('set_pill_size', { width: w, height: h }))
      .then(() => {
        lastSentWidth = w;
        lastSentHeight = h;
      })
      .catch(() => {})
      .finally(() => {
        pillSizeInFlight = false;
        if (pillSizePending) {
          const next = pillSizePending;
          pillSizePending = null;
          sendPillSize(next.w, next.h);
        }
      });
  }

  function reportPillSize(width: number, height: number) {
    const w = windowWidthFor(width);
    const h = windowHeightFor(height);
    if (pillSizeInFlight) {
      pillSizePending = w === lastSentWidth && h === lastSentHeight ? null : { w, h };
      return;
    }
    if (w === lastSentWidth && h === lastSentHeight) return;
    sendPillSize(w, h);
  }

  function measureAndResize() {
    if (!clusterEl) return;
    const w = windowWidthFor(clusterEl.offsetWidth);
    const h = windowHeightFor(clusterEl.offsetHeight);
    if (w > lastSentWidth || h > lastSentHeight) {
      reportPillSize(clusterEl.offsetWidth, clusterEl.offsetHeight);
    }
    if (settleTimer) clearTimeout(settleTimer);
    settleTimer = setTimeout(() => {
      settleTimer = null;
      if (clusterEl) reportPillSize(clusterEl.offsetWidth, clusterEl.offsetHeight);
    }, 100);
  }

  let pillResizeObserver: ResizeObserver | null = null;

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

  // Full rendered width of the expanded handsfree pill (mirrors
  // .pill.handsfree.hf-expanded's width calc exactly) — needed in JS so the
  // native window can be locked to it for the whole handsfree state (see
  // steady-width-hf below) instead of resizing when the Cancel/Confirm
  // buttons reveal.
  $: hfExpandedW = 12 * barW + 11 * barGap + 54;
  // Full width of the fixed "Loading model" pill (mirrors .pill.loading-local).
  const LOADING_LOCAL_W = 144;
  // The native window for the processing state must stay wide enough for
  // BOTH the widest stage label at rest AND the starting width of whichever
  // from-* entrance is actually playing. Handsfree and loading-local's pills
  // are both wider than "Transcribing…" + chrome, so a window sized only for
  // the steady state was too narrow for the first frame of those entrances —
  // the pill wanted to render wider than the window allowed, forcing a
  // grow-then-shrink native resize as the entrance keyframe played out. That
  // double resize is what read as "processing teleports/adjusts right as it
  // appears."
  //
  // Gated on prevState (which entrance is actually about to play), not an
  // unconditional max of every possible entrance: unconditionally including
  // loading-local's width made the WINDOW pay for a keyframe that wasn't
  // running, forcing a resize on the ordinary recording/handsfree path that
  // didn't need one — the exact class of bug this is fixing, just relocated.
  $: processingSteadyW = Math.max(
    stageMaxW + 24,
    prevState === 'handsfree' ? hfExpandedW : 0,
    prevState === 'loading_local_model' ? LOADING_LOCAL_W : 0
  );

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

  const BAR_MIN_H = 3;
  const BAR_MAX_H = 16;
  let barHeights: number[] = Array(BARS).fill(BAR_MIN_H);
  let targetLevel = 0;
  let smoothed = 0;
  let rafId = 0;
  const PEAK_FLOOR = 0.07;
  let adaptivePeak = PEAK_FLOOR;

  let lastAnimTime = 0;

  // Spatially-continuous shimmer: three sine waves of different spatial
  // frequencies drifting at different speeds, sampled once per bar. Because
  // it's a smooth function of (position, time), neighbouring bars always move
  // together as one flowing waveform. What this replaces gave every bar its
  // own independent random walk, re-rolled every 400ms and lerped between
  // rolls — which is why a steady voice read as a row of unrelated twitching
  // dots instead of audio, and why it managed to look both too static (the
  // walk barely travelled between rolls) and too restless (neighbours never
  // agreed on a direction). Being pure (u, t) math it also keeps the row alive
  // at a dead-constant input level, with no per-frame Math.random() at all.
  function shimmerAt(u: number, t: number): number {
    const wave =
      Math.sin(u * 3.1 + t * 1.9) * 0.5 +
      Math.sin(u * 6.3 - t * 1.3) * 0.3 +
      Math.sin(u * 1.7 + t * 2.7) * 0.2;
    return 0.72 + wave * 0.28; // ≈ 0.44 … 1.00
  }

  function animateBars(time: number) {
    if (!lastAnimTime) lastAnimTime = time;
    const dt = Math.min(time - lastAnimTime, 50);
    lastAnimTime = time;

    // Keep bar snapping aligned to whichever monitor the pill is currently on.
    refreshDpr();

    // Rise stays quick so the bars catch speech onsets. The fall used to decay
    // at 0.97/frame — ~1.3s to bottom out, so the row went on melting long
    // after the sound stopped and every syllable smeared into the next.
    // 0.90 lands in ~400ms: still smooth, but it actually tracks the voice.
    const riseRate = 1 - Math.pow(0.84, dt / 16.66);
    const fallRate = 1 - Math.pow(0.9, dt / 16.66);

    if (targetLevel > smoothed) {
      smoothed += (targetLevel - smoothed) * riseRate;
    } else {
      smoothed += (targetLevel - smoothed) * fallRate;
    }

    // Soft noise gate: subtract the floor rather than branching on it. The old
    // `if (smoothed < GATE) fill(3)` snapped all twelve bars flat in a single
    // frame the instant the level crossed the threshold, so a fading tail
    // stopped mid-fall and collapsed — the "drifts down slowly, then just
    // compacts" artifact. Subtracting is continuous, so the bars simply reach
    // the floor and stay there.
    const gated = Math.max(0, smoothed - GATE);

    if (gated > adaptivePeak) adaptivePeak = gated;
    // Re-adapt within a sentence or two. At the old 0.9997 the peak took ~40s
    // to forget a single loud moment, so everything after a cough or a laugh
    // normalised against it and sat near-flat.
    adaptivePeak = Math.max(PEAK_FLOOR, adaptivePeak * Math.pow(0.9985, dt / 16.66));

    // Normalize against the adaptive peak so quiet mics still drive bars to
    // full height; ease the mapping (pow 1.5) so small noises stay gentle.
    const eased = Math.pow(Math.min(gated / adaptivePeak, 1), 1.5);
    const t = time / 1000;

    barHeights = barGains.map((gain, i) => {
      const energy = eased * gain * shimmerAt(i / (BARS - 1), t);
      return BAR_MIN_H + energy * (BAR_MAX_H - BAR_MIN_H);
    });

    rafId = requestAnimationFrame(animateBars);
  }

  function startRaf() {
    if (rafId === 0) { lastAnimTime = 0; rafId = requestAnimationFrame(animateBars); }
  }
  function stopRaf() {
    if (rafId !== 0) { cancelAnimationFrame(rafId); rafId = 0; barHeights = Array(BARS).fill(BAR_MIN_H); }
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
      errHeight = 34;
      errLines = 1;
      errScroll = false;
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

  // Error surface geometry, in px — mirrors .pill.error's CSS. No status icon
  // (removed — the red palette plus Dismiss/Retry on either side already
  // read as "error" without it), so the collapsed capsule is bare padding
  // and briefly empty for the one frame before it opens.
  const ERROR_COLLAPSED_WIDTH = 28;
  // Everything in the row that is not the message: dismiss (18) + retry (18)
  // + the two 8px flex gaps flanking the text (dismiss↔text, text↔retry) +
  // 14px padding either side.
  const ERROR_CHROME_W = 80;
  // Width the message column wraps at. Past this the pill stops widening and
  // starts growing upward into a card instead.
  const ERROR_TEXT_MAX_W = 250;
  const ERROR_LINE_H = 15;
  // Beyond this the card stops growing and the message scrolls inside it.
  const ERROR_MAX_LINES = 7;
  const ERROR_CARD_PAD_V = 22; // 10px top + 10px bottom + a little slack
  const ERROR_SINGLE_H = 34; // the bare capsule — same as .pill's base height

  // Opens the error pill: render collapsed (icon-only), measure the message,
  // then grow to fit it so the CSS transitions have a starting value to
  // animate from. Short messages stay a single-line capsule and only widen;
  // longer ones cap their width, wrap, and grow upward into a rounded card;
  // past ERROR_MAX_LINES the card stops growing and the text scrolls.
  //
  // pill-error and pill-state arrive as separate events in unspecified order,
  // so openError() can be invoked again before or after a prior call
  // finishes. The task id lets a stale call bail out instead of clobbering a
  // newer one's measurement, and wasOpen skips the collapse phase on a
  // re-trigger so an already-open pill resizes in place rather than visibly
  // collapsing and re-expanding.
  let openErrorTaskId = 0;
  async function openError() {
    const taskId = ++openErrorTaskId;
    const wasOpen = errOpen;
    if (!wasOpen) {
      errOpen = false;
      errWidth = ERROR_COLLAPSED_WIDTH;
      errHeight = ERROR_SINGLE_H;
      errLines = 1;
      errScroll = false;
    }
    await tick();
    if (taskId !== openErrorTaskId || state !== 'error') return;

    const applySize = () => {
      // Measured off the hidden sizer, not the live .err-text: the live one is
      // a flex item squeezed to ~0 inside the collapsed capsule, so it can only
      // report a scrollWidth, which says nothing about how many lines the
      // message needs once it is allowed to wrap.
      const box = errSizerEl?.getBoundingClientRect();
      const textW = box ? Math.ceil(box.width) : 0;
      const lines = box ? Math.max(1, Math.round(box.height / ERROR_LINE_H)) : 1;

      if (textW <= 0) {
        errWidth = ERROR_COLLAPSED_WIDTH;
        errHeight = ERROR_SINGLE_H;
        errScroll = false;
      } else if (lines <= 1) {
        errWidth = textW + ERROR_CHROME_W;
        errHeight = ERROR_SINGLE_H;
        errScroll = false;
      } else {
        errWidth = ERROR_TEXT_MAX_W + ERROR_CHROME_W;
        const shown = Math.min(lines, ERROR_MAX_LINES);
        errHeight = shown * ERROR_LINE_H + ERROR_CARD_PAD_V;
        errScroll = lines > ERROR_MAX_LINES;
      }
      errLines = lines;
      errOpen = true;
      // Eagerly size the native window to the target before the CSS
      // transition starts, so the growing pill is never clipped while the
      // ResizeObserver catch-up lags behind the animation.
      reportPillSize(errWidth, errHeight);
    };

    if (wasOpen) {
      applySize();
    } else {
      requestAnimationFrame(() => {
        if (taskId !== openErrorTaskId || state !== 'error') return;
        applySize();
      });
    }

    // Re-measure once webfonts land. The first pass can run against the
    // fallback face, and the real face is usually wider — sizing to the
    // fallback is what truncated even short messages ("No speech detect…").
    document.fonts?.ready
      .then(() => {
        if (taskId !== openErrorTaskId || state !== 'error') return;
        applySize();
      })
      .catch(() => {});
  }

  onMount(() => {
    const unlisteners: Array<() => void> = [];
    let mounted = true;

    // Arm the cross-monitor DPI watcher (defined at component scope above so
    // refreshDpr can re-arm it as the pill moves between displays).
    armDprWatch();

    // Track the rendered content size so the native window stays snug around
    // it. Fires on every width transition/animation frame of the pill, on
    // label/stage text appearing, and on the initial idle mount (which
    // pre-sizes the window to the minimum before the first reveal).
    if (clusterEl && typeof ResizeObserver !== 'undefined') {
      pillResizeObserver = new ResizeObserver(() => measureAndResize());
      pillResizeObserver.observe(clusterEl);
    }

    (async () => {
      const { listen } = await import('@tauri-apps/api/event');

      const l1 = await listen<string>('pill-state', (ev) => {
        const incoming = (ev.payload as PillState) || 'idle';
        if (hfTimer !== null) { clearTimeout(hfTimer); hfTimer = null; }

        // A fresh recording starts a brand-new dictation: drop the previous
        // one's profile/stage. Terminal states are no longer "in progress", so
        // they clear too (idle is handled by goIdle).
        if (incoming !== 'processing') {
          clearStage();
        }
        if (incoming === 'recording') {
          profileLabel = null;
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
          errHeight = 34;
          errLines = 1;
          errScroll = false;
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
          // A brief beat (not the 1.2s this replaced) so the pill still reads
          // as "message, then it widens" rather than popping in fully formed
          // — but short enough that the widen reads as part of the entrance,
          // not as a second, disconnected animation arriving a while later.
          copyBtnTimer = setTimeout(() => {
            copyBtnTimer = null;
            if (state === 'paste_failed') showCopyBtn = true;
          }, 180);
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
      if (settleTimer) {
        clearTimeout(settleTimer);
        settleTimer = null;
      }
      pillResizeObserver?.disconnect();
      pillResizeObserver = null;
      mq?.removeEventListener('change', onDprChange);
      cancelAnimationFrame(rafId);
      if (errorTimer) clearTimeout(errorTimer);
      if (dyingTimer) clearTimeout(dyingTimer);
      if (stageTimer) clearTimeout(stageTimer);
      if (hfTimer) clearTimeout(hfTimer);
      if (cancelBtnTimer) clearTimeout(cancelBtnTimer);
      if (cancelDismissTimer) clearTimeout(cancelDismissTimer);
      if (copyBtnTimer) clearTimeout(copyBtnTimer);
      if (pasteFailedDismissTimer) clearTimeout(pasteFailedDismissTimer);
      if (copiedTimer) clearTimeout(copiedTimer);
      if (copiedPillTimer) clearTimeout(copiedPillTimer);
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

  // Manual early-out for the error toast, mirroring dismissCancelled below —
  // the 10s auto-dismiss timer is a fallback for "walked away", not a floor
  // on how long the pill has to sit there once the user has already seen it.
  function dismissError() {
    if (errorTimer) { clearTimeout(errorTimer); errorTimer = null; }
    goIdle();
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

  // Manual early-outs for the remaining transient/notification pills, mirroring
  // dismissError/dismissCancelled above — the auto-dismiss timers exist for
  // "walked away", not as a floor on how long an already-seen toast must sit.
  function dismissPasteFailed() {
    if (copyBtnTimer) { clearTimeout(copyBtnTimer); copyBtnTimer = null; }
    if (pasteFailedDismissTimer) { clearTimeout(pasteFailedDismissTimer); pasteFailedDismissTimer = null; }
    if (copiedTimer) { clearTimeout(copiedTimer); copiedTimer = null; }
    goIdle();
  }

  function dismissCopied() {
    if (copiedPillTimer) { clearTimeout(copiedPillTimer); copiedPillTimer = null; }
    goIdle();
  }
</script>

<!-- --bar-w/--bar-gap live on .wrap so every pill state (incl. processing, which
     has no bars of its own) inherits the DPI-snapped values for its width calc. -->
<div class="wrap" style="--bar-w:{barW}px; --bar-gap:{barGap}px; --stage-w:{stageW}px; --processing-steady-w:{processingSteadyW}px; --hf-expanded-w:{hfExpandedW}px">
  <div class="pill-cluster"
       class:steady-width={state === 'processing'}
       class:steady-width-hf={state === 'handsfree'}
       bind:this={clusterEl}>
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
        <!-- Hidden, always-complete copy of every label. The visible roll only
             mounts stages that actually ran, so it can't measure a stage that
             hasn't happened yet — but the window needs the widest label up
             front to hold one size for the whole processing state. -->
        <div class="stage-sizer" use:measureStageRows aria-hidden="true">
          {#each STAGE_ROWS as label (label)}
            <span class="stage-row" data-stage-label={label}>{label}</span>
          {/each}
        </div>
        <div class="stage-roll" style="transform: translateY({rollPos * -STAGE_ROW_H}px)">
          {#each activeStageRows as label (label)}
            <span class="stage-row">
              <span class="stage-label">{label}</span>
              <span class="stage-label stage-shine" aria-hidden="true">{label}</span>
            </span>
          {/each}
        </div>
      </div>
    </div>

  {:else if state === 'loading_local_model'}
    <div class="pill loading-local" class:from-processing={prevState === 'processing'} class:dying={dying}>
      <div class="loading-spinner"></div>
      <span>Loading model</span>
    </div>

  {:else if state === 'error'}
    <!-- Hidden copy of the message, allowed to wrap at the real column width.
         The live .err-text cannot be measured for this: inside the collapsed
         capsule it is a flex item squeezed to ~0, so it can report a
         scrollWidth but never a truthful line count. -->
    <span class="err-sizer" bind:this={errSizerEl} aria-hidden="true">{errorMsg || 'Something went wrong'}</span>
    <div class="pill error" class:err-open={errOpen} class:err-card={errLines > 1} class:err-scroll={errScroll} class:dying={dying}
         style={errWidth ? `width:${errWidth}px; height:${errHeight}px` : ''}>
      {#if errOpen}
        <!-- No status icon — the red palette plus Dismiss/Retry on either
             side already say "error" without a redundant exclamation mark.
             Dismiss sits at the left with Retry pinned to the opposite
             (right) edge — a passive "close" and an active "do something
             about it" read better kept apart than stacked on one side.
             Manual early-out: the pill otherwise sits for the full 10s
             auto-dismiss even once the user has already read it. -->
        <button class="hf-btn err-dismiss" onclick={dismissError} aria-label="Dismiss">
          <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round">
            <path d="M6 6l12 12M6 18 18 6"/>
          </svg>
        </button>
      {/if}
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
        <button class="hf-btn pf-dismiss" onclick={dismissPasteFailed} aria-label="Dismiss">
          <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round">
            <path d="M6 6l12 12M6 18 18 6"/>
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
      <button class="hf-btn copied-dismiss" onclick={dismissCopied} aria-label="Dismiss">
        <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round">
          <path d="M6 6l12 12M6 18 18 6"/>
        </svg>
      </button>
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
</div>

<style>
  :global(*, *::before, *::after) { box-sizing: border-box; }

  /* The pill window is a transparent floating capsule with no page background
     of its own. theme.css declares `color-scheme: light` on :root for the main
     app, and under a light color-scheme the UA paints the viewport canvas
     WHITE wherever html/body are transparent — so every native resize briefly
     showed white in the newly exposed area before the page composited over it.
     That is the flash when a narrow recording capsule jumps straight to a wide
     error pill. `normal` opts out of the UA canvas colour entirely and leaves
     it transparent. The selector is doubled deliberately: theme.css sets this
     on a plain `:root`, and `html` (below) or a single `:root` would tie or
     lose — tying leaves the winner up to stylesheet injection order, which is
     a build detail we should not depend on. `:root:root` outranks it outright. */
  :global(:root:root) { color-scheme: normal; }

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

  /* Measured wrapper — the backend sizes the native window to this element's
     size (see measureAndResize) so the transparent click-capture zone stays
     as small as the visible content. Column layout: the profile label floats
     above the capsule (centered) instead of pushing it off-center. */
  .pill-cluster {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    /* containing block for .err-sizer, which must stay out of flow so it
       never contributes to the measured cluster size */
    position: relative;
  }
  /* While processing, hold the measured cluster at processingSteadyW so the
     native window keeps one size for the whole state. The capsule itself
     still animates narrower per stage — only the invisible window stays put.
     Resizing per stage moved the window horizontally (it is re-centred on
     every size change) and WebView2 can present the previous frame at the new
     geometry, which read as the pill twitching sideways a few px and snapping
     back, most visibly on the big transcribing→pasting jump. processingSteadyW
     (computed in JS) is the max of the widest stage label AND the widest
     from-* entrance's starting width (handsfree/loading-local pills are both
     wider than "Transcribing…" + chrome) — sizing only for the steady state
     left the window too narrow for the first frame of those entrances, so the
     window had to grow then immediately shrink back as the entrance keyframe
     played out. */
  .pill-cluster.steady-width {
    min-width: var(--processing-steady-w, 144px);
  }
  /* Same idea for handsfree: it has exactly two widths (collapsed / button-
     expanded) that it CSS-transitions between, same shape as processing's
     per-stage widths, so it gets the same fix — hold the window at the wider
     (expanded) size for the whole state rather than resizing when the
     Cancel/Confirm buttons reveal ~150ms after mount. */
  .pill-cluster.steady-width-hf {
    min-width: var(--hf-expanded-w, 132px);
  }

  /* Resolved tone profile, shown as a small floating tag above the pill so
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

  /* Translate distances stay inside the window's vertical padding (PILL_PAD_H
     / 2 per side). At the old 8px/6px against a 5px pad the pill's bottom edge
     was clipped by the window for the whole entrance and exit. */
  @keyframes pillIn {
    from { transform: translateY(6px) scale(0.94); opacity: 0; }
    to   { transform: translateY(0) scale(1); opacity: 1; }
  }

  @keyframes pillOut {
    from { transform: translateY(0) scale(1); opacity: 1; }
    to   { transform: translateY(5px) scale(0.9); opacity: 0; }
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

  /* Processing: the stage text fills and centers in the pill (same center as
     the recording bars) — nothing appears to slide sideways between
     recording and processing. */
  /* Width follows the active stage label (--stage-w, measured in
     measureStageRows) instead of a fixed 140px sized for the longest one, so
     a short stage like "Pasting…" is a short pill. The transition carries it
     between stages; the from-* entrance keyframes below deliberately use
     `backwards` fill so this base width — not a baked-in end value — governs
     once the entrance finishes. */
  .pill.processing {
    width: calc(var(--stage-w, 116px) + 24px);
    padding: 0 12px;
    position: relative;
    transition: width 0.26s cubic-bezier(0.22, 1, 0.36, 1);
  }
  .pill.loading-local { width: 144px; padding: 0 14px; gap: 9px; }

  /* Recording→processing: grow in width. No overshoot — see .pill.error's
     transition comment; a bouncy width keyframe chases the native window
     resize the same way a bouncy width transition does. */
  /* These entrance keyframes declare only `from` and use `backwards` fill, so
     each animates into the element's own base width and then hands control
     back to it. With a baked-in `to` and `both` fill the filled end value
     would outrank the base rule forever, freezing the pill at that width and
     killing every later stage-to-stage width transition.

     Each is paired with pillIn (comma-separated, not a replacement — see the
     loading-spinner rule below for the same pattern) so entering processing
     gets the same scale/opacity fade every other pill state gets. Without it,
     the whole handsfree capsule (bars, buttons) just hard-cut to processing's
     text mid-frame — the width eased, but nothing else did, so it read as
     "no transition, it just appears" rather than one morph. pillIn only
     touches transform/opacity, the width keyframes only touch width, so the
     two apply to disjoint properties and don't fight each other. */
  .pill.processing.from-rec {
    animation: processIn 0.28s cubic-bezier(0.22, 1, 0.36, 1) backwards,
               pillIn 0.22s cubic-bezier(0.34, 1.56, 0.64, 1) both;
  }
  @keyframes processIn {
    /* start from the recording pill's DPI-snapped width so there's no jump */
    from { width: calc(12 * var(--bar-w, 3px) + 11 * var(--bar-gap, 2px) + 14px); }
  }

  /* Handsfree→processing: pill shrinks slightly */
  .pill.processing.from-hf {
    animation: processFromHf 0.25s ease-out backwards,
               pillIn 0.22s cubic-bezier(0.34, 1.56, 0.64, 1) both;
  }
  @keyframes processFromHf {
    /* start from the expanded handsfree pill's DPI-snapped width (bars + 54px of
       buttons/padding/gaps) so there's no jump at fractional DPI */
    from { width: calc(12 * var(--bar-w, 3px) + 11 * var(--bar-gap, 2px) + 54px); }
  }

  /* Processing→loading model: grow smoothly into the wider pill instead of
     popping in fresh, so a cold local model load reads as one continuous
     motion rather than two disconnected "new pill appeared" pops. */
  .pill.loading-local.from-processing {
    animation: loadingLocalIn 0.26s cubic-bezier(0.22, 1, 0.36, 1) both;
  }
  @keyframes loadingLocalIn {
    /* start from whatever width the processing pill was actually showing */
    from { width: calc(var(--stage-w, 116px) + 24px); }
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
     transcription/cleanup resumes, mirroring the handsfree→processing shrink.
     Paired with pillIn for the same reason as from-rec/from-hf above. */
  .pill.processing.from-loading {
    animation: processFromLoading 0.26s ease-out backwards,
               pillIn 0.22s cubic-bezier(0.34, 1.56, 0.64, 1) both;
  }
  @keyframes processFromLoading {
    from { width: 144px; }
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

  /* Stage counter: the stages that have run stay mounted in one vertical
     stack and the active one is picked by a translateY roll — no remounting,
     no opacity flashes, just a mechanical-counter roll between stages. The
     clip window is exactly one row tall. The counter fills the pill so the
     text stays dead-centered — the same center as the recording bars — with
     no other element to shift it sideways between states. */
  .stage-counter {
    flex: 1;
    height: 16px;
    overflow: hidden;
    display: flex;
    justify-content: center;
    position: relative;
  }
  /* Out of flow and invisible — exists purely to be measured. `align-items:
     flex-start` is load-bearing: as block children the rows would stretch to
     the container and every label would measure the same (widest) width. */
  .stage-sizer {
    position: absolute;
    top: 0;
    left: 0;
    visibility: hidden;
    pointer-events: none;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
  }
  /* `align-items: center` matters twice over: it lets each row shrink to its
     own text (a stretched row would report the widest label's width, so the
     per-stage measurement in measureStageRows would be useless), and it keeps
     every label centered on the same axis while the pill resizes around it. */
  .stage-roll {
    display: flex;
    flex-direction: column;
    align-items: center;
    /* Never shrink to the counter. As a normal flex item the roll would be
       squeezed whenever the pill was sized for a shorter stage, which both
       clipped the longer labels and fed a smaller number back into the
       measurement it was derived from. Holding its natural width lets it
       overflow the clip window symmetrically instead, so the active (centered)
       label always lands exactly inside it. */
    flex-shrink: 0;
    will-change: transform;
    transition: transform 0.28s cubic-bezier(0.22, 1, 0.36, 1);
  }
  /* Each stage row is two stacked copies of the same label: a fully legible
     base (so the text never reads as dim/broken) plus a second copy clipped
     to a moving gradient that rides on top as a soft highlight scanning back
     and forth across the word — this is the sole "still working" indicator
     for the pill's processing state, replacing a separate spinner icon.
     Splitting these into two layers (rather than the single dim-text-with-
     a-bright-band version this replaced) keeps the label readable at every
     point in the animation instead of the whole word dipping toward
     transparent between sweeps. */
  .stage-row {
    position: relative;
    height: 16px;
    line-height: 16px;
    font-size: 11px;
    font-weight: 600;
    white-space: nowrap;
    display: block;
  }
  /* Dimmed base. --pill-fg is pure #fff, so painting a white highlight over a
     full-brightness base made the sweep mathematically invisible — the label
     just read as solid white until it rolled to the next stage. The base now
     sits at 45% so the moving highlight has something to be brighter than. */
  .stage-label {
    display: block;
    color: rgba(255, 255, 255, 0.45);
  }
  .stage-shine {
    position: absolute;
    inset: 0;
    color: transparent;
    /* Narrow scanning band, sized in px (not %) so it spans the same ~3
       characters no matter how long the stage word is: a pure-white core with
       half-white shoulders either side, fading out at both edges. */
    background-image: linear-gradient(
      90deg,
      rgba(255, 255, 255, 0) 0%,
      rgba(255, 255, 255, 0.5) 25%,
      rgba(255, 255, 255, 1) 50%,
      rgba(255, 255, 255, 0.5) 75%,
      rgba(255, 255, 255, 0) 100%
    );
    background-size: 24px 100%;
    background-repeat: no-repeat;
    background-clip: text;
    -webkit-background-clip: text;
    /* Fast enough that a short-lived stage still shows real travel — "Pasting…"
       often lasts only a few hundred ms, and at the 1.9s this replaces the
       band barely crept across it before the stage was gone. */
    animation: stage-scan 1.05s ease-in-out infinite alternate;
  }
  /* Sweeps horizontally across the letters and back (alternate). The travel is
     bounded so the band stays *on* the word at both ends — 0% puts its left
     edge at the first letter, 100% its right edge at the last. Letting it run
     clear of the text (the -24px … 100%+24px it replaced) left the whole label
     sitting dim at each turnaround, which read as the text fading out rather
     than as something scanning across it. */
  @keyframes stage-scan {
    from { background-position: 0% 0; }
    to   { background-position: 100% 0; }
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

  /* Error: dark red-tinted surface. A short message stays a single-line
     capsule and only widens; a long one caps its width, wraps, and grows
     upward into a rounded card. 17px is exactly half the 34px capsule height,
     so the same radius reads as a perfect stadium when short and as softly
     rounded corners once it grows — no radius animation needed, and no
     awkward interpolation between two shapes. */
  .pill.error {
    width: 42px;
    height: 34px;
    gap: 8px;
    padding: 0 14px;
    background: var(--pill-error-bg);
    color: var(--pill-error-fg);
    border-radius: 17px;
    box-shadow: 0 0 0 1px var(--pill-error-border),
                0 2px 8px rgba(0,0,0,0.32);
    overflow: hidden;
    /* No overshoot here — a bouncy curve makes the measured content
       transiently exceed its target mid-transition, which chases the native
       window resize past-then-back and reads as flicker. */
    transition: width 0.3s cubic-bezier(0.22, 1, 0.36, 1),
                height 0.3s cubic-bezier(0.22, 1, 0.36, 1),
                padding 0.3s cubic-bezier(0.22, 1, 0.36, 1);
  }
  .err-icon {
    flex-shrink: 0;
    color: var(--pill-error-fg);
  }
  .err-text {
    font-size: 11.5px; font-weight: 500;
    line-height: 15px;
    color: var(--pill-error-fg);
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
    opacity: 0;
    transition: opacity 0.18s ease 0.08s;
  }
  .pill.error.err-open .err-text { opacity: 1; }

  /* Card form: message wraps to its own column, buttons pin to the top so
     they stay put as the body grows upward. */
  .pill.error.err-card {
    align-items: flex-start;
    padding: 10px 14px;
  }
  .pill.error.err-card .err-text {
    flex: 1;
    white-space: normal;
    overflow-wrap: anywhere;
    text-overflow: clip;
    max-height: 100%;
  }
  /* Only past ERROR_MAX_LINES does the body actually scroll; below that the
     card simply grew to fit, and a scrollbar would be dead chrome. */
  /* Standard scrollbar properties only — Chromium ignores ::-webkit-scrollbar
     entirely once scrollbar-width/-color are set, so the two cannot be
     combined. The thumb is tinted from the error foreground rather than the
     border colour, which was too close to the background to read as an
     affordance that there is more text below. */
  .pill.error.err-card.err-scroll .err-text {
    overflow-y: auto;
    padding-right: 6px;
    scrollbar-width: thin;
    scrollbar-color: rgba(255, 143, 128, 0.45) transparent;
  }
  /* Both buttons pin to the card's top edge alongside the first line, rather
     than stretching full-height or centering against a body that keeps
     growing taller as the message wraps to more lines. */
  .pill.error.err-card .err-retry,
  .pill.error.err-card .err-dismiss {
    margin-top: 1px;
  }

  /* Out of flow and invisible — the message at its real wrapping width, so
     openError can read both the natural width and the true line count. */
  .err-sizer {
    position: absolute;
    visibility: hidden;
    pointer-events: none;
    /* `width: max-content` is load-bearing. As an out-of-flow box the sizer's
       shrink-to-fit width is bounded by its containing block — the pill
       cluster, often under 100px — so with max-width alone every message
       wrapped at the cluster's width and measured as far more lines than it
       actually needs. max-content asks for the full single-line width and
       lets max-width cap it at the real column. */
    width: max-content;
    max-width: 250px; /* ERROR_TEXT_MAX_W */
    font-size: 11.5px;
    font-weight: 500;
    line-height: 15px;
    white-space: normal;
    overflow-wrap: anywhere;
  }

  .hf-btn.err-retry {
    color: var(--pill-error-fg);
    animation: hfBtnIn 0.12s cubic-bezier(0.34, 1.56, 0.64, 1) 0.13s both;
  }
  .hf-btn.err-retry:hover { background: rgba(255,255,255,0.14); }

  /* Delay is slightly ahead of err-retry's — dismiss now sits left of the
     text and retry sits right of it, so the reveal reads left-to-right. */
  .hf-btn.err-dismiss {
    color: var(--pill-error-fg);
    opacity: 0.75;
    animation: hfBtnIn 0.12s cubic-bezier(0.34, 1.56, 0.64, 1) 0.1s both;
  }
  .hf-btn.err-dismiss:hover { opacity: 1; background: rgba(255,255,255,0.14); }

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
    /* No overshoot — see .pill.error's transition comment. */
    transition: width 0.26s cubic-bezier(0.22, 1, 0.36, 1),
                padding 0.26s cubic-bezier(0.22, 1, 0.36, 1);
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
     short beat (180ms — see copyBtnTimer) so "Not pasted" reads as arriving
     first rather than the whole pill popping in fully formed. Long enough to
     read as sequenced, short enough that the widen still feels like part of
     one entrance instead of a second animation arriving a while later. */
  .pill.paste-failed {
    gap: 8px;
    padding: 0 14px;
    background: var(--pill-error-bg);
    color: var(--pill-error-fg);
    border-radius: 999px;
    box-shadow: 0 0 0 1px var(--pill-error-border),
                0 2px 8px rgba(0,0,0,0.32);
    width: 128px;
    overflow: hidden;
    /* No overshoot — see .pill.error's transition comment. */
    transition: width 0.26s cubic-bezier(0.22, 1, 0.36, 1),
                padding 0.26s cubic-bezier(0.22, 1, 0.36, 1);
  }
  /* +26px over the single-button version (18px dismiss button + one more
     8px flex gap) now that Copy and Dismiss show together. */
  .pill.paste-failed.copy-open {
    width: 184px;
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

  .hf-btn.pf-dismiss {
    color: var(--pill-error-fg);
    opacity: 0.75;
    animation: hfBtnIn 0.12s cubic-bezier(0.34, 1.56, 0.64, 1) 0.03s both;
  }
  .hf-btn.pf-dismiss:hover { opacity: 1; background: rgba(255,255,255,0.14); }

  /* Copied confirmation: neutral (not error-red) stadium pill for the global
     Ctrl+Alt+C / ⌥⌘C shortcut — fixed short message, auto-sized, plus a
     dismiss button so it doesn't have to be waited out either. Width is
     content-driven (no fixed value or open/collapsed states to juggle), so
     the button just joins the flex row and the usual ResizeObserver sizing
     picks up the extra space. */
  .pill.copied {
    gap: 8px;
    padding: 0 8px 0 14px;
    white-space: nowrap;
  }
  .copied-icon { color: var(--accent); flex-shrink: 0; }
  .copied-text {
    font-size: 11.5px; font-weight: 500;
    white-space: nowrap;
  }
  .hf-btn.copied-dismiss {
    color: var(--pill-muted);
    animation: hfBtnIn 0.12s cubic-bezier(0.34, 1.56, 0.64, 1) 0.05s both;
  }
  .hf-btn.copied-dismiss:hover { color: var(--pill-muted-strong); background: rgba(255,255,255,0.14); }

  /* Handsfree: starts compact (mirrors recording — same DPI-snapped width so the
     recording→handsfree continuation doesn't jump), expands to 112px after 450ms */
  .pill.handsfree {
    width: calc(12 * var(--bar-w, 3px) + 11 * var(--bar-gap, 2px) + 14px);
    padding: 0 7px;
    gap: 2px;
    /* No overshoot — see .pill.error's transition comment. */
    transition: width 0.2s cubic-bezier(0.22, 1, 0.36, 1),
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
