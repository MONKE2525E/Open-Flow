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
export function disableBrowserContextMenu() {
  document.addEventListener('contextmenu', (event) => {
    if (!event.defaultPrevented) {
      event.preventDefault();
    }
  });
}
