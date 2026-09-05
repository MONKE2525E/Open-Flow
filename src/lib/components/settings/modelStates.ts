import type { ProviderId } from '../../settings';
import {
  isTrustworthy,
  MISS_INTERVAL_MS,
  type ModelCatalogCache,
} from '../../modelCatalogStore.svelte';
import { fitsHardware, type Hardware } from './modelPresets';
import {
  CATALOG,
  catalogEntry,
  catalogFor,
  modelDisplayLabel,
  modelId,
  providerDisplayLabel,
  qualifiedModelLabel,
  splitModelId,
  type CatalogEntry,
  type ModelTag,
  type TaskType,
} from './models';

/**
 * - `ready`        usable right now
 * - `needs-setup`  known-good, but missing a key, a download, or the RAM
 * - `unavailable`  was offered once, and a trustworthy list no longer has it
 * - `not-found`    never seen in any successful list — a typo or a stale pick
 * - `unverified`   the provider returned it, but Verenu knows nothing about it
 */
export type ModelState = 'ready' | 'needs-setup' | 'unavailable' | 'not-found' | 'unverified';

export type Remedy = 'add-key' | 'download' | 'none';

export type ModelRow = {
  provider: ProviderId;
  id: string;
  /** Canonical `provider/model`. */
  key: string;
  label: string;
  tags: ModelTag[];
  state: ModelState;
  /** Short human reason, shown on the row. Empty for a plain ready model. */
  note: string;
  remedy: Remedy;
  sizeMb?: number;
};

export type LocalModelInfo = {
  id: string;
  name?: string;
  description?: string;
  is_downloaded?: boolean;
  is_downloading?: boolean;
  is_recommended?: boolean;
  size_mb?: number;
  quantization?: string;
  prompt_family?: string;
};

/**
 * Everything the picker needs to manage local models in place — downloading,
 * deleting and (for cleanup) the shared runtime and per-model prompts. These
 * used to live in a separate "Local models" section that listed the same
 * models again under a different UI.
 */
export type LocalControls = {
  models: LocalModelInfo[];
  /** False on hardware where on-device models haven't been validated. */
  supported: boolean;
  downloadProgress: Record<string, { progress?: number } | undefined>;
  downloadStage: Record<string, string | undefined>;
  onDownload: (id: string) => void;
  onCancel: (id: string) => void;
  onDelete: (id: string) => void;
  /** Cleanup only: the local LLM runtime every on-device model needs. */
  runtime?: {
    info?: { installed: boolean; backend?: string; approx_download_mb?: number; is_downloading?: boolean };
    progress?: { stage?: string; progress?: number };
    onDownload: () => void;
    onCancel: () => void;
    onDelete: () => void;
  };
};

export type PickerContext = {
  task: TaskType;
  apiKeyStatus: Record<ProviderId, boolean>;
  cache: ModelCatalogCache;
  localModels: LocalModelInfo[];
  hardware: Hardware;
};

/** Providers whose absence from a list means something. Local has no list. */
const LISTED_PROVIDERS: ProviderId[] = ['groq', 'openai', 'google', 'assemblyai'];
const GOOGLE_DEDICATED_TRANSCRIBER = 'gemini-3.5-transcribe';

function row(entry: CatalogEntry, state: ModelState, note = '', remedy: Remedy = 'none'): ModelRow {
  return {
    provider: entry.provider,
    id: entry.id,
    key: modelId(entry.provider, entry.id),
    label: entry.label,
    tags: entry.tags,
    state,
    note,
    remedy,
  };
}

function localRow(entry: CatalogEntry, ctx: PickerContext): ModelRow {
  const info = ctx.localModels.find((model) => model.id === entry.id);
  const sizeMb = info?.size_mb;
  const base = { ...row(entry, 'ready'), sizeMb };

  if (!info || info.is_downloaded !== true) {
    return { ...base, state: 'needs-setup', note: 'Not downloaded', remedy: 'download' };
  }
  if (sizeMb !== undefined && !fitsHardware(ctx.hardware, [sizeMb])) {
    return { ...base, state: 'needs-setup', note: 'Needs more memory than this machine has' };
  }
  return { ...base, note: 'Installed' };
}

function cloudRow(entry: CatalogEntry, ctx: PickerContext): ModelRow {
  if (!ctx.apiKeyStatus[entry.provider]) {
    return row(entry, 'needs-setup', 'No API key', 'add-key');
  }

  // Interactions API models are not returned by Google's generateContent
  // model listing. Keep the dedicated transcriber selectable when the key is
  // present instead of misclassifying it as retired.
  if (entry.provider === 'google' && entry.id === GOOGLE_DEDICATED_TRANSCRIBER) {
    return row(entry, 'ready');
  }

  const cache = ctx.cache[entry.provider];
  if (!isTrustworthy(cache)) {
    // A key alone isn't proof the model exists, but refusing to offer anything
    // until a fetch lands would leave a fresh install with an empty picker.
    return row(entry, 'ready', `Not verified against ${providerDisplayLabel(entry.provider)} yet`);
  }
  if (cache!.ids.includes(entry.id)) return row(entry, 'ready');

  return row(entry, 'unavailable', `No longer offered by ${providerDisplayLabel(entry.provider)}`);
}

/**
 * Every curated model for the task, classified.
 *
 * A model a trustworthy live list no longer contains is dropped outright
 * rather than shown as retired — the picker is a list of what you can pick,
 * and a retired model is not one of those. `keep` pins ids that must stay
 * visible anyway: your current selections, which need somewhere to display
 * their state and be replaced from.
 */
export function curatedRows(ctx: PickerContext, keep: string[] = []): ModelRow[] {
  const pinned = new Set(keep);
  return catalogFor(ctx.task)
    .map((entry) => (entry.provider === 'local' ? localRow(entry, ctx) : cloudRow(entry, ctx)))
    .filter((row) => row.state !== 'unavailable' || pinned.has(row.key));
}

/**
 * Ids a provider returned that aren't in the catalog.
 *
 * ponytail: Groq's and OpenAI's `/v1/models` return every model flat with no
 * modality field, so the task split here is a name heuristic. Curated rows
 * carry real `tasks` metadata; this tail is best-effort, which is why it ships
 * behind Advanced and is labelled Unverified.
 */
export function unverifiedRows(ctx: PickerContext): ModelRow[] {
  const transcriptionish = /whisper|transcribe|speech|[-/]stt\b/i;
  // Neither task can use these, and a provider list is full of them.
  const otherModality =
    /\b(tts|text-to-speech|embed|embedding|rerank|moderation|guard|safeguard|image|vision|video|veo|imagen|lyria|dall-e|sora|robotics|orpheus|playai)\b/i;
  const rows: ModelRow[] = [];

  for (const provider of LISTED_PROVIDERS) {
    const cache = ctx.cache[provider];
    if (!isTrustworthy(cache)) continue;
    for (const id of cache!.ids) {
      if (catalogEntry(provider, id)) continue;
      if (otherModality.test(id)) continue;
      const looksTranscription = transcriptionish.test(id);
      if (ctx.task === 'transcription' ? !looksTranscription : looksTranscription) continue;
      rows.push({
        provider,
        id,
        key: modelId(provider, id),
        label: id,
        tags: [],
        state: 'unverified',
        note: 'Not verified for this task',
        remedy: 'none',
      });
    }
  }

  return rows;
}

/**
 * The row for whatever is selected right now, even when it is in neither the
 * catalog nor the live list. Without it a deprecated or hand-typed id has
 * nowhere to show its state and no way to be replaced in place.
 */
export function rowForSelection(selectedId: string, ctx: PickerContext): ModelRow | null {
  const parsed = splitModelId(selectedId);
  if (!parsed) return null;

  const entry = catalogEntry(parsed.provider, parsed.model);
  if (entry) {
    return entry.provider === 'local' ? localRow(entry, ctx) : cloudRow(entry, ctx);
  }

  const cache = ctx.cache[parsed.provider];
  const base: ModelRow = {
    provider: parsed.provider,
    id: parsed.model,
    key: selectedId,
    label: modelDisplayLabel(parsed.provider, parsed.model),
    tags: [],
    state: 'unverified',
    note: 'Custom model',
    remedy: 'none',
  };

  if (parsed.provider === 'local' || !isTrustworthy(cache)) return base;
  if (cache!.ids.includes(parsed.model)) return base;

  return cache!.everSeen.includes(parsed.model)
    ? { ...base, state: 'unavailable', note: `No longer offered by ${providerDisplayLabel(parsed.provider)}` }
    : { ...base, state: 'not-found', note: `Not in ${providerDisplayLabel(parsed.provider)}'s model list` };
}

// ── Deprecation ────────────────────────────────────────────────────────────

export type UnavailableReason = 'deprecated' | 'not-found';

export type UnavailableSelection = {
  id: string;
  provider: ProviderId;
  model: string;
  reason: UnavailableReason;
  /** 'default', or the model's 0-based index in the fallback chain. */
  position: 'default' | number;
  suggestion: string | null;
};

/** Two misses only count once they're far enough apart to mean anything. */
function confirmedMissing(cache: ModelCatalogCache[ProviderId], id: string): boolean {
  const counter = cache?.missing[id];
  return !!counter && counter.count >= 2;
}

/**
 * Selected models a trustworthy provider list no longer contains.
 *
 * Silence is the safe answer: an unreachable provider, a missing key, a 429, or
 * a partial Google pagination all leave `lastError` set, and none of them mean
 * a model went away.
 */
export function unavailableSelections(
  task: TaskType,
  selected: string[],
  ctx: PickerContext,
): UnavailableSelection[] {
  const found: UnavailableSelection[] = [];

  selected.forEach((id, index) => {
    const parsed = splitModelId(id);
    if (!parsed) return;
    if (!LISTED_PROVIDERS.includes(parsed.provider)) return;
    // AssemblyAI's list is a static const on the Rust side, so a model missing
    // from it is a Verenu bug, not a provider deprecation.
    if (parsed.provider === 'assemblyai') return;
    if (parsed.provider === 'google' && parsed.model === GOOGLE_DEDICATED_TRANSCRIBER) return;

    const cache = ctx.cache[parsed.provider];
    if (!isTrustworthy(cache)) return;
    if (cache!.ids.includes(parsed.model)) return;
    if (!confirmedMissing(cache, id)) return;

    found.push({
      id,
      provider: parsed.provider,
      model: parsed.model,
      reason: cache!.everSeen.includes(parsed.model) ? 'deprecated' : 'not-found',
      position: index === 0 ? 'default' : index - 1,
      suggestion: suggestReplacement(task, parsed.provider, parsed.model, ctx),
    });
  });

  return found;
}

/**
 * Closest still-usable sibling from the same provider: most shared tags wins,
 * premium beats standard on a tie, then catalog order.
 */
export function suggestReplacement(
  task: TaskType,
  provider: ProviderId,
  model: string,
  ctx: PickerContext,
): string | null {
  const dead = catalogEntry(provider, model);
  if (!dead) return null;

  const ranked = curatedRows({ ...ctx, task })
    .filter((row) => row.state === 'ready' && row.provider === provider && row.id !== model)
    .map((row) => {
      const entry = catalogEntry(provider, row.id)!;
      return {
        entry,
        shared: entry.tags.filter((tag) => dead.tags.includes(tag)).length,
        tierRank: entry.tier === 'premium' ? 0 : 1,
        order: CATALOG.indexOf(entry),
      };
    })
    .sort((a, b) => b.shared - a.shared || a.tierRank - b.tierRank || a.order - b.order);

  return ranked.length ? modelId(provider, ranked[0].entry.id) : null;
}

/** First entry in the chain that can actually run right now. */
export function firstRunnable(chain: string[], ctx: PickerContext): string | null {
  for (const id of chain) {
    const row = rowForSelection(id, ctx);
    if (row?.state === 'ready') return id;
  }
  return null;
}

/**
 * One sentence per unavailable selection, built from the real chain — whether a
 * fallback picks up the work, and whether the task is about to fail outright.
 */
export function unavailableMessages(
  task: TaskType,
  defaultModel: string,
  fallbacks: string[],
  ctx: PickerContext,
): string[] {
  const chain = [defaultModel, ...fallbacks];
  const taskName = task === 'transcription' ? 'Transcription' : 'Clean-up';

  return unavailableSelections(task, chain, ctx).map((entry) => {
    const name = modelDisplayLabel(entry.provider, entry.model);
    const provider = providerDisplayLabel(entry.provider);

    if (entry.reason === 'not-found') {
      return `Verenu can't find ${name} in ${provider}'s model list. Check the id or pick another model.`;
    }
    if (entry.position !== 'default') {
      return `Fallback #${entry.position + 1} ${name} is no longer offered by ${provider} and will be skipped.`;
    }

    const next = firstRunnable(fallbacks, ctx);
    if (!next) {
      return `${name} is no longer offered by ${provider} and no fallback is usable — ${taskName.toLowerCase()} will fail until you pick another model.`;
    }
    const parsed = splitModelId(next)!;
    const nextName = qualifiedModelLabel(parsed.provider, parsed.model);
    return `${name} is no longer offered by ${provider}. ${taskName} will use ${nextName} instead.`;
  });
}

export { MISS_INTERVAL_MS, isTrustworthy };
