/** Bound the data-URI strings retained after icon components unmount.
 * Eviction only removes the cache's reference; mounted icons keep their image.
 */
export function createIconCache(maxEntries = 128, maxBytes = 4 * 1024 * 1024) {
  const entries = new Map<string, { promise: Promise<string | null>; bytes: number }>();
  let bytes = 0;
  function trim() {
    while (entries.size > maxEntries || bytes > maxBytes) {
      const key = entries.keys().next().value;
      if (key === undefined) break;
      bytes -= entries.get(key)!.bytes;
      entries.delete(key);
    }
  }
  return {
    get(key: string, load: () => Promise<string | null>): Promise<string | null> {
      const cached = entries.get(key);
      if (cached) {
        entries.delete(key);
        entries.set(key, cached);
        return cached.promise;
      }
      const entry = { promise: Promise.resolve(null) as Promise<string | null>, bytes: 0 };
      entry.promise = Promise.resolve().then(load).catch(() => null).then((value) => {
        if (entries.get(key) === entry) {
          // UTF-16 is a conservative budget even on engines using Latin-1.
          entry.bytes = (value?.length ?? 0) * 2;
          bytes += entry.bytes;
          trim();
        }
        return value;
      });
      entries.set(key, entry);
      trim();
      return entry.promise;
    },
  };
}
