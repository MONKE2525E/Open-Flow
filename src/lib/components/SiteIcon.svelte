<script lang="ts">
  // Uses Google's public favicon service to show the real site icon for a
  // website context target. This sends the domain to Google whenever a chip
  // renders — approved as an explicit tradeoff for real favicons (see commit
  // history / DATA_AND_PRIVACY.md) rather than the previous generic globe glyph.
  let { domain, size = 16 }: { domain: string; size?: number } = $props();

  let failed = $state(false);

  $effect(() => {
    domain;
    failed = false;
  });
</script>

{#if !failed}
  <img
    class="site-icon"
    src="https://www.google.com/s2/favicons?sz={size * 2}&domain={encodeURIComponent(domain)}"
    alt=""
    style="width: {size}px; height: {size}px;"
    onerror={() => failed = true}
  />
{:else}
  <span class="site-icon site-icon-fallback" style="width: {size}px; height: {size}px;" aria-hidden="true">
    <svg width={Math.round(size * 0.7)} height={Math.round(size * 0.7)} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"><circle cx="12" cy="12" r="9"/><path d="M3 12h18M12 3c2.2 2.5 3.3 5.5 3.3 9s-1.1 6.5-3.3 9c-2.2-2.5-3.3-5.5-3.3-9S9.8 5.5 12 3Z"/></svg>
  </span>
{/if}

<style>
  .site-icon {
    border-radius: 6px;
    flex: 0 0 auto;
    display: block;
    object-fit: contain;
  }
  .site-icon-fallback {
    display: grid;
    place-items: center;
    background: var(--bg-elev);
    color: var(--ink-mute);
    border: 1px solid var(--line);
  }
</style>
