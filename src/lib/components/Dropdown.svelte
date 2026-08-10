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
    // e.target can be document/text nodes — closest() only exists on Element.
    if (closeSelector && e.target instanceof Element && (e.target as HTMLElement).closest(closeSelector)) return;
    open = false;
  }

  function handleWindowKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') open = false;
  }
</script>

{@render children?.()}
