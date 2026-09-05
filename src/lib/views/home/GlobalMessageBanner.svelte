<script lang="ts">
  import { icons } from '../../icons';

  let { message }: { message: string } = $props();
</script>

<div class="notice-wrap" role="status">
  <div class="global-banner">
    <span class="global-icon" aria-hidden="true">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
        {@html icons.bell}
      </svg>
    </span>
    <span class="global-text">{message}</span>
  </div>
</div>

<style>
  .notice-wrap { position: relative; margin-bottom: 22px; }
  .global-banner {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 18px;
    background: var(--warn-bg, rgba(196, 116, 42, 0.08));
    border: 1px solid var(--warn-line, rgba(196, 116, 42, 0.35));
    border-radius: var(--r-lg);
    font-size: 13px;
    color: var(--ink);
  }
  .global-icon {
    display: inline-grid;
    place-items: center;
    flex: 0 0 24px;
    height: 24px;
    border-radius: 50%;
    color: var(--warning, #c4742a);
  }
  .global-icon svg {
    width: 18px;
    height: 18px;
  }
  .global-text { flex: 1; font-family: var(--sans); font-weight: 500; }
</style>
