<script lang="ts">
  import { untrack } from 'svelte';
  import { tweened } from 'svelte/motion';
  import { expoOut } from 'svelte/easing';
  import { motionMs } from '../../motion';

  let {
    value,
    format = (n: number) => Math.round(n).toLocaleString(),
  }: { value: number; format?: (n: number) => string } = $props();

  // Only the value at mount seeds the tween; every later change is picked up
  // by the $effect below, which is the point — silence the reactivity lint.
  const display = tweened(untrack(() => value), { duration: motionMs(700), easing: expoOut });

  $effect(() => {
    display.set(value);
  });
</script>

<span class="animated-num">{format($display)}</span>
