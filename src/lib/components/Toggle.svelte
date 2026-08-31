<script lang="ts">
  let {
    checked = false,
    onchange,
    label = '',
  }: { checked: boolean; onchange: (value: boolean) => void; label?: string } = $props();

  function preventToggleKeyScroll(event: KeyboardEvent) {
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
    }
  }

  function handleToggleKeyup(event: KeyboardEvent) {
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      onchange(!checked);
    }
  }
</script>

<button
  type="button"
  class="toggle"
  class:on={checked}
  role="switch"
  aria-checked={checked}
  aria-label={label || 'Toggle'}
  tabindex="0"
  onclick={() => onchange(!checked)}
  onkeydown={preventToggleKeyScroll}
  onkeyup={handleToggleKeyup}
>
  <span class="toggle-thumb" aria-hidden="true"></span>
</button>

<style>
  .toggle {
    width: 30px;
    height: 16px;
    display: block;
    box-sizing: border-box;
    padding: 0;
    border: 0;
    appearance: none;
    -webkit-appearance: none;
    background: var(--line-strong);
    border-radius: 999px;
    position: relative;
    cursor: pointer;
    transition: background 0.3s ease-out;
    flex-shrink: 0;
  }

  .toggle-thumb {
    position: absolute;
    width: 12px;
    height: 12px;
    background: var(--bg-elev);
    border-radius: 50%;
    top: 50%;
    left: 2px;
    transform: translateY(-50%);
    transition: left 0.35s cubic-bezier(0.22, 1, 0.36, 1);
    box-shadow: 0 1px 2px color-mix(in srgb, var(--ink) 15%, transparent);
  }

  .toggle.on {
    background: var(--accent);
  }

  .toggle.on .toggle-thumb {
    left: 16px;
  }

  .toggle:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
</style>
