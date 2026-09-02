<script lang="ts">
  import Dropdown from '../Dropdown.svelte';
  import { fade, fly } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { MOTION_MS, MOTION_PX, motionMs, motionPx } from '../../motion';
  import { normalizeAccentColor } from '../../accentTheme';

  let {
    value,
    onchange,
  }: {
    value: string | null;
    onchange: (value: string | null) => void | Promise<void>;
  } = $props();

  const presets = [
    { label: 'Terracotta', value: null, color: '#D97757' },
    { label: 'Blue', value: '#4F7FD8', color: '#4F7FD8' },
    { label: 'Teal', value: '#398E94', color: '#398E94' },
    { label: 'Green', value: '#4F9C78', color: '#4F9C78' },
    { label: 'Violet', value: '#8B6FD6', color: '#8B6FD6' },
    { label: 'Rose', value: '#D2637A', color: '#D2637A' },
  ] as const;

  let open = $state(false);
  let draft = $state('#D97757');
  let invalid = $state(false);
  let colorInput: HTMLInputElement | null = $state(null);

  $effect(() => {
    draft = value ?? '#D97757';
    invalid = false;
  });

  const displayColor = $derived(value ?? '#D97757');
  const displayLabel = $derived(value ? value : 'Default');

  async function choose(next: string | null) {
    invalid = false;
    await onchange(next);
  }

  async function commitDraft() {
    const normalized = normalizeAccentColor(draft);
    if (!normalized) {
      invalid = true;
      return;
    }
    draft = normalized;
    invalid = false;
    await onchange(normalized);
  }

  function handleHexKeydown(event: KeyboardEvent) {
    if (event.key !== 'Enter') return;
    event.preventDefault();
    void commitDraft();
  }
</script>

<Dropdown bind:open closeSelector=".accent-picker" optionSelector=".accent-swatch">
  <div class="ui-dropdown accent-picker">
    <button
      class="btn-ghost ui-dropdown-trigger ui-dropdown-trigger--compact accent-trigger"
      aria-haspopup="dialog"
      aria-expanded={open}
      onclick={() => (open = !open)}
    >
      <span class="trigger-swatch" style:background={displayColor} aria-hidden="true"></span>
      <span>{displayLabel}</span>
      <svg width="12" height="12" viewBox="0 0 12 12" fill="none" aria-hidden="true">
        <path d="m3.25 4.75 2.75 2.5 2.75-2.5" stroke="currentColor" stroke-width="1.25" stroke-linecap="round" stroke-linejoin="round"/>
      </svg>
    </button>

    {#if open}
      <div
        class="ui-dropdown-menu accent-menu"
        role="dialog"
        aria-label="Choose accent color"
        in:fly={{ y: motionPx(MOTION_PX.nudge), duration: motionMs(MOTION_MS.panel), easing: cubicOut }}
        out:fade={{ duration: motionMs(MOTION_MS.fast) }}
      >
        <div class="palette-heading">Presets</div>
        <div class="swatch-grid" role="listbox" aria-label="Accent color presets">
          {#each presets as preset}
            <button
              class="accent-swatch"
              class:selected={value === preset.value}
              role="option"
              aria-selected={value === preset.value}
              aria-label={preset.label}
              title={preset.label}
              onclick={() => choose(preset.value)}
            >
              <span style:background={preset.color}></span>
              {#if value === preset.value}
                <svg viewBox="0 0 12 12" aria-hidden="true"><path d="m2.5 6.2 2.1 2.1 4.9-4.8"/></svg>
              {/if}
            </button>
          {/each}
        </div>

        <div class="custom-heading">Custom color</div>
        <div class="custom-row">
          <button class="color-well" aria-label="Open color picker" onclick={() => colorInput?.click()}>
            <span style:background={displayColor}></span>
          </button>
          <input
            bind:this={colorInput}
            class="native-color-input"
            type="color"
            value={displayColor}
            aria-label="Custom accent color"
            onchange={(event) => choose((event.currentTarget as HTMLInputElement).value.toUpperCase())}
          />
          <div class="hex-field" class:invalid>
            <span aria-hidden="true">#</span>
            <input
              value={draft.replace(/^#/, '')}
              aria-label="Accent color hex value"
              aria-invalid={invalid}
              maxlength="7"
              spellcheck="false"
              oninput={(event) => {
                const raw = (event.currentTarget as HTMLInputElement).value.replace(/^#+/, '').slice(0, 6);
                draft = `#${raw}`;
                invalid = false;
              }}
              onkeydown={handleHexKeydown}
            />
          </div>
          <button class="apply-btn" onclick={() => void commitDraft()}>Apply</button>
        </div>
        {#if invalid}
          <div class="color-error" role="alert">Enter a six-digit hex color.</div>
        {/if}
      </div>
    {/if}
  </div>
</Dropdown>

<style>
  .accent-picker { position: relative; }
  .accent-trigger { min-width: 112px; }
  .trigger-swatch {
    border: 1px solid color-mix(in srgb, var(--ink) 16%, transparent);
    border-radius: 50%;
    height: 13px;
    width: 13px;
    transition: background-color var(--ui-duration-base) var(--ui-ease-out), transform var(--ui-duration-fast) var(--ui-ease-out);
  }
  .accent-trigger:hover .trigger-swatch { transform: scale(1.08); }
  .accent-menu {
    --ui-dropdown-menu-max-height: none;
    overflow: visible;
    padding: 12px;
    width: 236px;
  }
  .palette-heading,
  .custom-heading {
    color: var(--ink-mute);
    font-size: 11px;
    font-weight: 500;
  }
  .custom-heading { margin-top: 14px; }
  .swatch-grid {
    display: grid;
    gap: 8px;
    grid-template-columns: repeat(6, 1fr);
    margin-top: 8px;
  }
  .accent-swatch {
    align-items: center;
    background: transparent;
    border: 0;
    border-radius: 50%;
    cursor: pointer;
    display: flex;
    height: 26px;
    justify-content: center;
    padding: 3px;
    position: relative;
    transition: box-shadow var(--ui-duration-fast) var(--ui-ease-out), transform var(--ui-duration-fast) var(--ui-ease-out);
    width: 26px;
  }
  .accent-swatch > span {
    border: 1px solid color-mix(in srgb, var(--ink) 14%, transparent);
    border-radius: inherit;
    inset: 3px;
    position: absolute;
  }
  .accent-swatch:hover { transform: translateY(-1px) scale(1.06); }
  .accent-swatch.selected { box-shadow: 0 0 0 2px var(--bg-elev), 0 0 0 3px var(--ink-mute); }
  .accent-swatch:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
  .accent-swatch svg {
    fill: none;
    height: 12px;
    pointer-events: none;
    position: relative;
    stroke: var(--on-accent);
    stroke-linecap: round;
    stroke-linejoin: round;
    stroke-width: 1.8;
    width: 12px;
  }
  .custom-row { align-items: center; display: flex; gap: 7px; margin-top: 7px; }
  .color-well {
    background: var(--paper-2);
    border: 1px solid var(--line);
    border-radius: 6px;
    cursor: pointer;
    height: 28px;
    padding: 3px;
    width: 28px;
  }
  .color-well span { border-radius: 3px; display: block; height: 100%; width: 100%; }
  .color-well:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
  .native-color-input { height: 0; opacity: 0; pointer-events: none; position: absolute; width: 0; }
  .hex-field {
    align-items: center;
    background: var(--paper);
    border: 1px solid var(--line);
    border-radius: 6px;
    color: var(--ink-faint);
    display: flex;
    height: 28px;
    min-width: 0;
    padding: 0 7px;
    transition: border-color var(--ui-duration-fast) var(--ui-ease-out), box-shadow var(--ui-duration-fast) var(--ui-ease-out);
  }
  .hex-field:focus-within { border-color: var(--accent); box-shadow: var(--ui-focus-ring); }
  .hex-field.invalid { border-color: var(--danger); }
  .hex-field input {
    background: transparent;
    border: 0;
    color: var(--ink);
    font-family: var(--mono);
    font-size: 11px;
    outline: 0;
    padding: 0;
    text-transform: uppercase;
    width: 52px;
  }
  .apply-btn {
    background: var(--accent);
    border: 1px solid var(--accent);
    border-radius: 6px;
    color: var(--on-accent);
    cursor: pointer;
    font-family: var(--sans);
    font-size: 11px;
    font-weight: 500;
    height: 28px;
    padding: 0 9px;
    transition: background-color var(--ui-duration-fast) var(--ui-ease-out), transform var(--ui-duration-fast) var(--ui-ease-out);
  }
  .apply-btn:hover { background: color-mix(in srgb, var(--accent) 86%, var(--ink)); }
  .apply-btn:active { transform: scale(0.97); }
  .apply-btn:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
  .color-error { color: var(--danger); font-size: 10.5px; margin-top: 6px; }

  @media (prefers-reduced-motion: reduce) {
    .trigger-swatch,
    .accent-swatch,
    .apply-btn { transition-duration: 1ms; }
    .accent-swatch:hover { transform: none; }
  }
</style>
