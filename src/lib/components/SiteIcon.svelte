<script lang="ts" module>
  import { invoke } from '../tauri';

  /**
   * Favicons are resolved by the backend (`get_site_icon`) rather than loaded
   * straight from a remote `<img src>`: the app's CSP only permits `self`,
   * `asset:`, `blob:` and `data:` image sources, so the old direct-to-Google
   * URL was silently blocked and every site fell back to the globe glyph.
   *
   * The backend returns a data URI and disk-caches the result (including
   * "there is no icon") per hostname. This module-level map adds in-process
   * dedup on top, so a sidebar full of rows referencing the same site issues
   * one call, and remounting a row costs nothing.
   */
  const siteIconCache = new Map<string, Promise<string | null>>();

  /** Mirrors `normalize_favicon_host` in src-tauri/src/system/icons.rs. */
  function normalizeHost(input: string): string {
    const trimmed = input.trim().toLowerCase();
    if (!trimmed) return '';
    const withoutScheme = trimmed.split('://').pop() ?? trimmed;
    const host = withoutScheme.split(/[/?#]/)[0]?.split('@').pop()?.split(':')[0] ?? '';
    return host.replace(/^\.+|\.+$/g, '');
  }

  function loadSiteIcon(domain: string): Promise<string | null> {
    const host = normalizeHost(domain);
    if (!host) return Promise.resolve(null);
    let pending = siteIconCache.get(host);
    if (!pending) {
      // A rejected/failed lookup stays cached as `null` — no retry storm on a
      // site that simply has no reachable icon.
      pending = invoke<string | null>('get_site_icon', { domain: host }).catch(() => null);
      siteIconCache.set(host, pending);
    }
    return pending;
  }
</script>

<script lang="ts">
  let { domain, size = 16 }: { domain: string; size?: number } = $props();

  let dataUri = $state<string | null>(null);

  // The row never waits on this: it renders with the fallback glyph and swaps
  // in the real icon whenever the (usually already cached) lookup resolves.
  $effect(() => {
    dataUri = null;
    let cancelled = false;
    loadSiteIcon(domain).then((uri) => {
      if (!cancelled) dataUri = uri;
    });
    return () => {
      cancelled = true;
    };
  });
</script>

{#if dataUri}
  <!-- onerror is a belt-and-braces net: the backend validates PNG bytes, but
       if the webview still fails to decode, fall back to the glyph. -->
  <img
    class="site-icon"
    src={dataUri}
    alt=""
    style="width: {size}px; height: {size}px;"
    onerror={() => (dataUri = null)}
  />
{:else}
  <span class="site-icon site-icon-fallback" style="width: {size}px; height: {size}px;" aria-hidden="true">
    <svg width={Math.round(size * 0.7)} height={Math.round(size * 0.7)} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"><circle cx="12" cy="12" r="9"/><path d="M3 12h18M12 3c2.2 2.5 3.3 5.5 3.3 9s-1.1 6.5-3.3 9c-2.2-2.5-3.3-5.5-3.3-9S9.8 5.5 12 3Z"/></svg>
  </span>
{/if}

<style>
  .site-icon {
    border-radius: 6px;
    flex: 0 0 auto;
    display: block;
    object-fit: contain;
  }
  .site-icon-fallback {
    display: grid;
    place-items: center;
    background: var(--bg-elev);
    color: var(--ink-mute);
    border: 1px solid var(--line);
  }
</style>
