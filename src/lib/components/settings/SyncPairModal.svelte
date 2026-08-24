<script lang="ts">
  import { invoke } from '../../tauri';
  import { syncStore } from '../../syncStore.svelte';
  import { modalFocusTrap } from '../../modalFocus';
  import { modalBackdrop, modalCard, MOTION_MS, motionMs } from '../../motion';
  import { fade } from 'svelte/transition';
  import { icons } from '../../icons';

  let code = $state('');
  let busy = $state(false);
  let error = $state('');

  let codeInput = $state<HTMLInputElement | null>(null);
  let rejectButton = $state<HTMLButtonElement | null>(null);

  const incoming = $derived(syncStore.incoming);

  $effect(() => {
    incoming;
    code = '';
    error = '';
  });

  async function respond(approve: boolean): Promise<void> {
    if (!incoming || busy) return;
    busy = true;
    error = '';
    try {
      await invoke('sync_respond_to_pairing', { code: code.replace(/\s/g, ''), approve });
      syncStore.incoming = null;
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }

  function dismiss(): void {
    // Closing the prompt without deciding declines quietly - pairing must be
    // explicit on both devices.
    if (busy) return;
    syncStore.incoming = null;
    void invoke('sync_respond_to_pairing', { code: '', approve: false }).catch(() => {});
  }
</script>

{#if incoming}
  <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
  <button
    class="modal-backdrop"
    aria-label="Dismiss pairing request"
    onclick={dismiss}
    in:modalBackdrop={{ duration: 180 }}
    out:modalBackdrop={{ duration: 160 }}
  ></button>
  <div
    class="modal-card pair-card"
    role="dialog"
    aria-modal="true"
    aria-label="Pairing request from {incoming.name}"
    use:modalFocusTrap={{
      active: !!incoming,
      initialFocus: () => codeInput ?? rejectButton,
    }}
    in:modalCard={{ duration: motionMs(MOTION_MS.panel) }}
    out:modalCard={{ duration: motionMs(MOTION_MS.fast) }}
  >
    <div class="pair-head">
      <div class="pair-icon">
        <svg
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          {@html icons.devices}
        </svg>
      </div>
      <div>
        <div class="pair-title">Pair this device?</div>
        <div class="pair-sub">
          <strong>{incoming.name}</strong> wants to sync with this device over your local network.
        </div>
      </div>
    </div>
    <label class="pair-label" for="pair-code-input">Enter the code shown there</label>
    <input
      id="pair-code-input"
      bind:this={codeInput}
      bind:value={code}
      class="ui-input pair-code"
      inputmode="numeric"
      autocomplete="off"
      placeholder="000000"
      maxlength={7}
      spellcheck="false"
      onkeydown={(e) => {
        if (e.key === 'Enter' && !busy && code.replace(/\s/g, '').length === 6) void respond(true);
      }}
    />
    {#if error}
      <div class="pair-error" role="alert" in:fade={{ duration: motionMs(MOTION_MS.fast) }}>
        {error}
      </div>
    {/if}
    <div class="pair-actions">
      <button
        bind:this={rejectButton}
        class="btn-ghost btn-compact"
        onclick={() => void respond(false)}
        disabled={busy}
      >
        Reject
      </button>
      <button
        class="btn-primary btn-compact"
        onclick={() => void respond(true)}
        disabled={busy || code.replace(/\s/g, '').length !== 6}
      >
        {busy ? 'Pairing…' : 'Pair'}
      </button>
    </div>
  </div>
{:else}
  <div hidden></div>
{/if}

<style>
  .pair-card {
    width: min(400px, calc(100vw - 48px));
    padding: 20px;
    display: grid;
    gap: 12px;
  }
  .pair-head {
    display: flex;
    gap: 12px;
    align-items: flex-start;
  }
  .pair-icon {
    width: 34px;
    height: 34px;
    border-radius: var(--r-md, 10px);
    background: var(--accent-soft, rgba(125, 125, 125, 0.14));
    color: var(--accent);
    display: grid;
    place-items: center;
    flex-shrink: 0;
  }
  .pair-icon svg {
    width: 19px;
    height: 19px;
  }
  .pair-title {
    font-size: 14.5px;
    font-weight: 600;
  }
  .pair-sub {
    font-size: 12.5px;
    color: var(--text-dim);
    margin-top: 2px;
    line-height: 1.45;
  }
  .pair-label {
    font-size: 12px;
    color: var(--text-dim);
  }
  .pair-code {
    font-family: var(--font-mono);
    font-size: 20px;
    letter-spacing: 0.35em;
    text-align: center;
    padding: 10px 8px;
  }
  .pair-error {
    font-size: 12px;
    color: var(--accent);
  }
  .pair-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 2px;
  }
</style>
