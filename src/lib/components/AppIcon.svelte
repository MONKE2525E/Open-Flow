<script lang="ts" module>
  import { invoke } from '../tauri';
  import { normalizeExe } from '../appMappings';
  import { createIconCache } from '../iconCache';

  // Module-level so every AppIcon instance across the app shares one fetch
  // per exe — Contexts' picker/rows and AppMappingsEditor's rows all resolve
  // the same handful of icons without re-invoking the native extractor.
  const iconCache = createIconCache();

  function loadIcon(exe: string): Promise<string | null> {
    if (exe.startsWith('?::')) return Promise.resolve(null);
    const key = normalizeExe(exe);
    return iconCache.get(key, () => invoke<string | null>('get_app_icon', { exe: key }));
  }
</script>

<script lang="ts">
  let { exe, label = '', size = 18 }: { exe: string; label?: string; size?: number } = $props();

  let dataUri = $state<string | null>(null);

  $effect(() => {
    dataUri = null;
    let cancelled = false;
    loadIcon(exe).then((uri) => {
      if (!cancelled) dataUri = uri;
    });
    return () => {
      cancelled = true;
    };
  });

  const initial = $derived(exe.startsWith('?::') ? '?' : (label || exe || '?').trim().slice(0, 1).toUpperCase());
</script>

{#if dataUri}
  <img class="app-icon" src={dataUri} alt="" style="width: {size}px; height: {size}px;" />
{:else}
  <span class="app-icon app-icon-fallback" style="width: {size}px; height: {size}px; font-size: {Math.round(size * 0.55)}px;" aria-hidden="true">{initial}</span>
{/if}

<style>
  .app-icon {
    border-radius: 6px;
    flex: 0 0 auto;
    display: block;
    object-fit: contain;
  }
  .app-icon-fallback {
    display: grid;
    place-items: center;
    background: var(--accent-soft);
    color: var(--accent-ink);
    font-weight: 600;
  }
</style>
