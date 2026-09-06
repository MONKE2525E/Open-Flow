import { expect, it, vi } from 'vitest';
import { createIconCache } from './iconCache';

it('deduplicates pending icons and retains negative results', async () => {
  const cache = createIconCache();
  const load = vi.fn(async () => { throw new Error('missing'); });
  const first = cache.get('a', load);
  expect(cache.get('a', load)).toBe(first);
  expect(await first).toBeNull();
  expect(await cache.get('a', load)).toBeNull();
  expect(load).toHaveBeenCalledTimes(1);
});

it('evicts least recently used entries under the count and string budgets', async () => {
  const cache = createIconCache(2, 12);
  const load = vi.fn(async () => 'abc');
  const first = cache.get('a', load);
  await first;
  await cache.get('b', load);
  expect(cache.get('a', load)).toBe(first);
  await cache.get('c', load);
  expect(cache.get('a', load)).toBe(first);
  await cache.get('b', load);
  expect(load).toHaveBeenCalledTimes(4);
  const large = vi.fn(async () => 'too large');
  await cache.get('large', large);
  await cache.get('large', large);
  expect(large).toHaveBeenCalledTimes(2);
});

it('ignores late completion of evicted entries', async () => {
  const cache = createIconCache(1, 8);
  let resolve!: (value: string) => void;
  const old = cache.get('old', () => new Promise<string>((done) => { resolve = done; }));
  await Promise.resolve();
  const current = cache.get('current', async () => 'ok');
  await current;
  resolve('a very large old icon');
  await old;
  expect(cache.get('current', async () => 'unexpected')).toBe(current);
});
