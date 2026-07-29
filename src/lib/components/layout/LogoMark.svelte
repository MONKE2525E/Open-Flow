<script lang="ts">
  // Shared fixed-geometry mark. Keeping the bars as SVG rects avoids flexbox
  // rasterization making one bar appear narrower at fractional DPI.
</script>

<svg viewBox="0 0 48 40" width="100%" height="100%" fill="none" aria-hidden="true">
  <rect x="3"  y="26" width="6" height="14" rx="3" fill="currentColor"/>
  <rect x="12" y="12" width="6" height="28" rx="3" fill="currentColor"/>
  <rect x="21" y="0"  width="6" height="40" rx="3" fill="currentColor"/>
  <rect x="30" y="18" width="6" height="22" rx="3" fill="currentColor"/>
  <rect x="39" y="30" width="6" height="10" rx="3" fill="currentColor"/>
</svg>
