// Svelte action for the soft top/bottom scroll fades. Applied to a scroll
// container, it reports whether there's more content above/below so the caller
// can show a top and/or bottom fade only when it's actually meaningful — a
// scrolled-to-top page keeps its heading crisp. Reused by Settings and the main
// content area.

export type ScrollEdgeCallback = (top: boolean, bottom: boolean, node: HTMLElement) => void;

export function scrollEdges(node: HTMLElement, onChange: ScrollEdgeCallback) {
  let callback = onChange;
  const update = () => {
    const { scrollTop, scrollHeight, clientHeight } = node;
    callback(scrollTop > 4, scrollTop + clientHeight < scrollHeight - 4, node);
  };
  update();
  node.addEventListener('scroll', update, { passive: true });
  const observer = new ResizeObserver(update);
  observer.observe(node);
  return {
    update(next: ScrollEdgeCallback) {
      callback = next;
      update();
    },
    destroy() {
      node.removeEventListener('scroll', update);
      observer.disconnect();
    },
  };
}
