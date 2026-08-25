<script lang="ts">
  import type { UpdateInfo } from '../../stores';
  import { installActionLabel } from './helpers';

  export let currentVersion: string;
  export let updateInfo: UpdateInfo;
  export let installing: boolean;
  export let onInstall: () => void;
  export let onDismiss: () => void;
</script>

<div class="notice-wrap">
  <div class="update-banner">
    <span class="update-text">
      Update available — v{currentVersion} → v{updateInfo.version}
    </span>
    <div class="update-actions">
      <button class="update-dismiss" onclick={onDismiss}>Dismiss</button>
      <button class="update-btn" onclick={onInstall} disabled={installing}>
        {installing
          ? (updateInfo.installMode === 'download' ? 'Opening…' : 'Installing…')
          : installActionLabel(updateInfo)}
      </button>
    </div>
  </div>
</div>

<style>
  .notice-wrap {
    position: relative;
    margin-bottom: 22px;
  }

  .update-banner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 14px;
    padding: 12px 18px;
    background: rgba(217, 119, 87, 0.08);
    border: 1px solid rgba(217, 119, 87, 0.20);
    border-radius: var(--r-lg);
    font-size: 13px;
    color: var(--ink-strong);
  }

  .update-text {
    flex: 1;
    font-family: var(--serif);
    font-weight: 500;
  }

  .update-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
  }

  .update-dismiss {
    padding: 6px 12px;
    background: transparent;
    color: var(--ink-mute);
    border: 1px solid var(--line-strong);
    border-radius: 6px;
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
    transition: color 0.15s, border-color 0.15s;
  }
  .update-dismiss:hover {
    color: var(--ink-strong);
    border-color: var(--ink-mute);
  }

  .update-btn {
    flex-shrink: 0;
    padding: 6px 14px;
    background: var(--accent);
    color: var(--on-accent);
    border: none;
    border-radius: 6px;
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
    transition: background 0.15s, opacity 0.15s;
  }
  .update-btn:hover:not(:disabled) {
    background: var(--accent-ink);
  }
  .update-btn:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
</style>
