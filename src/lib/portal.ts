/**
 * Moves a node to `document.body` for as long as the component lives.
 *
 * Modals need this because `position: fixed` is measured against the nearest
 * ancestor with a `transform`, `filter` or `backdrop-filter` — not the
 * viewport. The settings page animates with a transform, so a dialog rendered
 * inside it lands off-centre and clipped. Re-parenting to the body puts the
 * viewport back in charge.
 *
 * The component keeps owning the node, so props stay reactive; only the
 * insertion point moves. Contrast with hoisting modal state into a store,
 * which fixes the geometry but leaves the data to go stale.
 */
export function portal(node: HTMLElement) {
  const target = document.body;
  const placeholder = document.createComment('portal');
  node.parentNode?.insertBefore(placeholder, node);
  target.appendChild(node);

  return {
    destroy() {
      node.remove();
      placeholder.remove();
    },
  };
}
