// Auto-generated model recommendations for the Models tab.
//
// The simple (non-Advanced) Models view shows a short list of ready-to-use
// "presets" instead of the full per-provider model accordion. Which presets we
// offer is derived purely from (a) which API keys the user has and (b) the
// machine's RAM. This module is deliberately pure/side-effect-free apart from
// `getHardware()` — everything else is a plain function so it's easy to reason
// about and test.

import { invoke } from '../../tauri';
import type { ProviderId } from '../../settings';
import { modelId, recommendedModels, type TaskType, type UiProviderId } from './models';

export type Hardware = {
  totalRamMb: number;
  freeRamMb: number;
  gpus: { vramTotalMb: number; vramUsedMb: number }[];
  /**
   * True when the backend read failed (or an older backend lacks the command).
   * In that case we assume a capable machine and offer every preset rather than
   * hiding local options behind a phantom "not enough RAM".
   */
  unknown: boolean;
};

export type RequiredLocalModel = { task: TaskType; id: string; sizeMb: number };

export type PresetTarget = {
  transcriptionDefaultModel: string;
  cleanupEnabled: boolean;
  cleanupDefaultModel: string | null;
  dualTranscription: boolean;
  transcriptionFallbacks: string[];
  cleanupFallbacks: string[];
  /** Local model ids that must be on disk before this preset can run. */
  requiredLocalModels: RequiredLocalModel[];
};

export type Preset = {
  id: string;
  /** 'preset' = selectable config; 'add-key' = an inert prompt to add a key. */
  kind: 'preset' | 'add-key';
  name: string;
  tagline: string;
  /** 0 = maximum accuracy … 1 = maximum efficiency; positions the bar marker. */
  position: number;
  offline: boolean;
  target: PresetTarget | null;
};

/** What the live settings currently look like — compared against preset targets. */
export type ActiveConfig = {
  transcriptionDefaultModel: string;
  cleanupEnabled: boolean;
  cleanupDefaultModel: string;
  dualTranscription: boolean;
  transcriptionFallbacks: string[];
  cleanupFallbacks: string[];
};

// ── Hardware ──────────────────────────────────────────────────────────────

// A generous stand-in used when the RAM read fails — "assume capable" so the
// picker never wrongly hides local presets. 16 GB total / 12 GB free clears
// every threshold below.
const CAPABLE_DEFAULT: Hardware = { totalRamMb: 16384, freeRamMb: 12288, gpus: [], unknown: true };

type RawHardware = {
  total_ram_mb?: number;
  free_ram_mb?: number;
  gpus?: { vram_total_mb?: number; vram_used_mb?: number }[];
};

export async function getHardware(): Promise<Hardware> {
  try {
    const raw = await invoke<RawHardware>('get_hardware_capabilities');
    // total_ram_mb === 0 is the backend's "read failed" sentinel — treat it the
    // same as a thrown error and fall back to the capable default.
    if (!raw || !raw.total_ram_mb) return CAPABLE_DEFAULT;
    return {
      totalRamMb: raw.total_ram_mb,
      freeRamMb: raw.free_ram_mb ?? 0,
      gpus: (raw.gpus ?? []).map((gpu) => ({
        vramTotalMb: gpu.vram_total_mb ?? 0,
        vramUsedMb: gpu.vram_used_mb ?? 0,
      })),
      unknown: false,
    };
  } catch {
    return CAPABLE_DEFAULT;
  }
}

// ── RAM viability ─────────────────────────────────────────────────────────

// Total (not free) RAM is the capacity signal for *which presets to offer* —
// idle models get unloaded and the user can close other apps, so gating on the
// current free figure would wrongly hide "Most accurate" on a big machine that
// happens to be busy right now. Runtime memory pressure is handled separately
// on the backend. Headroom covers the OS, the app, and the target editor app.
const OS_HEADROOM_MB = 3000;
const RAM_FACTOR = 1.3;

function ramNeededMb(sizesMb: number[]): number {
  const total = sizesMb.reduce((sum, size) => sum + size, 0);
  return Math.round(total * RAM_FACTOR) + OS_HEADROOM_MB;
}

export function fitsHardware(hardware: Hardware, sizesMb: number[]): boolean {
  return hardware.unknown || hardware.totalRamMb >= ramNeededMb(sizesMb);
}

// ── Local model catalog (mirrors the Rust size_mb catalogs) ───────────────
// Only the ids used by presets. Gemma is intentionally excluded — its curated
// GGUFs are flagged not-recommended in the backend catalog (tokenizer issues).

const STT_PARAKEET_V3 = { id: 'parakeet-v3', sizeMb: 456 };
const STT_MOONSHINE_TINY = { id: 'moonshine-tiny', sizeMb: 31 };
const STT_COHERE = { id: 'cohere', sizeMb: 1708 };

const LLM_QWEN_1_5B = { id: 'qwen2.5-1.5b-instruct', sizeMb: 1080 };
const LLM_QWEN_3B = { id: 'qwen2.5-3b-instruct', sizeMb: 1960 };
const LLM_QWEN_7B = { id: 'qwen2.5-7b-instruct', sizeMb: 4680 };

type LocalTier = {
  key: string;
  name: string;
  tagline: string;
  position: number;
  stt: { id: string; sizeMb: number };
  llm: { id: string; sizeMb: number } | null;
};

// Ordered most-efficient → most-accurate.
const LOCAL_TIERS: LocalTier[] = [
  {
    key: 'fastest',
    name: 'Fastest',
    tagline: 'Fast and light. Runs entirely on your device, private and offline.',
    position: 0.8,
    stt: STT_PARAKEET_V3,
    llm: LLM_QWEN_1_5B,
  },
  {
    key: 'balanced',
    name: 'Balanced',
    tagline: 'A good mix of speed and accuracy. Runs entirely on your device, private and offline.',
    position: 0.5,
    stt: STT_PARAKEET_V3,
    llm: LLM_QWEN_3B,
  },
  {
    key: 'accurate',
    name: 'Most accurate',
    tagline: 'Highest accuracy, heavier to run. Runs entirely on your device, private and offline.',
    position: 0.2,
    stt: STT_COHERE,
    llm: LLM_QWEN_7B,
  },
];

function localTierSizes(tier: LocalTier): number[] {
  return tier.llm ? [tier.stt.sizeMb, tier.llm.sizeMb] : [tier.stt.sizeMb];
}

function localTierTarget(tier: LocalTier): PresetTarget {
  const required: RequiredLocalModel[] = [{ task: 'transcription', id: tier.stt.id, sizeMb: tier.stt.sizeMb }];
  if (tier.llm) required.push({ task: 'cleanup', id: tier.llm.id, sizeMb: tier.llm.sizeMb });
  return {
    transcriptionDefaultModel: modelId('local', tier.stt.id),
    cleanupEnabled: tier.llm !== null,
    cleanupDefaultModel: tier.llm ? modelId('local', tier.llm.id) : null,
    dualTranscription: false,
    transcriptionFallbacks: [],
    cleanupFallbacks: [],
    requiredLocalModels: required,
  };
}

function localTierPreset(tier: LocalTier, idPrefix: string): Preset {
  return {
    id: `${idPrefix}-${tier.key}`,
    kind: 'preset',
    name: tier.name,
    tagline: tier.tagline,
    position: tier.position,
    offline: true,
    target: localTierTarget(tier),
  };
}

// The floor: transcription with no cleanup, for machines too small for a local
// LLM (or with no key to run cloud cleanup). Uses the lightest STT that fits.
function transcriptionOnlyPreset(hardware: Hardware): Preset {
  const stt = fitsHardware(hardware, [STT_PARAKEET_V3.sizeMb]) ? STT_PARAKEET_V3 : STT_MOONSHINE_TINY;
  return {
    id: 'local-transcription-only',
    kind: 'preset',
    name: 'Transcription only',
    tagline: 'Local AI for speech-to-text. Runs entirely on your device, private and offline.',
    position: 0.9,
    offline: true,
    target: {
      transcriptionDefaultModel: modelId('local', stt.id),
      cleanupEnabled: false,
      cleanupDefaultModel: null,
      dualTranscription: false,
      transcriptionFallbacks: [],
      cleanupFallbacks: [],
      requiredLocalModels: [{ task: 'transcription', id: stt.id, sizeMb: stt.sizeMb }],
    },
  };
}

// ── Cloud provider selection ──────────────────────────────────────────────

type KeyStatus = Record<ProviderId, boolean>;

function firstAvailable(status: KeyStatus, order: UiProviderId[]): UiProviderId | undefined {
  return order.find((provider) => status[provider]);
}

const CLOUD_PROVIDERS: UiProviderId[] = ['groq', 'openai', 'google', 'assemblyai'];

function hasCloudKey(status: KeyStatus): boolean {
  return CLOUD_PROVIDERS.some((provider) => status[provider]);
}

function transcriptionModelFor(provider: UiProviderId, tier: 'standard' | 'premium'): string {
  // Safe: every cloud provider has a recommendedModels.transcription entry.
  return modelId(provider, recommendedModels.transcription[provider]![tier]);
}

function cleanupModelFor(provider: UiProviderId | undefined, tier: 'standard' | 'premium'): string | null {
  if (!provider) return null;
  const entry = recommendedModels.cleanup[provider];
  return entry ? modelId(provider, entry[tier]) : null;
}

// ── Public: build the preset list ─────────────────────────────────────────

export function buildPresets(status: KeyStatus, hardware: Hardware, localSupported: boolean): Preset[] {
  if (hasCloudKey(status)) {
    return buildCloudPresets(status, hardware, localSupported);
  }
  if (localSupported) {
    return buildLocalOnlyPresets(hardware);
  }
  // No keys and local inference unavailable (e.g. Intel Mac) — the only path
  // forward is adding an API key.
  return [addKeyPreset()];
}

const TRANSCRIPTION_ORDER: UiProviderId[] = ['groq', 'openai', 'google', 'assemblyai'];
const CLEANUP_FAST_ORDER: UiProviderId[] = ['groq', 'openai', 'google'];
const CLEANUP_ACCURATE_ORDER: UiProviderId[] = ['openai', 'google', 'groq'];

// Every other keyed transcription provider's model at the same tier, in
// preference order — so a preset degrades across providers, not just across the
// primary provider's own models. Excludes the primary (already the default).
function transcriptionFallbacksFor(status: KeyStatus, primary: UiProviderId, tier: 'standard' | 'premium'): string[] {
  return TRANSCRIPTION_ORDER
    .filter((provider) => status[provider] && provider !== primary)
    .map((provider) => transcriptionModelFor(provider, tier));
}

function cleanupFallbacksFor(
  status: KeyStatus,
  primary: UiProviderId | undefined,
  order: UiProviderId[],
  tier: 'standard' | 'premium',
): string[] {
  return order
    .filter((provider) => status[provider] && provider !== primary && !!recommendedModels.cleanup[provider])
    .map((provider) => cleanupModelFor(provider, tier))
    .filter((model): model is string => model !== null);
}

function buildCloudPresets(status: KeyStatus, hardware: Hardware, localSupported: boolean): Preset[] {
  const tp = firstAvailable(status, ['groq', 'openai', 'google', 'assemblyai'])!;
  // Prefer Groq for speed on fast cleanup; prefer OpenAI/Gemini quality on the
  // accurate tier. May be undefined when the only key is AssemblyAI (no cleanup
  // provider), in which case those presets fall back to transcription-only.
  const fastCleanup = firstAvailable(status, ['groq', 'openai', 'google']);
  const accurateCleanup = firstAvailable(status, ['openai', 'google', 'groq']);

  const presets: Preset[] = [
    {
      id: 'cloud-fastest',
      kind: 'preset',
      name: 'Fastest',
      tagline: 'Quick and light. A slight accuracy tradeoff for speed.',
      position: 0.88,
      offline: false,
      target: cloudTarget({
        transcriptionDefaultModel: transcriptionModelFor(tp, 'standard'),
        transcriptionFallbacks: transcriptionFallbacksFor(status, tp, 'standard'),
        cleanupDefaultModel: cleanupModelFor(fastCleanup, 'standard'),
        cleanupFallbacks: cleanupFallbacksFor(status, fastCleanup, CLEANUP_FAST_ORDER, 'standard'),
        dualTranscription: false,
      }),
    },
    {
      id: 'cloud-balanced',
      kind: 'preset',
      name: 'Balanced',
      tagline: 'A solid mix of speed and accuracy for everyday use.',
      position: 0.5,
      offline: false,
      target: cloudTarget({
        transcriptionDefaultModel: transcriptionModelFor(tp, 'premium'),
        transcriptionFallbacks: transcriptionFallbacksFor(status, tp, 'premium'),
        cleanupDefaultModel: cleanupModelFor(fastCleanup, 'premium'),
        cleanupFallbacks: cleanupFallbacksFor(status, fastCleanup, CLEANUP_FAST_ORDER, 'premium'),
        dualTranscription: false,
      }),
    },
    {
      id: 'cloud-accurate',
      kind: 'preset',
      name: 'Most accurate',
      tagline: 'Highest accuracy. Compares two transcription models before cleanup.',
      position: 0.12,
      offline: false,
      target: cloudTarget({
        transcriptionDefaultModel: transcriptionModelFor(tp, 'premium'),
        // The primary's standard model comes first (it feeds the dual-model
        // compare), then every other keyed provider's premium model.
        transcriptionFallbacks: [
          transcriptionModelFor(tp, 'standard'),
          ...transcriptionFallbacksFor(status, tp, 'premium'),
        ],
        cleanupDefaultModel: cleanupModelFor(accurateCleanup, 'premium'),
        cleanupFallbacks: cleanupFallbacksFor(status, accurateCleanup, CLEANUP_ACCURATE_ORDER, 'premium'),
        dualTranscription: true,
      }),
    },
  ];

  // A single "go fully private" option, using the strongest local setup this
  // machine can run.
  if (localSupported) {
    const local = bestViableLocalTier(hardware);
    if (local) {
      presets.push({
        id: 'cloud-private',
        kind: 'preset',
        name: 'Local AI',
        tagline: 'Runs entirely on your device. Private and offline. Nothing leaves your machine.',
        position: local.position,
        offline: true,
        target: localTierTarget(local),
      });
    }
  }

  return presets;
}

function cloudTarget(opts: {
  transcriptionDefaultModel: string;
  transcriptionFallbacks: string[];
  cleanupDefaultModel: string | null;
  cleanupFallbacks: string[];
  dualTranscription: boolean;
}): PresetTarget {
  const cleanupEnabled = opts.cleanupDefaultModel !== null;
  return {
    transcriptionDefaultModel: opts.transcriptionDefaultModel,
    cleanupEnabled,
    cleanupDefaultModel: opts.cleanupDefaultModel,
    dualTranscription: opts.dualTranscription,
    transcriptionFallbacks: opts.transcriptionFallbacks,
    cleanupFallbacks: cleanupEnabled ? opts.cleanupFallbacks : [],
    requiredLocalModels: [],
  };
}

function buildLocalOnlyPresets(hardware: Hardware): Preset[] {
  const viable = LOCAL_TIERS.filter((tier) => fitsHardware(hardware, localTierSizes(tier)));
  if (viable.length === 0) {
    return [transcriptionOnlyPreset(hardware)];
  }
  return viable.map((tier) => localTierPreset(tier, 'local'));
}

// Most-accurate tier that still fits, for the cloud "Local AI" card. Falls back
// through lighter tiers; null only if even the lightest local config won't fit.
function bestViableLocalTier(hardware: Hardware): LocalTier | null {
  const viable = LOCAL_TIERS.filter((tier) => fitsHardware(hardware, localTierSizes(tier)));
  if (viable.length === 0) return null;
  // LOCAL_TIERS is ordered efficient → accurate; last viable is the most accurate.
  return viable[viable.length - 1];
}

function addKeyPreset(): Preset {
  return {
    id: 'add-key',
    kind: 'add-key',
    name: 'Add an API key',
    tagline: 'Add a Groq, OpenAI, or Gemini key to start dictating. Groq is free and recommended.',
    position: 0.5,
    offline: false,
    target: null,
  };
}

// ── Public: match live settings to a preset ───────────────────────────────

function sameFallbacks(a: string[], b: string[]): boolean {
  if (a.length !== b.length) return false;
  return a.every((value, index) => value === b[index]);
}

export function matchActivePreset(presets: Preset[], current: ActiveConfig): string | null {
  for (const preset of presets) {
    const target = preset.target;
    if (!target) continue;
    if (target.transcriptionDefaultModel !== current.transcriptionDefaultModel) continue;
    if (target.dualTranscription !== current.dualTranscription) continue;
    if (target.cleanupEnabled !== current.cleanupEnabled) continue;
    if (target.cleanupEnabled && target.cleanupDefaultModel !== current.cleanupDefaultModel) continue;
    if (!sameFallbacks(target.transcriptionFallbacks, current.transcriptionFallbacks)) continue;
    if (target.cleanupEnabled && !sameFallbacks(target.cleanupFallbacks, current.cleanupFallbacks)) continue;
    return preset.id;
  }
  return null;
}
