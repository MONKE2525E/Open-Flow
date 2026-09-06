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
  let needsContrastBackdrop = $state(false);

  /**
   * Detect the narrow case where a light icon would disappear on paper.
   * Looking at the pixels keeps colorful and deliberately dark icons alone,
   * even when they happen to have transparent corners.
   */
  function assessIconContrast(event: Event) {
    const image = event.currentTarget as HTMLImageElement;
    const canvas = document.createElement('canvas');
    canvas.width = 32;
    canvas.height = 32;
    const context = canvas.getContext('2d', { willReadFrequently: true });
    if (!context) return;

    try {
      context.clearRect(0, 0, canvas.width, canvas.height);
      context.drawImage(image, 0, 0, canvas.width, canvas.height);
      const pixels = context.getImageData(0, 0, canvas.width, canvas.height).data;
      let transparentPixels = 0;
      let visiblePixels = 0;
      let lightPixels = 0;
      let darkPixels = 0;
      let colorfulPixels = 0;

      for (let index = 0; index < pixels.length; index += 4) {
        const alpha = pixels[index + 3] / 255;
        if (alpha < 0.12) {
          transparentPixels += 1;
          continue;
        }

        visiblePixels += alpha;
        const red = pixels[index] / 255;
        const green = pixels[index + 1] / 255;
        const blue = pixels[index + 2] / 255;
        const luminance = 0.2126 * red + 0.7152 * green + 0.0722 * blue;
        const saturation = Math.max(red, green, blue) - Math.min(red, green, blue);

        if (luminance > 0.78) lightPixels += alpha;
        if (luminance < 0.28) darkPixels += alpha;
        if (saturation > 0.24 && luminance < 0.9) colorfulPixels += alpha;
      }

      const totalPixels = pixels.length / 4;
      const transparentRatio = transparentPixels / totalPixels;
      const lightRatio = visiblePixels > 0 ? lightPixels / visiblePixels : 0;
      const darkRatio = visiblePixels > 0 ? darkPixels / visiblePixels : 0;
      const colorfulRatio = visiblePixels > 0 ? colorfulPixels / visiblePixels : 0;

      needsContrastBackdrop = transparentRatio > 0.12
        && lightRatio > 0.72
        && darkRatio < 0.12
        && colorfulRatio < 0.2;
    } catch {
      // Icon contrast is cosmetic. Keep the normal icon if the webview cannot
      // inspect the image (for example, if canvas access is unavailable).
      needsContrastBackdrop = false;
    }
  }

  $effect(() => {
    dataUri = null;
    needsContrastBackdrop = false;
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
  <span
    class="app-icon-frame"
    class:has-contrast-backdrop={needsContrastBackdrop}
    style="width: {size}px; height: {size}px;"
  >
    <img class="app-icon" src={dataUri} alt="" onload={assessIconContrast} style="width: {size}px; height: {size}px;" />
  </span>
{:else}
  <span class="app-icon app-icon-fallback" style="width: {size}px; height: {size}px; font-size: {Math.round(size * 0.55)}px;" aria-hidden="true">{initial}</span>
{/if}

<style>
  .app-icon-frame {
    position: relative;
    display: grid;
    place-items: center;
    flex: 0 0 auto;
  }
  .app-icon-frame::before {
    content: '';
    position: absolute;
    inset: 1px;
    border-radius: 50%;
    background: #000;
    opacity: 0;
    pointer-events: none;
  }
  .app-icon-frame.has-contrast-backdrop::before { opacity: 1; }
  :global(:root[data-theme="dark"]) .app-icon-frame.has-contrast-backdrop::before { opacity: 0; }
  .app-icon {
    position: relative;
    z-index: 1;
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
