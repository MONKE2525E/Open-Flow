<script lang="ts">
  let {
    checked = false,
    onchange,
  }: { checked: boolean; onchange: (value: boolean) => void } = $props();
</script>

<div
  class="toggle"
  class:on={checked}
  role="switch"
  aria-checked={checked}
  tabindex="0"
  onclick={() => onchange(!checked)}
  onkeydown={(e) => (e.key === 'Enter' || e.key === ' ') && onchange(!checked)}
></div>

<style>
  .toggle {
    width: 30px;
    height: 16px;
    background: var(--line-strong);
    border-radius: 999px;
    position: relative;
    cursor: pointer;
    transition: background 0.3s ease-out;
    flex-shrink: 0;
  }

  .toggle::after {
    content: '';
    position: absolute;
    width: 12px;
    height: 12px;
    background: var(--bg-elev);
    border-radius: 50%;
    top: 2px;
    left: 2px;
    transition: left 0.35s cubic-bezier(0.22, 1, 0.36, 1);
    box-shadow: 0 1px 2px color-mix(in srgb, var(--ink) 15%, transparent);
  }

  .toggle.on {
    background: var(--accent);
  }

  .toggle.on::after {
    left: 16px;
  }
</style>
