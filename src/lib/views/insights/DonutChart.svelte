<script lang="ts">
  /*
   * Canvas donut with filleted (rounded-cap) segments, gap-separated sweeps,
   * a value tween on data change, and a soft hover interaction that widens
   * the hovered segment and dims the rest — both from the ring and from the
   * legend. No chart library; the ring is hand-drawn per frame.
   */
  import { onMount, onDestroy, untrack } from 'svelte';
  import { reducedMotionEnabled } from '../../motion';

  export interface DonutSegment {
    id: string;
    name: string;
    color: string;
    value: number;
    valueLabel: string;
  }

  let {
    segments,
    primaryLabel,
    secondaryLabel = '',
    size = 136,
  }: { segments: DonutSegment[]; primaryLabel: string; secondaryLabel?: string; size?: number } = $props();

  const TAU = Math.PI * 2;
  const TWEEN_MS = 320;
  const HOVER_MS = 120;
  const STROKE_FRACTION = 0.11;
  const RADIUS_FRACTION = 0.335;
  const GAP = 3.5;
  const CORNER = 2.75;
  const HOVER_WIDEN = 6;
  const HOVER_POP = 5;
  const DIM_ALPHA = 0.32;

  const easeOutCubic = (t: number) => 1 - (1 - t) ** 3;

  interface Point { x: number; y: number }
  interface Fillet { filletCenter: Point; flatFacePoint: Point; ringPoint: Point; angle: number }

  /**
   * atan2 always returns a value in (-π, π], but segment boundaries are
   * cumulative and range well past that (a ring can sweep past 3π/2 before
   * wrapping back to 12 o'clock). Re-expressing the wrapped result as the
   * angle nearest `reference` keeps boundaries monotonically increasing
   * across the whole sweep — without this, any segment whose end crossed
   * the ±π seam got a start/end pair that looked "backwards" and silently
   * failed to render.
   */
  function unwrapNear(angle: number, reference: number): number {
    let a = angle;
    while (a - reference > Math.PI) a -= TAU;
    while (a - reference < -Math.PI) a += TAU;
    return a;
  }

  function makeFillet(
    center: Point,
    boundaryAngle: number,
    capOffset: number,
    inward: number,
    circleRadius: number,
    outer: boolean,
    corner: number,
  ): Fillet {
    const nx = Math.cos(boundaryAngle);
    const ny = Math.sin(boundaryAngle);
    const tx = -ny;
    const ty = nx;
    const filletRadius = outer ? circleRadius - corner : circleRadius + corner;
    const tangentOffset = capOffset + inward * corner;
    const radialOffset = Math.sqrt(Math.max(0, filletRadius * filletRadius - tangentOffset * tangentOffset));
    const filletCenter = {
      x: center.x + nx * radialOffset + tx * tangentOffset,
      y: center.y + ny * radialOffset + ty * tangentOffset,
    };
    const flatFacePoint = {
      x: center.x + nx * radialOffset + tx * capOffset,
      y: center.y + ny * radialOffset + ty * capOffset,
    };
    const circleScale = circleRadius / filletRadius;
    const ringPoint = {
      x: center.x + (filletCenter.x - center.x) * circleScale,
      y: center.y + (filletCenter.y - center.y) * circleScale,
    };
    const rawAngle = Math.atan2(ringPoint.y - center.y, ringPoint.x - center.x);
    return { filletCenter, flatFacePoint, ringPoint, angle: unwrapNear(rawAngle, boundaryAngle) };
  }

  /** Shortest-path arc between two points around a shared small-radius center — the fillet corners. */
  function minorArc(path: Path2D, center: Point, radius: number, from: Point, to: Point) {
    const a0 = Math.atan2(from.y - center.y, from.x - center.x);
    const a1raw = Math.atan2(to.y - center.y, to.x - center.x);
    let diff = a1raw - a0;
    while (diff > Math.PI) diff -= TAU;
    while (diff < -Math.PI) diff += TAU;
    path.arc(center.x, center.y, radius, a0, a0 + diff, diff < 0);
  }

  function donutSegmentPath(
    cx: number, cy: number, startBoundary: number, endBoundary: number,
    radius: number, width: number, gap: number, corner: number,
  ): Path2D | null {
    const center = { x: cx, y: cy };
    const outerRadius = radius + width / 2;
    const innerRadius = radius - width / 2;
    const sO = makeFillet(center, startBoundary, gap / 2, 1, outerRadius, true, corner);
    const sI = makeFillet(center, startBoundary, gap / 2, 1, innerRadius, false, corner);
    const eO = makeFillet(center, endBoundary, -gap / 2, -1, outerRadius, true, corner);
    const eI = makeFillet(center, endBoundary, -gap / 2, -1, innerRadius, false, corner);
    if (eO.angle <= sO.angle || eI.angle <= sI.angle) return null;
    const p = new Path2D();
    p.moveTo(sO.ringPoint.x, sO.ringPoint.y);
    p.arc(cx, cy, outerRadius, sO.angle, eO.angle, false);
    minorArc(p, eO.filletCenter, corner, eO.ringPoint, eO.flatFacePoint);
    p.lineTo(eI.flatFacePoint.x, eI.flatFacePoint.y);
    minorArc(p, eI.filletCenter, corner, eI.flatFacePoint, eI.ringPoint);
    p.arc(cx, cy, innerRadius, eI.angle, sI.angle, true);
    minorArc(p, sI.filletCenter, corner, sI.ringPoint, sI.flatFacePoint);
    p.lineTo(sO.flatFacePoint.x, sO.flatFacePoint.y);
    minorArc(p, sO.filletCenter, corner, sO.flatFacePoint, sO.ringPoint);
    p.closePath();
    return p;
  }

  /** Exact annular sector, no fillet — the fallback for slices too thin for the corner treatment. */
  function sectorPath(cx: number, cy: number, startBoundary: number, endBoundary: number, radius: number, width: number): Path2D {
    const outerRadius = radius + width / 2;
    const innerRadius = radius - width / 2;
    const p = new Path2D();
    p.arc(cx, cy, outerRadius, startBoundary, endBoundary, false);
    p.arc(cx, cy, innerRadius, endBoundary, startBoundary, true);
    p.closePath();
    return p;
  }

  // One probe element resolves any CSS color (including color-mix()) to a
  // concrete value canvas can always paint — canvas fillStyle parsing is
  // less permissive than CSS in some WebView builds.
  let colorProbe: HTMLDivElement | null = null;
  function resolveColor(css: string): string {
    if (typeof document === 'undefined') return css;
    if (!colorProbe) {
      colorProbe = document.createElement('div');
      colorProbe.style.position = 'absolute';
      colorProbe.style.visibility = 'hidden';
      colorProbe.style.pointerEvents = 'none';
      document.body.appendChild(colorProbe);
    }
    // Clear any previous inline color first — CSSOM silently ignores an
    // invalid assignment, so without a reset a failed resolve would leave the
    // previous call's color behind and report that instead.
    colorProbe.style.color = '';
    colorProbe.style.color = css;
    return getComputedStyle(colorProbe).color || css;
  }

  let canvasEl = $state<HTMLCanvasElement | null>(null);
  let ctx: CanvasRenderingContext2D | null = null;
  let rafId = 0;

  let hoveredId = $state<string | null>(null);

  let animFrom = new Map<string, number>();
  let animTo = new Map<string, number>();
  let animStart = 0;
  let resolvedColors = new Map<string, string>();
  // Resolved once per data change (with the segment colors) rather than on
  // every paint frame — getComputedStyle() on each rAF tick forces style
  // recalc/reflow at 60–120fps.
  let trackColor = '';

  let dimming = false; // is *something* currently hovered, for the dim tween
  let dimStart = 0;
  let dimFromAlpha = 1;

  // Per-segment widen/pop progress (0..1), tracked individually so the
  // previously-focused segment eases back down instead of snapping when the
  // hover target changes — only the "grow in" direction had a tween before.
  let widenFrom = new Map<string, number>();
  let widenTarget = new Map<string, number>();
  let widenStart = new Map<string, number>();

  function currentWiden(id: string): number {
    const from = widenFrom.get(id) ?? 0;
    const target = widenTarget.get(id) ?? 0;
    const start = widenStart.get(id) ?? 0;
    const t = easeOutCubic(Math.min(1, (performance.now() - start) / HOVER_MS));
    return from + (target - from) * t;
  }

  function startValueTween() {
    const next = new Map<string, number>();
    for (const seg of segments) {
      next.set(seg.id, animTo.get(seg.id) ?? 0);
    }
    animFrom = next;
    animTo = new Map(segments.map((s) => [s.id, s.value]));
    animStart = performance.now();
    resolvedColors = new Map(segments.map((s) => [s.id, resolveColor(s.color)]));
    trackColor = resolveColor('var(--control-hover)');
    scheduleFrame();
  }

  $effect(() => {
    // Re-run whenever the segment set or its values change.
    segments;
    // A hovered segment may no longer exist after a data change (e.g. range
    // filter removed it) — a stale id would dim the whole chart and legend
    // until the next hover. Drop it and any dim state.
    untrack(() => {
      if (hoveredId !== null && !segments.some((s) => s.id === hoveredId)) {
        hoveredId = null;
        dimming = false;
        dimFromAlpha = 1;
      }
      startValueTween();
    });
  });

  function setHover(id: string | null) {
    if (id === hoveredId) return;
    const wasActive = hoveredId !== null;
    const nowActive = id !== null;
    if (wasActive !== nowActive) {
      dimFromAlpha = currentDimAlpha();
      dimming = nowActive;
      dimStart = performance.now();
    }
    const now = performance.now();
    // The old target eases back to 0; the new one eases up to 1 — both
    // captured from wherever they currently sit, so a fast re-hover doesn't
    // jump.
    if (hoveredId !== null) {
      widenFrom.set(hoveredId, currentWiden(hoveredId));
      widenTarget.set(hoveredId, 0);
      widenStart.set(hoveredId, now);
    }
    if (id !== null) {
      widenFrom.set(id, currentWiden(id));
      widenTarget.set(id, 1);
      widenStart.set(id, now);
    }
    hoveredId = id;
    scheduleFrame();
  }

  function currentDimAlpha(): number {
    const t = easeOutCubic(Math.min(1, (performance.now() - dimStart) / HOVER_MS));
    const target = dimming ? DIM_ALPHA : 1;
    return dimFromAlpha + (target - dimFromAlpha) * t;
  }

  function scheduleFrame() {
    if (rafId) return;
    rafId = requestAnimationFrame(paint);
  }

  function paint() {
    rafId = 0;
    if (!ctx || !canvasEl) return;
    const now = performance.now();
    const reduced = reducedMotionEnabled();

    const tweenT = reduced ? 1 : easeOutCubic(Math.min(1, (now - animStart) / TWEEN_MS));
    const values = segments.map((s) => {
      const from = animFrom.get(s.id) ?? 0;
      const to = animTo.get(s.id) ?? s.value;
      return from + (to - from) * tweenT;
    });
    const total = values.reduce((a, b) => a + b, 0);

    const dimT = reduced ? 1 : easeOutCubic(Math.min(1, (now - dimStart) / HOVER_MS));
    const dimAlpha = reduced
      ? (dimming ? DIM_ALPHA : 1)
      : dimFromAlpha + ((dimming ? DIM_ALPHA : 1) - dimFromAlpha) * dimT;

    const w = size;
    const h = size;
    ctx.clearRect(0, 0, w, h);
    const cx = w / 2;
    const cy = h / 2;
    const radius = size * RADIUS_FRACTION;
    const stroke = size * STROKE_FRACTION;

    // Track ring underneath — keeps the shape legible with zero/tiny data.
    ctx.beginPath();
    ctx.arc(cx, cy, radius, 0, TAU);
    ctx.strokeStyle = trackColor || resolveColor('var(--control-hover)');
    ctx.lineWidth = stroke;
    ctx.stroke();

    if (total > 0) {
      let cursor = -Math.PI / 2; // 12 o'clock
      segments.forEach((seg, i) => {
        const fraction = values[i] / total;
        const start = cursor;
        const end = cursor + fraction * TAU;
        cursor = end;
        if (fraction <= 0) return;

        const dimmed = hoveredId !== null && hoveredId !== seg.id;
        // Every segment eases its own widen/pop toward whatever it's
        // currently targeting (1 while hovered, 0 otherwise) — a pure width
        // increase (growing equally in/out) reads as barely-there at this
        // scale, so it pops outward too, and the *previous* hover target
        // eases back down instead of snapping when you move to a new slice.
        const widenT = reduced ? (widenTarget.get(seg.id) ?? 0) : currentWiden(seg.id);
        const width = stroke + HOVER_WIDEN * widenT;
        const segRadius = radius + HOVER_POP * widenT;

        const sweep = end - start;
        const minSweepForFillet = ((GAP + CORNER * 2) / radius) * 1.4;
        // For sub-fillet sweeps, shrink the arc bounds by a tiny amount so
        // the exact-annulus fallback never draws over its own boundary — but
        // never so much that start passes end (that would invert the arc and
        // sweep nearly 360 degrees).
        const inset = Math.min(0.01, sweep / 3);
        const filletPath =
          sweep > minSweepForFillet
            ? donutSegmentPath(cx, cy, start, end, segRadius, width, GAP, CORNER)
            : null;
        const path = filletPath ?? sectorPath(cx, cy, start + inset, end - inset, segRadius, width);

        ctx!.globalAlpha = dimmed ? dimAlpha : 1;
        ctx!.fillStyle = resolvedColors.get(seg.id) ?? seg.color;
        if (path) ctx!.fill(path);
      });
      ctx.globalAlpha = 1;
    }

    const widenSettled = reduced || segments.every((s) => {
      const from = widenFrom.get(s.id) ?? 0;
      const target = widenTarget.get(s.id) ?? 0;
      return from === target || currentWiden(s.id) === target;
    });
    const stillTweening = tweenT < 1 || dimT < 1 || !widenSettled;
    if (stillTweening) scheduleFrame();
  }

  function segmentAtPoint(x: number, y: number): string | null {
    const cx = size / 2;
    const cy = size / 2;
    const dx = x - cx;
    const dy = y - cy;
    const dist = Math.hypot(dx, dy);
    const radius = size * RADIUS_FRACTION;
    const stroke = size * STROKE_FRACTION;
    if (Math.abs(dist - radius) > stroke * 0.7) return null;

    const total = segments.reduce((a, b) => a + b.value, 0);
    if (total <= 0) return null;

    const raw = Math.atan2(dy, dx);
    const angle = (((raw + Math.PI / 2) % TAU) + TAU) % TAU;
    let cursor = 0;
    for (const seg of segments) {
      const fraction = seg.value / total;
      const next = cursor + fraction * TAU;
      if (angle >= cursor && angle < next) return seg.id;
      cursor = next;
    }
    return null;
  }

  function onCanvasMove(event: PointerEvent) {
    if (!canvasEl) return;
    const rect = canvasEl.getBoundingClientRect();
    const scaleX = size / rect.width;
    const scaleY = size / rect.height;
    const x = (event.clientX - rect.left) * scaleX;
    const y = (event.clientY - rect.top) * scaleY;
    setHover(segmentAtPoint(x, y));
  }

  onMount(() => {
    if (!canvasEl) return;
    syncCanvasSize();
    ctx = canvasEl.getContext('2d');
    applyDprScale();
    // Don't call startValueTween() here — the $effect tracking `segments`
    // already does on mount, and a second call would reset animFrom to the
    // target values (animTo already set by the first), skipping the initial
    // 0-to-value entrance. Just ensure a frame paints once the context is up.
    scheduleFrame();
  });

  // The size prop can change at runtime (responsive layout) — re-sync the
  // canvas backing store, CSS size, and the DPR context scale so paint()
  // never draws onto a stale/clipped buffer.
  $effect(() => {
    size;
    if (!ctx || !canvasEl) return;
    // Resizing width/height resets the context state and transform matrix to
    // identity, so the DPR scale must be re-applied *after* the resize.
    syncCanvasSize();
    applyDprScale();
    scheduleFrame();
  });

  function applyDprScale() {
    if (!ctx) return;
    const dpr = Math.max(1, window.devicePixelRatio || 1);
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  }

  function syncCanvasSize() {
    if (!canvasEl) return;
    const dpr = Math.max(1, window.devicePixelRatio || 1);
    canvasEl.width = size * dpr;
    canvasEl.height = size * dpr;
    canvasEl.style.width = `${size}px`;
    canvasEl.style.height = `${size}px`;
  }

  onDestroy(() => {
    if (rafId) cancelAnimationFrame(rafId);
    colorProbe?.remove();
    // Null it out too — a destroyed probe is a detached node, and any later
    // resolveColor() must create a fresh element rather than reuse it.
    colorProbe = null;
  });
</script>

<div class="donut">
  <div
    class="donut-ring-wrap"
    style:width="{size}px"
    style:height="{size}px"
    role="img"
    aria-label={`${primaryLabel}${secondaryLabel ? ' ' + secondaryLabel : ''}, split across ${segments.length} segments.`}
  >
    <canvas
      bind:this={canvasEl}
      onpointermove={onCanvasMove}
      onpointerleave={() => setHover(null)}
    ></canvas>
    <div class="donut-centre">
      <span class="donut-primary">{primaryLabel}</span>
      {#if secondaryLabel}<span class="donut-secondary">{secondaryLabel}</span>{/if}
    </div>
  </div>

  <ul class="donut-legend">
    {#each segments as seg}
      <li
        class:focused={hoveredId === seg.id}
        class:dimmed={hoveredId !== null && hoveredId !== seg.id}
        onpointerenter={() => setHover(seg.id)}
        onpointerleave={() => setHover(null)}
      >
        <span class="swatch" style:background={seg.color}></span>
        <span class="legend-name">{seg.name}</span>
        <span class="legend-value">{seg.valueLabel}</span>
      </li>
    {/each}
  </ul>
</div>

<style>
  .donut {
    display: flex;
    align-items: center;
    gap: 18px;
    flex-wrap: wrap;
  }

  .donut-ring-wrap {
    position: relative;
    flex: 0 0 auto;
  }

  canvas {
    display: block;
    cursor: default;
  }

  .donut-centre {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 1px;
    pointer-events: none;
  }
  .donut-primary {
    font-family: var(--serif);
    font-size: 19px;
    font-weight: 500;
    color: var(--ink);
    font-variant-numeric: tabular-nums;
    line-height: 1;
  }
  .donut-secondary {
    font-size: 10px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--ink-mute);
  }

  .donut-legend {
    list-style: none;
    margin: 0;
    padding: 0;
    /* Keep model prices beside their names instead of distributing them over
       the full editorial measure on wide windows. */
    flex: 0 1 560px;
    width: min(100%, 560px);
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .donut-legend li {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    padding: 4px 6px;
    margin: 0 -6px;
    border-radius: 6px;
    transition: background-color var(--ui-duration-fast) var(--ui-ease-out),
      transform var(--ui-duration-fast) var(--ui-ease-out),
      opacity var(--ui-duration-fast) var(--ui-ease-out);
  }
  .donut-legend li.focused {
    background: color-mix(in srgb, var(--accent) 10%, transparent);
    transform: translateX(2px);
  }
  .donut-legend li.dimmed { opacity: 0.55; }

  .swatch {
    width: 9px;
    height: 9px;
    border-radius: 3px;
    flex: 0 0 auto;
  }
  .legend-name {
    font-family: var(--mono);
    font-size: 11px;
    color: var(--ink-soft);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .legend-value {
    margin-left: auto;
    font-size: 11.5px;
    color: var(--ink);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
</style>
