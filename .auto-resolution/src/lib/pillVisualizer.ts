// Flowing audio visualizer for the dictation pill.
//
// The row is an audio ENVELOPE visualizer: not a PCM waveform, not a spectrum.
// Distance from the centre means AGE — the newest audio appears at the two
// middle bars and travels outward as it ages, mirrored left/right.
//
// Two rewrites got us here, and the second one needed a data change rather than
// a motion change:
//
//   1. Originally every bar drew the SAME instantaneous RMS value, scaled by a
//      fixed bell curve and a procedural shimmer. The row could only pump
//      uniformly; the only variation was sine noise fighting the signal.
//
//   2. Then bars sampled a history of that same scalar, so content did travel
//      outward — but it still read as a level meter, because one RMS value per
//      50ms is fundamentally insufficient to show flow. RMS over a window
//      exists precisely to average away everything inside that window, so a
//      sustained vowel produces a near-constant stream of identical numbers.
//      Carrying an unchanging number outward is motion you cannot see.
//
// So the backend now also emits a compact short-window PEAK envelope
// (`EnvelopeTap` in media/audio.rs, ~100 f32/sec on `audio-envelope`). At 10ms
// resolution a voiced vowel carries real pitch-period and vibrato structure, so
// a held "aaaa" genuinely varies even though its average level does not. That
// variation is the flow. Nothing here is procedural: no shimmer, no noise, no
// bounce. Every wiggle on screen came off the microphone.
//
// The chain, one stage per problem:
//   envelope samples -> dB -> slow reference window -> smoothing -> ring buffer
//   -> continuously advancing playhead -> per-bar age sampling -> spring -> px
//
// Lives outside PillApp.svelte so the stages can be driven directly by tests
// rather than verified against a re-typed copy of the same maths.

/** Bar count and height range — mirrors the pill's rendered geometry. */
export const BARS = 12;
export const BAR_MIN_H = 3;
export const BAR_MAX_H = 16;

/** Sub-window size of each envelope sample. MUST match ENVELOPE_WINDOW_MS in media/audio.rs. */
export const ENVELOPE_WINDOW_MS = 10;

// --- Geometry: how much time the row spans -----------------------------------
// Mirroring means 12 bars show only 6 distinct ages, so the age step decides how
// fast content visibly travels. At the old 110ms a peak took 550ms to reach the
// edge, which reads as settling rather than flowing. 45ms sends it centre-to-edge
// in ~225ms: clearly moving, while still showing a quarter-second of context.
const AGE_STEP_MS = 45;
const HALF_SPAN = (BARS - 1) / 2;

/** Oldest audio visible in the row, in ms — the outermost bar pair's age. */
export const HISTORY_SPAN_MS = HALF_SPAN * AGE_STEP_MS;

// Ring holds the visible span plus headroom for read latency and drift.
const RING_MS = HISTORY_SPAN_MS + 500;
const RING_LEN = Math.ceil(RING_MS / ENVELOPE_WINDOW_MS);

// --- Playhead ----------------------------------------------------------------
// The read position advances with REAL TIME every frame, not when a packet
// arrives — that is what makes motion continuous at display refresh instead of
// stepping at the packet rate. Packets only ever extend the write head.
//
// It is held a little behind the write head so that normal IPC jitter (batches
// arrive every ~50ms, not every 10ms) never starves the reader mid-frame. Drift
// between the audio clock and the display clock is corrected by easing toward
// the target rather than snapping, so a correction is never visible.
const TARGET_LATENCY_MS = 70;
const RESYNC_RATE = 0.06;

// --- Stage: dB ---------------------------------------------------------------
// Amplitude is a poor match for loudness, so everything downstream works in dB.
const SILENT_DB = -90;

/** Linear amplitude -> dBFS. Exported for tests. */
export function toDb(linear: number): number {
  return linear > 0 ? 20 * Math.log10(linear) : SILENT_DB;
}

// --- Stage: adaptive noise floor ---------------------------------------------
// A room is never silent. With a fan running, a fixed-width window (reference
// minus a constant) put steady broadband noise squarely in the MIDDLE of the row
// -- measured at a mean of 9.5px out of 16 with 13 direction reversals a second,
// which is the churning-ocean look. Nothing about that window knew where the
// room's floor actually was.
//
// The first attempt at fixing it tracked the MINIMUM recent level and started
// the window a fixed 7dB above it. That was worse, and instructively so: a
// moderate fan and a loud fan produced identical output, because the floor
// tracked the noise's troughs while the reference tracked its peaks, so the
// window straddled the fan's own fluctuation and expanded it to full scale. Any
// window defined by "quietest recent" and "loudest recent" does this — with only
// noise present, noise IS the signal.
//
// So the floor is the background's MEAN, and how far above it the display starts
// is derived from the background's own VARIABILITY. A 10ms peak of broadband
// noise swings several dB either side of its mean; placing the window bottom a
// few deviations up puts virtually all of that swing below the display, where it
// cannot drive the bars. Because the margin is measured rather than assumed, it
// adapts to a quiet mic, a loud mic, a hard drive, or an air conditioner without
// any per-device constant.
//
// Both statistics update only while the gate says this is background. Otherwise
// a long sentence drags the floor up behind it and the waveform shrinks
// mid-sentence.
// The floor tracks the LOW end of recent audio with an asymmetric follower:
// it rises slowly and falls quickly, so it settles onto the quiet background
// while speech, which is only ever above it, can pull it up barely at all.
//
// Asymmetric rather than a gated mean, because a gated mean can LATCH. Freezing
// the floor whenever the gate is open sounds right, but if the floor ever learns
// from speech first, the window bottom ends up above the voice, the gate never
// opens, and the row is dead permanently — measured as every scenario flat at
// 3px, including loud speech. A follower that always falls cannot get stuck: any
// quiet moment pulls it back down within a few hundred ms.
const NOISE_RISE_TAU_MS = 8000;
const NOISE_FALL_TAU_MS = 300;
// Only audio within this band of the floor is allowed to teach it. This is the
// load-bearing detail. Conditioning the floor on the GATE instead created a
// positive feedback loop: speech inflated the spread, which raised the window
// bottom, which drove the level to zero, which closed the gate, which is exactly
// what permitted the floor to rise -- so the bottom climbed until the row died,
// measured as loud speech over a fan rendering completely flat. Judging against
// raw dB relative to the floor itself depends on nothing downstream, so no such
// loop can form.
// Tight on purpose. At 6dB the band still admitted the dips between syllables,
// so the measured spread crept upward through a long sentence, lifting the
// window bottom and visibly shrinking the waveform mid-speech.
const NOISE_TRACK_BAND_DB = 4;
// Acquisition. Seeding the floor from a single sample cannot work in either
// direction: seeding AT the first sample declares an opening word to be the
// background and the row renders flat, while seeding a fixed amount below it
// leaves the floor stranded, unable to catch up to a fan for many seconds.
// Seeding from the MINIMUM over a short opening window resolves both, because
// speech contains gaps between syllables while steady noise does not — so the
// minimum lands on the background either way.
const NOISE_SEED_MS = 400;

// Bounded so a burst of noise can never inflate the margin without limit.
const NOISE_DEV_MAX_DB = 4;
// Spread of the background around that floor, learned only from samples close to
// it so speech never inflates it.
const NOISE_DEV_TAU_MS = 900;
// How many deviations of the background's own spread the display clears. A 10ms
// peak of broadband noise swings several dB either side of its floor; putting
// the window bottom a few deviations up leaves virtually all of that swing below
// the display, where it cannot drive the bars. Measured rather than assumed, so
// it adapts to a quiet mic, a loud mic, a hard drive or an air conditioner with
// no per-device constant.
// Tuned to leave a sliver of the background's upper tail just inside the window,
// so a fan reads as subtle slow movement rather than a dead flat line. Raising it
// flattens the idle state completely; lowering it lets noise churn.
const NOISE_DEV_K = 2.15;
const NOISE_MIN_MARGIN_DB = 5;
// The window can never collapse, however close floor and reference get --
// otherwise a quiet steady room expands its own noise to full scale.
const MIN_WINDOW_DB = 18;

// --- Stage: speech reference (top of the window) ------------------------------
// Slow both ways: rise slow enough that one loud syllable never visibly shrinks
// the row, fall slower still so the row does not creep upward through a quiet
// passage. Headroom above it stops a steady sound clipping flat against the top.
const REF_RISE_TAU_MS = 900;
const REF_FALL_TAU_MS = 5000;
const HEADROOM_DB = 6;
// The reference only learns from audio that is clearly inside the window, well
// clear of the noise floor. Letting it track anything above the floor is how a
// fan taught the reference its own level and normalized itself to full scale.
// Judged in dB against the window BOTTOM, not against the normalized level: the
// level depends on the top of the window, which depends on the reference, so
// gating the reference on it is circular. During the feedback failure above that
// circularity is why the reference stayed frozen at the fan's own level.
const REF_ACTIVE_MARGIN_DB = 6;

/** dB mapped into an explicit [lo, hi] window -> 0..1. Exported for tests. */
export function mapWindow(db: number, loDb: number, hiDb: number): number {
  const span = Math.max(hiDb - loDb, 1);
  const n = (db - loDb) / span;
  return Math.min(Math.max(n, 0), 1);
}

// --- Stage: hysteresis gate ---------------------------------------------------
// A dead zone just above the noise floor. Its control signal is a SLOW average
// of the level, not the level itself: that is what supplies the hysteresis. A
// gate driven directly by an instantaneous value flickers open and shut frame to
// frame whenever the input hovers near the threshold, which is exactly the
// appear/disappear churn fan noise produces. Because the control moves over
// ~150ms, the gate cannot toggle faster than that no matter how noisy the input.
// Asymmetric: opens quickly so a consonant is not held back, closes slowly so
// it cannot chatter and so it rides through the gaps between syllables. A
// symmetric 150ms control cost ~50ms on every onset for no stability benefit.
const GATE_OPEN_TAU_MS = 45;
const GATE_CLOSE_TAU_MS = 400;
const GATE_LO = 0.06;
const GATE_HI = 0.30;
// The gate never closes completely. Background noise should read as calm, slow
// movement rather than a dead flat line -- it just must not churn.
const GATE_FLOOR = 0.16;

function smoothstep(edge0: number, edge1: number, x: number): number {
  const t = Math.min(Math.max((x - edge0) / (edge1 - edge0), 0), 1);
  return t * t * (3 - 2 * t);
}

// --- Stage: adaptive (one-euro) smoothing -------------------------------------
// Fixed smoothing forces a choice between calm noise and responsive speech: slow
// enough to settle a fan is slow enough to blunt a consonant. This is a one-euro
// filter, whose cutoff rises with the rate of change — so small rapid wobble
// (noise) is filtered hard, while a genuine transient (a consonant, a raised
// voice) passes almost untouched. That is the "strong smoothing of small changes,
// no visible latency on real ones" requirement in a single filter.
//
// The rate coefficient is asymmetric: onsets are allowed through much faster
// than decays, which is what makes speech feel immediate while the tail settles
// smoothly instead of snapping.
const MIN_CUTOFF_HZ = 0.9;
const BETA_UP = 6;
const BETA_DOWN = 1.4;
const DERIV_CUTOFF_HZ = 1;

/** Standard one-euro alpha for a given cutoff and timestep. */
function alphaFor(cutoffHz: number, dtSec: number): number {
  const tau = 1 / (2 * Math.PI * cutoffHz);
  return 1 / (1 + tau / dtSec);
}

// --- Stage: carrier / detail --------------------------------------------------
// A slow carrier follows loudness and sets the SIZE of the waveform; the fast
// deviation from it carries texture and is amplified so that texture is legible.
// The detail term is scaled by the gate as well as by the carrier, because
// amplifying deviation is precisely the wrong thing to do to background noise —
// ungated, this stage was a large part of why a fan looked like surf.
// --- Stage: idle presence -----------------------------------------------------
// With the window bottom placed above the background, a fan renders as an
// exactly flat line, which reads as the visualizer being switched off. This adds
// back a small amount of movement from the band BETWEEN the noise floor and the
// window bottom -- real audio that is otherwise discarded -- smoothed hard so
// only the slow mechanical drift of the room survives and none of the fast
// churn. Bounded to well under a pixel of travel.
const IDLE_TAU_MS = 350;
const IDLE_AMPL = 0.055;

const CARRIER_TAU_MS = 260;
const DETAIL_GAIN = 4.3;
// Gentle high-end compression. Low and mid levels retain their existing
// response, while a full-scale value lands at 88% so normal loud speech does
// not pin the bars against their maximum height.
const OUTPUT_SOFT_KNEE = 0.12;

// --- Per-bar motion ----------------------------------------------------------
// Critically damped: settles into its target without ever overshooting, since a
// bouncing spring would invent motion the audio never contained. Solved
// analytically, which is exact and unconditionally stable — a plain Euler spring
// at this stiffness goes unstable on a dropped frame.
const SPRING_OMEGA = 42;

// Older audio sits slightly lower, so energy visibly decays as it travels
// outward and the row keeps its centre-weighted silhouette. A function of
// position only, so unlike the shimmer it replaced it cannot fight the signal.
const EDGE_TAPER = 0.72;
const barTaper: number[] = Array.from({ length: BARS }, (_, i) => {
  const d = Math.abs(i - HALF_SPAN) / HALF_SPAN;
  return 1 - d * (1 - EDGE_TAPER);
});
const barAge: number[] = Array.from(
  { length: BARS },
  (_, i) => Math.abs(i - HALF_SPAN) * AGE_STEP_MS
);

export interface PillVisualizer {
  /** Feed one `audio-envelope` batch: peaks at a fixed ENVELOPE_WINDOW_MS cadence. */
  pushEnvelope(samples: ArrayLike<number>): void;
  /** Advance one animation frame; returns the 12 bar heights in px. */
  step(nowMs: number): number[];
  /** Drop every stage's state back to silence. */
  reset(): void;
}

export function createPillVisualizer(): PillVisualizer {
  const ring = new Float32Array(RING_LEN);
  // Absolute count of samples ever written — the ring is addressed modulo its
  // length, so positions stay monotonic and the playhead can be expressed in
  // plain milliseconds.
  let written = 0;
  let refDb = 0;
  let noiseFloorDb = SILENT_DB;
  let noiseDevDb = 3;
  let seedMinDb = Infinity;
  let seedMs = 0;
  let refSeeded = false;
  let justSeeded = false;
  let smoothed = 0;
  let smoothedDeriv = 0;
  let gateCtl = 0;
  let idleSlow = 0;
  let carrier = 0;
  let readHeadMs = 0;
  let playheadSeeded = false;
  let lastFrameAt = 0;

  const barLevel = new Float32Array(BARS);
  const barVel = new Float32Array(BARS);
  let heights: number[] = Array(BARS).fill(BAR_MIN_H);

  const writeHeadMs = () => written * ENVELOPE_WINDOW_MS;

  // Value at an absolute time, interpolated between the two samples either side
  // so the read is continuous rather than quantized to the sample grid.
  const sampleAt = (tMs: number): number => {
    const pos = tMs / ENVELOPE_WINDOW_MS;
    const oldest = Math.max(0, written - RING_LEN + 1);
    if (pos <= oldest) return ring[oldest % RING_LEN] ?? 0;
    const newest = written - 1;
    if (pos >= newest) return newest >= 0 ? ring[newest % RING_LEN] : 0;
    const i0 = Math.floor(pos);
    const frac = pos - i0;
    const a = ring[i0 % RING_LEN];
    const b = ring[(i0 + 1) % RING_LEN];
    return a + (b - a) * frac;
  };

  return {
    pushEnvelope(samples: ArrayLike<number>) {
      const dtSec = ENVELOPE_WINDOW_MS / 1000;
      const kNoiseUp = 1 - Math.exp(-ENVELOPE_WINDOW_MS / NOISE_RISE_TAU_MS);
      const kNoiseDown = 1 - Math.exp(-ENVELOPE_WINDOW_MS / NOISE_FALL_TAU_MS);
      const kNoiseDev = 1 - Math.exp(-ENVELOPE_WINDOW_MS / NOISE_DEV_TAU_MS);
      const kGateOpen = 1 - Math.exp(-ENVELOPE_WINDOW_MS / GATE_OPEN_TAU_MS);
      const kGateClose = 1 - Math.exp(-ENVELOPE_WINDOW_MS / GATE_CLOSE_TAU_MS);
      const kCarrier = 1 - Math.exp(-ENVELOPE_WINDOW_MS / CARRIER_TAU_MS);
      const kIdle = 1 - Math.exp(-ENVELOPE_WINDOW_MS / IDLE_TAU_MS);

      for (let i = 0; i < samples.length; i++) {
        const db = toDb(samples[i]);

        const acquiring = seedMs < NOISE_SEED_MS;
        if (acquiring) {
          seedMs += ENVELOPE_WINDOW_MS;
          if (db < seedMinDb) seedMinDb = db;
          noiseFloorDb = seedMinDb;
        }
        // Background floor: falls fast, rises very slowly. The fast fall is what
        // keeps it honest during speech — every gap between syllables pulls it
        // straight back down to the background, so continuous talking cannot
        // walk it upward. The spread is learned only from the quiet population
        // (audio within NOISE_TRACK_BAND_DB of the floor) so speech never
        // inflates the margin.
        const nearFloor = db < noiseFloorDb + NOISE_TRACK_BAND_DB;
        if (!acquiring) {
          noiseFloorDb += (db - noiseFloorDb) * (db < noiseFloorDb ? kNoiseDown : kNoiseUp);
        }
        if (nearFloor) {
          noiseDevDb += (Math.abs(db - noiseFloorDb) - noiseDevDb) * kNoiseDev;
          if (noiseDevDb > NOISE_DEV_MAX_DB) noiseDevDb = NOISE_DEV_MAX_DB;
        }

        const loDb =
          noiseFloorDb + Math.max(NOISE_MIN_MARGIN_DB, NOISE_DEV_K * noiseDevDb);
        // Until useful audio appears, use a floor-relative window. The old
        // fixed -35dBFS seed silently assumed a fairly hot microphone. Quieter
        // post-gain inputs never seeded `refDb`, leaving the window top at
        // +6dBFS and compressing the entire row into an apparent flat line.
        const hiDb = refSeeded
          ? Math.max(refDb + HEADROOM_DB, loDb + MIN_WINDOW_DB)
          : loDb + MIN_WINDOW_DB;
        const level = mapWindow(db, loDb, hiDb);

        // Reference only learns from audio clearly above the window bottom.
        // Seeding is relative to the learned room floor, not absolute dBFS, so
        // the same signal shape works on quiet and loud microphone chains.
        if (db > loDb + REF_ACTIVE_MARGIN_DB) {
          if (!refSeeded) {
            // Preserve the acquisition window when the first qualifying sample
            // only barely clears the floor. Seeding directly to that sample
            // makes later syllables spend seconds raising the reference, which
            // visibly shrinks a continuous utterance as calibration catches up.
            refDb = Math.max(db, loDb + MIN_WINDOW_DB - HEADROOM_DB);
            refSeeded = true;
            justSeeded = true;
          } else {
            const tau = db > refDb ? REF_RISE_TAU_MS : REF_FALL_TAU_MS;
            refDb += (db - refDb) * (1 - Math.exp(-ENVELOPE_WINDOW_MS / tau));
          }
        }

        // One-euro: cutoff tracks how fast the signal is really moving, so
        // wobble is filtered hard and transients are not.
        const rawDeriv = (level - smoothed) / dtSec;
        const aD = alphaFor(DERIV_CUTOFF_HZ, dtSec);
        smoothedDeriv += aD * (rawDeriv - smoothedDeriv);
        const beta = smoothedDeriv >= 0 ? BETA_UP : BETA_DOWN;
        const cutoff = MIN_CUTOFF_HZ + beta * Math.abs(smoothedDeriv);

        if (justSeeded) {
          // Start every stage AT the first real value instead of ramping up
          // from silence, which otherwise dominates the opening of a recording.
          smoothed = level;
          carrier = level;
          gateCtl = level;
          justSeeded = false;
        } else {
          smoothed += alphaFor(cutoff, dtSec) * (level - smoothed);
          gateCtl += (level - gateCtl) * (level > gateCtl ? kGateOpen : kGateClose);
          carrier += (smoothed - carrier) * kCarrier;
        }

        // Hysteresis gate, never fully closed: background stays calm but alive.
        const gate = GATE_FLOOR + (1 - GATE_FLOOR) * smoothstep(GATE_LO, GATE_HI, gateCtl);

        // Slow drift of the background itself, from the band below the window.
        idleSlow += (mapWindow(db, noiseFloorDb, loDb) - idleSlow) * kIdle;

        const detail = (smoothed - carrier) * DETAIL_GAIN * carrier * gate;
        const value = (carrier + detail) * gate + idleSlow * IDLE_AMPL;
        const bounded = Math.min(Math.max(value, 0), 1);
        ring[written % RING_LEN] = bounded * (1 - OUTPUT_SOFT_KNEE * bounded);
        written++;
      }
    },

    step(nowMs: number): number[] {
      if (!lastFrameAt) lastFrameAt = nowMs;
      const dt = Math.min(nowMs - lastFrameAt, 50);
      lastFrameAt = nowMs;

      // Advance with real time first — this is the whole point, and it happens
      // whether or not a packet arrived this frame.
      readHeadMs += dt;

      const desired = writeHeadMs() - TARGET_LATENCY_MS;
      if (!playheadSeeded) {
        // First frame with data: jump straight to the target rather than easing
        // up from zero, which would replay the buffer from its start. Tracked
        // with an explicit flag — comparing readHeadMs against dt also matched
        // on the second frame, re-seeding the playhead after it had started.
        playheadSeeded = true;
        readHeadMs = Math.max(0, desired);
      } else {
        readHeadMs += (desired - readHeadMs) * RESYNC_RATE;
      }
      // Never read ahead of what has actually arrived; starving on a stalled
      // packet should hold the last shape, not invent one.
      if (readHeadMs > writeHeadMs()) readHeadMs = writeHeadMs();

      const dts = dt / 1000;
      const decay = Math.exp(-SPRING_OMEGA * dts);
      const next: number[] = new Array(BARS);
      for (let i = 0; i < BARS; i++) {
        const target = sampleAt(readHeadMs - barAge[i]) * barTaper[i];
        const y0 = barLevel[i] - target;
        const b = barVel[i] + SPRING_OMEGA * y0;
        const y = (y0 + b * dts) * decay;
        barLevel[i] = target + y;
        barVel[i] = (b - SPRING_OMEGA * (y0 + b * dts)) * decay;
        const level = Math.min(Math.max(barLevel[i], 0), 1);
        // Deliberately NOT snapped to device pixels. Widths and gaps are (see
        // snap()/barW/barGap in PillApp.svelte) because a fractional bar WIDTH
        // rasterizes unevenly between neighbours. Height has no such problem,
        // and snapping it quantized a 13px range into 13 steps, so quiet speech
        // climbed the row in visible stairs.
        next[i] = BAR_MIN_H + level * (BAR_MAX_H - BAR_MIN_H);
      }
      heights = next;
      return heights;
    },

    reset() {
      ring.fill(0);
      written = 0;
      refDb = 0;
      noiseFloorDb = SILENT_DB;
      noiseDevDb = 3;
      seedMinDb = Infinity;
      seedMs = 0;
      refSeeded = false;
      justSeeded = false;
      smoothed = 0;
      smoothedDeriv = 0;
      gateCtl = 0;
      idleSlow = 0;
      carrier = 0;
      readHeadMs = 0;
      playheadSeeded = false;
      lastFrameAt = 0;
      barLevel.fill(0);
      barVel.fill(0);
      heights = Array(BARS).fill(BAR_MIN_H);
    },
  };
}
