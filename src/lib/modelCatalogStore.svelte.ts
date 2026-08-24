import { invoke } from './tauri';
import { saveSetting, type ProviderId } from './settings';
import { modelId } from './components/settings/models';

/** How long a good list stays fresh before the next Settings visit refetches. */
export const CATALOG_TTL_MS = 24 * 60 * 60 * 1000;
/** How long to wait after a failure before trying that provider again. */
export const CATALOG_RETRY_MS = 15 * 60 * 1000;
/**
 * Two misses only mean something if they're separated in time. Without this,
 * Settings-open plus a key-save seconds apart would satisfy the deprecation
 * threshold during a single provider incident.
 */
export const MISS_INTERVAL_MS = 15 * 60 * 1000;
/**
 * A 200 that drops more than this share of the ids we knew about is treated as
 * a degraded response: recorded as a success, but it advances no miss counters.
 * A provider serving a truncated list is exactly what the two-miss rule exists
 * to survive, and it doesn't arrive as an HTTP error.
 */
const SUSPICIOUS_SHRINK = 0.5;

export type MissCounter = { count: number; lastCountedAt: number };

export type ProviderCache = {
  ids: string[];
  /** Every id ever seen in a successful list — separates "retired" from "never existed". */
  everSeen: string[];
  /** 0 means no successful fetch has ever landed for this provider. */
  lastSuccessAt: number;
  lastAttemptAt: number;
  /** null means the last attempt succeeded. */
  lastError: string | null;
  /** Keyed by canonical `provider/model` id. */
  missing: Record<string, MissCounter>;
};

export type ModelCatalogCache = Partial<Record<ProviderId, ProviderCache>>;

const emptyProviderCache = (): ProviderCache => ({
  ids: [],
  everSeen: [],
  lastSuccessAt: 0,
  lastAttemptAt: 0,
  lastError: null,
  missing: {},
});

/**
 * Defensive read of a persisted blob. The Rust validator rejects malformed
 * saves, so anything wrong here came from a hand-edited settings file — drop
 * the bad parts rather than throwing on Settings open.
 */
export function mergeCatalogCache(raw: unknown): ModelCatalogCache {
  const merged: ModelCatalogCache = {};
  if (!raw || typeof raw !== 'object') return merged;

  for (const [provider, value] of Object.entries(raw as Record<string, unknown>)) {
    if (!['groq', 'openai', 'google', 'assemblyai', 'local'].includes(provider)) continue;
    if (!value || typeof value !== 'object') continue;
    const entry = value as Record<string, unknown>;

    const strings = (key: string): string[] =>
      Array.isArray(entry[key])
        ? (entry[key] as unknown[]).filter((v): v is string => typeof v === 'string')
        : [];
    const timestamp = (key: string): number => {
      const value = entry[key];
      return typeof value === 'number' && Number.isFinite(value) && value >= 0 ? value : 0;
    };

    const missing: Record<string, MissCounter> = {};
    if (entry.missing && typeof entry.missing === 'object') {
      for (const [id, counter] of Object.entries(entry.missing as Record<string, unknown>)) {
        if (!counter || typeof counter !== 'object') continue;
        const { count, lastCountedAt } = counter as Record<string, unknown>;
        if (typeof count !== 'number' || !Number.isFinite(count) || count < 0) continue;
        missing[id] = {
          count: Math.floor(count),
          lastCountedAt:
            typeof lastCountedAt === 'number' && Number.isFinite(lastCountedAt) && lastCountedAt >= 0
              ? lastCountedAt
              : 0,
        };
      }
    }

    merged[provider as ProviderId] = {
      ids: strings('ids'),
      everSeen: strings('everSeen'),
      lastSuccessAt: timestamp('lastSuccessAt'),
      lastAttemptAt: timestamp('lastAttemptAt'),
      lastError: typeof entry.lastError === 'string' ? entry.lastError : null,
      missing,
    };
  }

  return merged;
}

/** True when a provider's list is complete enough to reason about absence. */
export function isTrustworthy(cache: ProviderCache | undefined): boolean {
  return !!cache && cache.lastError === null && cache.lastSuccessAt > 0;
}

export function shouldRefresh(cache: ProviderCache | undefined, now: number): boolean {
  if (!cache) return true;
  // A failed attempt earns a short cooldown, not a full day of silence — and a
  // successful one must not be retried every time Settings opens.
  if (cache.lastError !== null) return now - cache.lastAttemptAt >= CATALOG_RETRY_MS;
  return now - cache.lastSuccessAt >= CATALOG_TTL_MS;
}

/**
 * Folds a successful fetch into a provider's cache. Pure, so the counting rules
 * are testable without a clock or IPC.
 *
 * `tracked` is the set of canonical ids worth counting misses for — the user's
 * selections plus the curated catalog. Anything else is pruned so the counter
 * map can't grow without bound.
 */
export function applySuccess(
  previous: ProviderCache | undefined,
  provider: ProviderId,
  ids: string[],
  tracked: string[],
  now: number,
): ProviderCache {
  const before = previous ?? emptyProviderCache();
  const live = new Set(ids);
  const everSeen = Array.from(new Set([...before.everSeen, ...ids]));

  const knownBefore = before.ids.length;
  const stillPresent = before.ids.filter((id) => live.has(id)).length;
  const degraded =
    knownBefore > 0 && (ids.length === 0 || stillPresent < knownBefore * SUSPICIOUS_SHRINK);

  const missing: Record<string, MissCounter> = {};
  for (const id of tracked) {
    const parsed = id.startsWith(`${provider}/`) ? id.slice(provider.length + 1) : null;
    if (parsed === null) continue;
    if (live.has(parsed)) continue; // Reappeared, or never gone — counter resets.
    const prior = before.missing[id];
    if (degraded) {
      // Carry the counter forward untouched: this response can't be trusted to
      // prove absence, but it also shouldn't erase what we'd already observed.
      if (prior) missing[id] = prior;
      continue;
    }
    if (prior && now - prior.lastCountedAt < MISS_INTERVAL_MS) {
      missing[id] = prior;
      continue;
    }
    missing[id] = { count: (prior?.count ?? 0) + 1, lastCountedAt: now };
  }

  return {
    ids,
    everSeen,
    lastSuccessAt: now,
    lastAttemptAt: now,
    lastError: null,
    missing,
  };
}

/** A failed fetch records the attempt and touches nothing else. */
export function applyFailure(
  previous: ProviderCache | undefined,
  error: string,
  now: number,
): ProviderCache {
  const before = previous ?? emptyProviderCache();
  return { ...before, lastAttemptAt: now, lastError: error };
}

// ── Store ──────────────────────────────────────────────────────────────────

export const modelCatalogStore = $state<{ cache: ModelCatalogCache }>({ cache: {} });

/** One in-flight request per provider, so overlapping triggers coalesce. */
const inFlight = new Map<ProviderId, Promise<void>>();
/** Serializes persistence: a per-provider refresh must never clobber another's entry. */
let writeChain: Promise<unknown> = Promise.resolve();

export function hydrateCatalogCache(raw: unknown) {
  modelCatalogStore.cache = mergeCatalogCache(raw);
}

function persist() {
  // Always writes the whole object, never one provider's slice.
  const snapshot = JSON.parse(JSON.stringify(modelCatalogStore.cache)) as ModelCatalogCache;
  writeChain = writeChain
    .then(() => saveSetting('provider_model_cache', snapshot))
    .catch((error) => console.warn('Failed to persist model catalog cache', error));
  return writeChain;
}

export function refreshCatalog(provider: ProviderId, tracked: string[], now = Date.now()) {
  const pending = inFlight.get(provider);
  if (pending) return pending;

  const request = invoke<string[]>('list_provider_models', { provider })
    .then((ids) => {
      modelCatalogStore.cache[provider] = applySuccess(
        modelCatalogStore.cache[provider],
        provider,
        ids,
        tracked,
        now,
      );
    })
    .catch((error) => {
      modelCatalogStore.cache[provider] = applyFailure(
        modelCatalogStore.cache[provider],
        String(error),
        now,
      );
    })
    .then(() => {
      persist();
    })
    .finally(() => {
      inFlight.delete(provider);
    });

  inFlight.set(provider, request);
  return request;
}

/** Refreshes every keyed provider whose cache has gone stale. */
export function refreshStaleCatalogs(
  apiKeyStatus: Record<ProviderId, boolean>,
  tracked: string[],
  now = Date.now(),
) {
  const providers: ProviderId[] = ['groq', 'openai', 'google', 'assemblyai'];
  return Promise.all(
    providers
      .filter((provider) => apiKeyStatus[provider])
      .filter((provider) => shouldRefresh(modelCatalogStore.cache[provider], now))
      .map((provider) => refreshCatalog(provider, tracked, now)),
  );
}

/** Canonical ids worth tracking misses for: the user's picks plus the catalog. */
export function trackedIds(selected: string[], curated: { provider: ProviderId; id: string }[]) {
  return Array.from(
    new Set([...selected, ...curated.map((entry) => modelId(entry.provider, entry.id))]),
  );
}
