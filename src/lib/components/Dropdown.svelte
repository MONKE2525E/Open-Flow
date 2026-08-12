<script lang="ts">
  import { tick } from 'svelte';

  let {
    open = $bindable(false),
    closeSelector = '',
    children,
  }: { open: boolean; closeSelector?: string; children?: import('svelte').Snippet } = $props();

  $effect(() => {
    if (!open) return;

    let disposed = false;
    tick().then(() => {
      if (!disposed) window.addEventListener('click', handleOutsideClick);
    });
    window.addEventListener('keydown', handleWindowKeydown);

    return () => {
      disposed = true;
      window.removeEventListener('click', handleOutsideClick);
      window.removeEventListener('keydown', handleWindowKeydown);
    };
  });

  function handleOutsideClick(e: MouseEvent) {
    // closest() only exists on Element; a click target can be a Text node or
    // document, so resolve to the nearest element first before testing the
    // close selector (e.g. clicking the label text inside the trigger).
    const target = e.target instanceof Element
      ? e.target
      : (e.target as Node)?.parentElement;
    if (closeSelector && target?.closest(closeSelector)) return;
    open = false;
  }

  function handleWindowKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') open = false;
  }
</script>

{@render children?.()}
