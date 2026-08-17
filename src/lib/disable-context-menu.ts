/**
 * Tauri's WebView2 has a browser context menu by default. Verenu is a native
 * app, so that menu is never useful; in particular, the idle pill webview is
 * kept alive and can still receive a right-click during a click-through
 * transition.
 *
 * Listen during bubbling so feature-specific contextmenu handlers get first
 * refusal. Contexts.svelte uses preventDefault() for its color picker, which
 * must remain available.
 */
export function disableBrowserContextMenu(): () => void {
  const handler = (event: MouseEvent) => {
    if (!event.defaultPrevented) {
      event.preventDefault();
    }
  };
  document.addEventListener('contextmenu', handler);

  let removed = false;
  return () => {
    if (removed) return;
    removed = true;
    document.removeEventListener('contextmenu', handler);
  };
}
