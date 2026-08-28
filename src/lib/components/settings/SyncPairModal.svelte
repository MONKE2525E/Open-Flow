<script lang="ts">
  import { invoke } from '../../tauri';
  import { syncStore, errorText, PAIRING_WINDOW_MS } from '../../syncStore.svelte';
  import { modalFocusTrap } from '../../modalFocus';
  import { modalBackdrop, modalCard, MOTION_MS, motionMs, motionPx } from '../../motion';
  import { fade, fly } from 'svelte/transition';
  import { icons } from '../../icons';
  import { tick } from 'svelte';

  let code = $state('');
  let busy = $state(false);
  let error = $state('');
  let shake = $state(0);

  let codeInput = $state<HTMLInputElement | null>(null);
  let rejectButton = $state<HTMLButtonElement | null>(null);

  const incoming = $derived(syncStore.incoming);
  const complete = $derived(code.length === 6);

  let now = $state(Date.now());
  $effect(() => {
    if (!syncStore.incoming) return;
    now = Date.now();
    const id = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(id);
  });

  const remainingMs = $derived(
    incoming ? Math.max(0, PAIRING_WINDOW_MS - (now - incoming.startedAt)) : 0,
  );
  const expired = $derived(!!incoming && remainingMs === 0);

  $effect(() => {
    incoming;
    code = '';
    error = '';
  });

  // Digits only: the backend expects a bare 6-digit code, and a pasted "482 913"
  // used to fail validation instead of just working.
  function onInput(event: Event): void {
    const input = event.currentTarget as HTMLInputElement;
    const digits = input.value.replace(/\D/g, '').slice(0, 6);
    code = digits;
    input.value = digits;
    error = '';
  }

  async function respond(approve: boolean): Promise<void> {
    if (!incoming || busy) return;
    busy = true;
    error = '';
    try {
      await invoke('sync_respond_to_pairing', { code, approve });
      syncStore.incoming = null;
    } catch (err) {
      // A mistyped code keeps the request alive on the backend, so leave the
      // dialog open and hand the field back to the user.
      error = errorText(err);
      shake += 1;
      code = '';
      await tick();
      codeInput?.focus();
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

  function handleKeydown(event: KeyboardEvent): void {
    if (event.key !== 'Escape' || !incoming) return;
    event.preventDefault();
    dismiss();
  }

  function countdown(ms: number): string {
    const total = Math.ceil(ms / 1000);
    return `${Math.floor(total / 60)}:${String(total % 60).padStart(2, '0')}`;
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if incoming}
  <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
  <button
    class="modal-backdrop pair-backdrop"
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

    {#key shake}
      <div class="code-field" class:is-invalid={!!error}>
        <input
          id="pair-code-input"
          bind:this={codeInput}
          value={code}
          oninput={onInput}
          class="ui-input pair-code"
          inputmode="numeric"
          autocomplete="one-time-code"
          maxlength={6}
          spellcheck="false"
          disabled={busy || expired}
          onkeydown={(e) => {
            if (e.key === 'Enter' && !busy && complete) void respond(true);
          }}
        />
        <div class="code-slots" aria-hidden="true">
          {#each Array.from({ length: 6 }) as _, i}
            <span class="slot" class:filled={i < code.length} class:active={i === code.length || (complete && i === 5)}>
              {code[i] ?? ''}
            </span>
          {/each}
        </div>
      </div>
    {/key}

    <div class="pair-foot">
      {#if error}
        <span class="pair-error" role="alert" in:fly={{ y: motionPx(-3), duration: motionMs(MOTION_MS.fast) }}>
          {error}
        </span>
      {:else if expired}
        <span class="pair-error" role="alert">This request expired — ask the other device to try again.</span>
      {:else}
        <span class="pair-timer" in:fade={{ duration: motionMs(MOTION_MS.fast) }}>
          Expires in {countdown(remainingMs)}
        </span>
      {/if}
    </div>

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
        disabled={busy || expired || !complete}
      >
        {busy ? 'Pairing…' : 'Pair'}
      </button>
    </div>
  </div>
{:else}
  <div hidden></div>
{/if}

<style>
  /*
   * This prompt is raised above the settings overlay (z-index 60): an incoming
   * pairing request is unmissable by definition, and Settings - Sync is exactly
   * where the user is standing when the other device asks.
   */
  .pair-backdrop {
    z-index: 70;
  }
  .pair-card {
    z-index: 71;
    width: min(400px, calc(100vw - 48px));
    padding: 24px;
    display: grid;
    gap: 14px;
  }
  .pair-head {
    display: flex;
    gap: 14px;
    align-items: flex-start;
  }
  .pair-icon {
    position: relative;
    width: 34px;
    height: 34px;
    border-radius: var(--r-md);
    background: var(--control-active);
    border: 1px solid var(--line);
    color: var(--ink-soft);
    display: grid;
    place-items: center;
    flex-shrink: 0;
  }
  .pair-icon svg {
    width: 19px;
    height: 19px;
    position: relative;
    z-index: 1;
  }
  .pair-title {
    font-size: 14.5px;
    font-weight: 600;
    color: var(--ink-strong);
  }
  .pair-sub {
    font-size: 12.5px;
    color: var(--ink-mute);
    margin-top: 4px;
    line-height: 1.55;
  }
  .pair-label {
    font-size: 12px;
    color: var(--ink-mute);
  }

  /* Six visible slots over one real input: the input stays the focus target
     and keeps native paste/IME behaviour. */
  .code-field {
    position: relative;
    animation: none;
  }
  .code-field.is-invalid {
    animation: shake 320ms var(--ui-ease-out);
  }
  @keyframes shake {
    0%,
    100% {
      transform: translateX(0);
    }
    20% {
      transform: translateX(-5px);
    }
    45% {
      transform: translateX(4px);
    }
    70% {
      transform: translateX(-2px);
    }
  }
  .pair-code {
    width: 100%;
    height: 52px;
    opacity: 0;
    cursor: text;
  }
  .code-slots {
    position: absolute;
    inset: 0;
    display: grid;
    grid-template-columns: repeat(6, 1fr);
    gap: 6px;
    pointer-events: none;
  }
  .slot {
    display: grid;
    place-items: center;
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    background: var(--control-hover);
    color: var(--ink-strong);
    font-family: var(--mono);
    font-size: 20px;
    font-variant-numeric: tabular-nums;
    transition:
      border-color var(--ui-duration-fast) var(--ui-ease-out),
      background-color var(--ui-duration-fast) var(--ui-ease-out),
      transform var(--ui-duration-fast) var(--ui-ease-out);
  }
  .slot.filled {
    background: var(--bg-elev);
    border-color: var(--line-strong);
    transform: translateY(-1px);
  }
  .pair-code:focus-visible ~ .code-slots .slot.active {
    border-color: var(--accent);
    box-shadow: var(--ui-focus-ring);
  }
  .code-field.is-invalid .slot {
    border-color: var(--danger-line);
  }

  .pair-foot {
    min-height: 16px;
    margin-top: 2px;
    font-size: 12px;
    line-height: 1.4;
  }
  .pair-error {
    color: var(--danger);
  }
  .pair-timer {
    color: var(--ink-faint);
  }
  .pair-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 2px;
  }

  @media (prefers-reduced-motion: reduce) {
    .code-field.is-invalid {
      animation: none;
    }
    .slot.filled {
      transform: none;
    }
  }
</style>

