import { describe, expect, it } from 'vitest';
import {
  applyFailure,
  applySuccess,
  CATALOG_RETRY_MS,
  CATALOG_TTL_MS,
  isTrustworthy,
  mergeCatalogCache,
  MISS_INTERVAL_MS,
  shouldRefresh,
  type ProviderCache,
} from './modelCatalogStore.svelte';

const T0 = 1_700_000_000_000;

function cache(overrides: Partial<ProviderCache> = {}): ProviderCache {
  return {
    ids: ['whisper-large-v3', 'whisper-large-v3-turbo'],
    everSeen: ['whisper-large-v3', 'whisper-large-v3-turbo'],
    lastSuccessAt: T0,
    lastAttemptAt: T0,
    lastError: null,
    missing: {},
    ...overrides,
  };
}

describe('shouldRefresh', () => {
  it('refreshes a provider that has never been fetched', () => {
    expect(shouldRefresh(undefined, T0)).toBe(true);
  });

  it('leaves a fresh successful list alone until the TTL', () => {
    expect(shouldRefresh(cache(), T0 + CATALOG_TTL_MS - 1)).toBe(false);
    expect(shouldRefresh(cache(), T0 + CATALOG_TTL_MS)).toBe(true);
  });

  it('retries a failure on the short cooldown, not the full day', () => {
    const failed = cache({ lastError: 'offline' });
    expect(shouldRefresh(failed, T0 + CATALOG_RETRY_MS - 1)).toBe(false);
    expect(shouldRefresh(failed, T0 + CATALOG_RETRY_MS)).toBe(true);
  });
});

describe('applyFailure', () => {
  it('preserves ids, everSeen and miss counters', () => {
    const before = cache({ missing: { 'groq/gone': { count: 1, lastCountedAt: T0 } } });
    const after = applyFailure(before, 'timeout', T0 + 1000);
    expect(after.ids).toEqual(before.ids);
    expect(after.everSeen).toEqual(before.everSeen);
    expect(after.missing).toEqual(before.missing);
    expect(after.lastSuccessAt).toBe(T0);
    expect(after.lastAttemptAt).toBe(T0 + 1000);
    expect(after.lastError).toBe('timeout');
  });

  it('makes the provider untrustworthy without erasing its list', () => {
    expect(isTrustworthy(cache())).toBe(true);
    expect(isTrustworthy(applyFailure(cache(), 'offline', T0))).toBe(false);
    expect(isTrustworthy(cache({ lastSuccessAt: 0 }))).toBe(false);
  });
});

describe('applySuccess', () => {
  const tracked = ['groq/whisper-large-v3', 'groq/whisper-large-v3-turbo', 'openai/gpt-4o'];

  it('replaces the id list and unions everSeen', () => {
    const after = applySuccess(cache(), 'groq', ['whisper-large-v3', 'new-model'], tracked, T0 + 1);
    expect(after.ids).toEqual(['whisper-large-v3', 'new-model']);
    expect(after.everSeen).toEqual(['whisper-large-v3', 'whisper-large-v3-turbo', 'new-model']);
    expect(after.lastError).toBeNull();
  });

  it('ignores tracked ids belonging to other providers', () => {
    const after = applySuccess(cache(), 'groq', ['whisper-large-v3'], tracked, T0 + 1);
    expect(Object.keys(after.missing)).toEqual(['groq/whisper-large-v3-turbo']);
  });

  it('counts a miss once, then not again inside the interval', () => {
    const first = applySuccess(cache(), 'groq', ['whisper-large-v3'], tracked, T0 + 1);
    expect(first.missing['groq/whisper-large-v3-turbo'].count).toBe(1);

    // Settings-open and a key-save seconds apart must not reach the threshold.
    const soon = applySuccess(first, 'groq', ['whisper-large-v3'], tracked, T0 + 2000);
    expect(soon.missing['groq/whisper-large-v3-turbo'].count).toBe(1);

    const later = applySuccess(
      first,
      'groq',
      ['whisper-large-v3'],
      tracked,
      T0 + MISS_INTERVAL_MS + 1,
    );
    expect(later.missing['groq/whisper-large-v3-turbo'].count).toBe(2);
  });

  it('resets the counter the moment a model reappears', () => {
    const missed = applySuccess(cache(), 'groq', ['whisper-large-v3'], tracked, T0 + 1);
    const back = applySuccess(
      missed,
      'groq',
      ['whisper-large-v3', 'whisper-large-v3-turbo'],
      tracked,
      T0 + MISS_INTERVAL_MS + 1,
    );
    expect(back.missing['groq/whisper-large-v3-turbo']).toBeUndefined();
  });

  it('prunes counters for ids that are no longer tracked', () => {
    const missed = applySuccess(cache(), 'groq', ['whisper-large-v3'], tracked, T0 + 1);
    const pruned = applySuccess(
      missed,
      'groq',
      ['whisper-large-v3'],
      ['groq/whisper-large-v3'],
      T0 + MISS_INTERVAL_MS + 1,
    );
    expect(pruned.missing).toEqual({});
  });

  it('does not advance counters when the list shrinks suspiciously', () => {
    const wide = cache({ ids: ['a', 'b', 'c', 'd'], everSeen: ['a', 'b', 'c', 'd'] });
    const trackedWide = ['groq/a', 'groq/b', 'groq/c', 'groq/d'];
    const after = applySuccess(wide, 'groq', ['a'], trackedWide, T0 + 1);
    expect(after.missing).toEqual({});
    // Still recorded as a success — the response was a 200, just not trustworthy
    // enough to prove three models vanished at once.
    expect(after.lastSuccessAt).toBe(T0 + 1);
    expect(after.ids).toEqual(['a']);
  });

  it('carries existing counters through a degraded response untouched', () => {
    const wide = cache({ ids: ['a', 'b', 'c', 'd'], everSeen: ['a', 'b', 'c', 'd'] });
    const trackedWide = ['groq/a', 'groq/b', 'groq/c', 'groq/d'];
    const missed = applySuccess(wide, 'groq', ['a', 'b', 'c'], trackedWide, T0 + 1);
    expect(missed.missing['groq/d'].count).toBe(1);

    const degraded = applySuccess(missed, 'groq', ['a'], trackedWide, T0 + MISS_INTERVAL_MS + 1);
    expect(degraded.missing['groq/d'].count).toBe(1);
  });

  it('treats an empty list as degraded when we knew about models before', () => {
    const after = applySuccess(cache(), 'groq', [], tracked, T0 + 1);
    expect(after.missing).toEqual({});
  });

  it('counts normally on a first fetch, with nothing known before', () => {
    const after = applySuccess(undefined, 'groq', ['whisper-large-v3'], tracked, T0);
    expect(after.missing['groq/whisper-large-v3-turbo'].count).toBe(1);
  });
});

describe('mergeCatalogCache', () => {
  it('returns an empty cache for junk', () => {
    expect(mergeCatalogCache(null)).toEqual({});
    expect(mergeCatalogCache('nope')).toEqual({});
    expect(mergeCatalogCache(42)).toEqual({});
  });

  it('drops unknown providers and malformed fields instead of throwing', () => {
    const merged = mergeCatalogCache({
      'not-a-provider': { ids: ['x'] },
      groq: {
        ids: ['whisper-large-v3', 7, null],
        everSeen: 'nope',
        lastSuccessAt: -5,
        lastAttemptAt: Number.NaN,
        lastError: 12,
        missing: {
          'groq/a': { count: 2, lastCountedAt: T0 },
          'groq/b': { count: 'two', lastCountedAt: T0 },
          'groq/c': 'nope',
        },
      },
    });

    expect(Object.keys(merged)).toEqual(['groq']);
    expect(merged.groq?.ids).toEqual(['whisper-large-v3']);
    expect(merged.groq?.everSeen).toEqual([]);
    expect(merged.groq?.lastSuccessAt).toBe(0);
    expect(merged.groq?.lastAttemptAt).toBe(0);
    expect(merged.groq?.lastError).toBeNull();
    expect(merged.groq?.missing).toEqual({ 'groq/a': { count: 2, lastCountedAt: T0 } });
  });
});
