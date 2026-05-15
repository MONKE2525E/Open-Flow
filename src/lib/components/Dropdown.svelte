<script lang="ts">
  import { tick } from 'svelte';

  let {
    open = $bindable(false),
    closeSelector = '',
    children,
  }: { open: boolean; closeSelector?: string; children?: import('svelte').Snippet } = $props();

  $effect(() => {
    if (open) {
      tick().then(() =>
        window.addEventListener('click', handleOutsideClick, { once: true })
      );
    }
  });

  function handleOutsideClick(e: MouseEvent) {
    if (closeSelector && (e.target as HTMLElement).closest(closeSelector)) return;
    open = false;
  }
</script>

{@render children?.()}
