<script lang="ts">
  import type { ProviderId } from '../../settings';
  import { providers } from '../setupData';
  import { getProviderLogo } from '../ProviderLogos';

  let { provider = $bindable() }: { provider: ProviderId } = $props();
</script>

<div class="step">
  <div class="provider-cards">
    {#each providers as p}
      <button
        class="provider-card"
        class:selected={provider === p.id}
        onclick={() => { provider = p.id; }}
      >
        <div class="provider-top">
          <div class="provider-icon">{@html getProviderLogo(p.id)}</div>
          <div class="provider-info">
            <div class="provider-name-row">
              <span class="provider-name">{p.name}</span>
              {#if p.badge}
                <span class="badge">{p.badge}</span>
              {/if}
            </div>
            <span class="provider-tagline">{p.tagline}</span>
          </div>
          <div class="provider-radio" class:checked={provider === p.id}></div>
        </div>
        <p class="provider-desc">{p.desc}</p>
      </button>
    {/each}
  </div>

  <p class="trademark-note">
    The logos above belong to their respective companies. Verenu is not affiliated with, endorsed by, or sponsored by Groq, OpenAI, or Google — they are shown solely to indicate provider compatibility.
  </p>
</div>

<style>
  .trademark-note {
    font-size: 11px;
    color: var(--ink-faint);
    line-height: 1.5;
    margin: 4px 0 0;
  }

  .provider-cards { display: flex; flex-direction: column; gap: 10px; }

  .provider-card {
    background: var(--bg-elev);
    border: 1.5px solid var(--line);
    border-radius: var(--r-md);
    padding: 14px 16px;
    text-align: left;
    cursor: pointer;
    transition: border-color 0.15s, background 0.15s;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .provider-card:hover { border-color: var(--line-strong); }

  .provider-card.selected { border-color: var(--accent); background: var(--accent-soft); }

  .provider-top { display: flex; align-items: center; gap: 12px; }

  .provider-icon { width: 28px; height: 28px; color: var(--ink-mute); flex-shrink: 0; }
  .provider-icon :global(svg) { width: 100%; height: 100%; }

  .provider-card.selected .provider-icon { color: var(--accent-ink); }

  .provider-info { flex: 1; }

  .provider-name-row { display: flex; align-items: center; gap: 8px; margin-bottom: 2px; }

  .provider-name { font-size: 14px; font-weight: 500; color: var(--ink-strong); }

  .badge {
    font-size: 10.5px;
    font-weight: 600;
    background: var(--accent);
    color: var(--on-accent);
    border-radius: 20px;
    padding: 1px 8px;
    letter-spacing: 0.02em;
  }

  .provider-tagline { font-size: 12px; color: var(--ink-mute); }

  .provider-desc {
    font-size: 12.5px;
    color: var(--ink-mute);
    margin: 0;
    line-height: 1.45;
    padding-left: 40px;
  }

  .provider-radio {
    width: 17px;
    height: 17px;
    border-radius: 50%;
    border: 2px solid var(--line-strong);
    flex-shrink: 0;
    transition: border-color 0.15s;
    position: relative;
  }

  .provider-radio.checked { border-color: var(--accent); }
  .provider-radio.checked::after {
    content: '';
    position: absolute;
    inset: 3px;
    border-radius: 50%;
    background: var(--accent);
  }
</style>
