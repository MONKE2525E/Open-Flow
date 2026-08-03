<script lang="ts">
  import { appStore } from '../../stores';
  import { emit } from '../../tauri';
  import AppMappingsEditor from '../AppMappingsEditor.svelte';
</script>

{#if !appStore.cleanupEnabled}
  <div class="cleanup-off-banner">
    <p>
      <strong>Cleanup is turned off</strong>, so app mappings have no effect right now — the tone
      and per-app overrides here only apply during the cleanup step. Your mappings are kept, just
      not used.
    </p>
    <button type="button" class="cleanup-off-link" onclick={() => emit('open-flow:open-settings-section', 'general')}>
      Turn Cleanup back on in Settings → General
    </button>
  </div>
{/if}

<div class="mappings-host" class:mappings-disabled={!appStore.cleanupEnabled} aria-disabled={!appStore.cleanupEnabled} inert={!appStore.cleanupEnabled}>
  <AppMappingsEditor />
</div>

<style>
  /*
   * The editor caps itself at 640px because it is also rendered on the Style
   * page, whose column runs to --page-max (1160px). In settings the column is
   * already the measure, so the cap only made the mappings sit short of the
   * section heading — drop it here and let the settings column govern.
   */
  .mappings-host {
    --mappings-measure: none;
  }

  .mappings-disabled {
    opacity: 0.45;
    pointer-events: none;
    user-select: none;
  }

  .cleanup-off-banner {
    padding: 12px 14px;
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    background: color-mix(in srgb, var(--paper) 55%, var(--bg-elev));
    margin-bottom: 20px;
  }

  .cleanup-off-banner p {
    margin: 0 0 8px;
    font-size: 12.5px;
    line-height: 1.5;
    color: var(--ink-mute);
  }

  .cleanup-off-banner strong {
    color: var(--ink-soft);
  }

  .cleanup-off-link {
    font-size: 12.5px;
    font-weight: 500;
    color: var(--accent-ink);
    background: transparent;
    border: 0;
    padding: 0;
    cursor: pointer;
    text-decoration: underline;
  }

  .cleanup-off-link:hover {
    opacity: 0.8;
  }
</style>
