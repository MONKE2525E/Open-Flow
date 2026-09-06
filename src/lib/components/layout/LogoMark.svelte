<script lang="ts">
  import canonicalMark from '../../../../src-tauri/icons/verenu-mark.svg?raw';

  // The SVG master owns the geometry. Embed it as markup so currentColor stays
  // live for accent themes instead of becoming a second painted asset.
  const mark = canonicalMark.replace('<svg ', '<svg class="verenu-mark" ');
</script>

{@html mark}

<style>
  :global(.verenu-mark) {
    display: block;
    width: 100%;
    height: 100%;
    overflow: visible;
  }
</style>
