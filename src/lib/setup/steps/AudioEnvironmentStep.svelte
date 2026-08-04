<script lang="ts">
  import { isWindows } from '../../platform';

  // One answer drives two settings: with speakers, whatever is playing bleeds
  // into the mic, so Verenu silences it (and pauses media, on Windows) while
  // you dictate. Headphones need neither.
  let { usesHeadphones = $bindable(true) }: { usesHeadphones?: boolean } = $props();

  const options = [
    {
      id: true,
      title: 'Headphones',
      sub: 'Wired, Bluetooth, or a headset',
      detail: 'Nothing you play can reach the microphone, so Verenu leaves your audio alone.',
    },
    {
      id: false,
      title: 'Speakers',
      sub: 'Laptop or desktop speakers',
      detail: isWindows
        ? 'Verenu will mute system audio and pause playing media while you dictate, so music and video do not end up in your transcript.'
        : 'Verenu will mute system audio while you dictate, so music and video do not end up in your transcript.',
    },
  ];
</script>

<div class="step audio-env-step">
  <div class="env-grid">
    {#each options as opt}
      <button
        type="button"
        class="pick-card env-card"
        class:selected={usesHeadphones === opt.id}
        aria-pressed={usesHeadphones === opt.id}
        onclick={() => { usesHeadphones = opt.id; }}
      >
        <div class="env-head">
          <span class="env-icon" aria-hidden="true">
            {#if opt.id}
              <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><path d="M4 15v-3a8 8 0 0 1 16 0v3"/><path d="M4 15a2 2 0 0 1 2-2h1v6H6a2 2 0 0 1-2-2v-2Z"/><path d="M20 15a2 2 0 0 0-2-2h-1v6h1a2 2 0 0 0 2-2v-2Z"/></svg>
            {:else}
              <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><rect x="5" y="2.5" width="14" height="19" rx="2.5"/><circle cx="12" cy="15" r="3.2"/><circle cx="12" cy="7" r="1.3"/></svg>
            {/if}
          </span>
          <div class="env-titles">
            <span class="env-title">{opt.title}</span>
            <span class="env-sub">{opt.sub}</span>
          </div>
          <div class="pick-radio" class:checked={usesHeadphones === opt.id}></div>
        </div>
        <p class="env-detail">{opt.detail}</p>
      </button>
    {/each}
  </div>

  <p class="env-note">Either way, you can change this later in Settings → Audio.</p>
</div>

<style>
  .audio-env-step { gap: 14px; }

  .env-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; align-items: stretch; }

  .env-card {
    border-radius: var(--r-md);
    padding: 16px;
    gap: 10px;
    height: 100%;
  }

  .env-head { display: flex; align-items: flex-start; gap: 11px; }

  .env-icon { color: var(--ink-faint); flex-shrink: 0; transition: color 0.16s ease; }
  .env-card.selected .env-icon { color: var(--accent-ink); }

  .env-titles { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }

  .env-title { font-size: 14px; font-weight: 500; color: var(--ink-strong); }
  .env-sub { font-size: 11.5px; color: var(--ink-faint); }

  .pick-radio { margin-top: 3px; }

  .env-detail { margin: 0; font-size: 12.5px; color: var(--ink-mute); line-height: 1.45; }

  .env-note { margin: 0; font-size: 11.5px; color: var(--ink-faint); }

  @media (max-width: 640px) {
    .env-grid { grid-template-columns: 1fr; }
  }
</style>
