<script lang="ts">
  import type { ProviderId } from '../../settings';
  import { providers } from '../setupData';
  import { getProviderLogo } from '../ProviderLogos';

  let { provider = $bindable() }: { provider: ProviderId } = $props();
</script>

<div class="step provider-step">
  <div class="provider-cards">
    {#each providers as p}
      <button
        class="pick-card provider-card"
        class:selected={provider === p.id}
        aria-pressed={provider === p.id}
        onclick={() => { provider = p.id; }}
      >
        <div class="provider-icon">{@html getProviderLogo(p.id)}</div>
        <div class="provider-info">
          <div class="provider-name-row">
            <span class="provider-name">{p.name}</span>
            {#if p.badge}<span class="badge">{p.badge}</span>{/if}
          </div>
          <p class="provider-desc">{p.desc}</p>
        </div>
        <div class="pick-radio" class:checked={provider === p.id}></div>
      </button>
    {/each}
  </div>

  <p class="trademark-note">
    The logos above belong to their respective companies. Verenu is not affiliated with, endorsed by, or sponsored by Groq, OpenAI, or Google — they are shown solely to indicate provider compatibility.
  </p>
</div>

<style>
  .provider-step { gap: 14px; }

  .provider-cards { display: flex; flex-direction: column; gap: 8px; }

  /* Icon, text column, radio on one row, all vertically centred against the
     card — the description used to sit in its own row under a 40px indent and
     the radio hung off the first line, so neither edge lined up. */
  .provider-card {
    flex-direction: row;
    align-items: center;
    gap: 13px;
    border-radius: var(--r-md);
    padding: 13px 15px;
  }

  .provider-icon {
    width: 26px;
    height: 26px;
    color: var(--ink-mute);
    flex-shrink: 0;
    transition: color 0.16s ease;
  }

  .provider-icon :global(svg) { width: 100%; height: 100%; }

  .provider-card.selected .provider-icon { color: var(--accent-ink); }

  .provider-info { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 3px; }

  .provider-name-row { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }

  .provider-name { font-size: 14px; font-weight: 500; color: var(--ink-strong); }

  .badge {
    font-size: 10px;
    font-weight: 600;
    background: var(--accent);
    color: var(--on-accent);
    border-radius: 20px;
    padding: 1px 7px;
    letter-spacing: 0.02em;
  }

  .provider-desc {
    font-size: 12.5px;
    color: var(--ink-mute);
    margin: 0;
    line-height: 1.45;
  }

  .trademark-note {
    font-size: 11px;
    color: var(--ink-faint);
    line-height: 1.5;
    margin: 0;
  }
</style>
