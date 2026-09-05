<script lang="ts">
  import { slide } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { invoke } from '../../tauri';
  import { saveSetting } from '../../settings';
  import {
    cleanupPromptEditor,
    cleanupPromptStore,
    closeCleanupPromptEditor,
  } from '../../stores.svelte';
  import { portal } from '../../portal';
  import { qualifiedModelLabel } from './models';
  import { modalFocusTrap } from '../../modalFocus';
  import {
    modalBackdrop,
    expandFromOrigin,
    MOTION_MS,
    MOTION_PX,
    motionMs,
    motionPx,
  } from '../../motion';

  const CLEANUP_PROMPT_TAGS = [
    '{{ active_app }}',
    '{{ cleanup_preset }}',
    '{{ formatting_rules }}',
    '{{ snippet_overrides }}',
  ];

  interface PromptTestCaseResult {
    name: string;
    passed: boolean;
    detail: string;
  }

  interface PromptTestReport {
    passed: boolean;
    static_warnings: string[];
    live_results: PromptTestCaseResult[];
    live_warnings: string[];
  }

  type TestStatus =
    | { status: 'idle' }
    | { status: 'testing' }
    | { status: 'passed' }
    | { status: 'failed'; report?: PromptTestReport; error?: string };

  let modalEl = $state<HTMLDivElement | null>(null);

  let draft = $state('');
  let defaultText = $state('');
  let loading = $state(false);
  let testState = $state<TestStatus>({ status: 'idle' });
  let liveWarnings = $state<string[]>([]);

  let isMounted = true;
  let lintTimeout: ReturnType<typeof setTimeout> | undefined;

  $effect(() => {
    return () => {
      isMounted = false;
      clearTimeout(lintTimeout);
    };
  });

  const origin = $derived(cleanupPromptEditor.origin ?? undefined);
  const provider = $derived(cleanupPromptEditor.provider!);
  const model = $derived(cleanupPromptEditor.model!);

  const testedAgainst = $derived(qualifiedModelLabel(provider, model));

  const statusKind = $derived((): 'clean' | 'warn' | 'error' | 'testing' | 'passed' => {
    if (testState.status === 'testing') return 'testing';
    if (testState.status === 'passed') return 'passed';
    if (testState.status === 'failed') return 'error';
    if (liveWarnings.length > 0) return 'warn';
    return 'clean';
  });

  const statusLabel = $derived((): string => {
    const k = statusKind();
    if (k === 'testing') return 'Testing…';
    if (k === 'passed') return 'Saved';
    if (k === 'error') {
      if (testState.status === 'failed') {
        if (testState.error) return 'Error';
        if (testState.report) {
          const n =
            testState.report.static_warnings.length +
            testState.report.live_warnings.length +
            testState.report.live_results.filter((r) => !r.passed).length;
          return `Failed ${n} check${n !== 1 ? 's' : ''}`;
        }
      }
      return 'Failed';
    }
    if (k === 'warn') return `${liveWarnings.length} warning${liveWarnings.length !== 1 ? 's' : ''}`;
    return 'Looks good';
  });

  $effect(() => {
    if (!cleanupPromptEditor.open) return;
    loadDraft();
  });

  // The textarea only mounts after the draft IPC returns (loading gate). The
  // focus trap lands on the card while loading; hand focus to the editor once
  // it exists, unless the user has already moved inside the dialog.
  $effect(() => {
    if (!cleanupPromptEditor.open || loading) return;
    const textarea = modalEl?.querySelector<HTMLElement>('textarea');
    if (!textarea) return;
    const active = document.activeElement;
    if (active === modalEl || active === document.body) textarea.focus();
  });

  async function loadDraft() {
    loading = true;
    draft = '';
    defaultText = '';
    testState = { status: 'idle' };
    liveWarnings = [];
    try {
      const def = await invoke<string>('get_default_cleanup_prompt');
      if (!isMounted) return;
      defaultText = def;
      const saved = cleanupPromptStore.override.trim();
      const text = saved || def;
      draft = text;
      runLint(text);
    } catch (err) {
      if (isMounted) console.error('CleanupPromptModal loadDraft failed:', err);
    } finally {
      if (isMounted) loading = false;
    }
  }

  function runLint(text: string) {
    clearTimeout(lintTimeout);
    lintTimeout = setTimeout(async () => {
      try {
        const warnings = await invoke<string[]>('lint_cleanup_prompt', { template: text });
        if (isMounted) liveWarnings = warnings;
      } catch {
        // lint errors are non-critical
      }
    }, 300);
  }

  function onInput(e: Event) {
    draft = (e.currentTarget as HTMLTextAreaElement).value;
    if (testState.status === 'passed' || testState.status === 'failed') {
      testState = { status: 'idle' };
    }
    runLint(draft);
  }

  /** Matching the default is how you clear the override, not a value to store. */
  function applyOverride(text: string) {
    cleanupPromptStore.override = text.trim() === defaultText.trim() ? '' : text;
  }

  async function handleSave(force = false) {
    if (force) {
      applyOverride(draft);
      await saveSetting('cleanup_prompt_override', cleanupPromptStore.override);
      if (!isMounted) return;
      testState = { status: 'passed' };
      setTimeout(() => { if (isMounted) closeCleanupPromptEditor(); }, 500);
      return;
    }

    testState = { status: 'testing' };
    try {
      const report = await invoke<PromptTestReport>('test_cleanup_prompt', {
        provider,
        model,
        template: draft,
      });
      if (!isMounted) return;
      if (report.passed) {
        applyOverride(draft);
        await saveSetting('cleanup_prompt_override', cleanupPromptStore.override);
        if (!isMounted) return;
        testState = { status: 'passed' };
        setTimeout(() => { if (isMounted) closeCleanupPromptEditor(); }, 600);
      } else {
        testState = { status: 'failed', report };
      }
    } catch (err) {
      if (!isMounted) return;
      testState = {
        status: 'failed',
        error: err instanceof Error ? err.message : String(err),
      };
    }
  }

  function handleReset() {
    testState = { status: 'idle' };
    liveWarnings = [];
    draft = defaultText;
    runLint(defaultText);
  }

  function handleClose() {
    closeCleanupPromptEditor();
  }

  function onBackdropClick() {
    handleClose();
  }

  // The focus trap owns Tab cycling and focus restore; Escape stays here so a
  // dialog is always one key away from dismissal (Settings' Escape guard
  // yields while [role="dialog"] is present).
  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      handleClose();
    }
  }

  const failedLiveResults = $derived(
    testState.status === 'failed' && testState.report
      ? testState.report.live_results.filter((r) => !r.passed)
      : []
  );
  const allErrors = $derived(
    testState.status === 'failed' && testState.report
      ? [
          ...testState.report.static_warnings,
          ...testState.report.live_warnings,
          ...failedLiveResults.map((r) => `${r.name}: ${r.detail}`),
        ]
      : testState.status === 'failed' && testState.error
        ? [testState.error]
        : liveWarnings
  );
  const showSaveAnyway = $derived(testState.status === 'failed');
</script>

<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
<div class="prompt-modal-wrap" use:portal>
  <div
    class="prompt-modal-backdrop"
    onclick={onBackdropClick}
    onkeydown={(event) => { if (event.key === 'Enter' || event.key === ' ') { event.preventDefault(); onBackdropClick(); } }}
    role="button"
    tabindex="0"
    aria-label="Close dialog"
    in:modalBackdrop={{ duration: 180 }}
    out:modalBackdrop={{ duration: 160 }}
  ></div>

  <div
    bind:this={modalEl}
    class="prompt-modal-card"
    use:modalFocusTrap={{
      active: true,
      initialFocus: () => modalEl?.querySelector<HTMLElement>('textarea') ?? modalEl,
    }}
    role="dialog"
    aria-modal="true"
    aria-label="Edit cleanup prompt"
    tabindex="-1"
    onkeydown={onKeydown}
    in:expandFromOrigin={{ origin, duration: 240 }}
    out:expandFromOrigin={{ origin, duration: 180 }}
  >
    <!-- Close — absolute, matches .settings-close -->
    <button type="button" class="prompt-modal-close" aria-label="Close editor" onclick={handleClose}>
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round">
        <path d="M6 6l12 12M18 6L6 18"/>
      </svg>
    </button>

    <!-- Header bar — paper bg like settings sidebar -->
    <div class="prompt-head">
      <div class="prompt-head-info">
        <span class="prompt-head-provider">Cleanup prompt</span>
        <span class="prompt-head-model">used by every model · tested on {testedAgainst}</span>
      </div>
      <div class="prompt-head-actions">
        {#if statusKind() !== 'clean'}
          <span class="status-chip status-chip--{statusKind()}">
            {#if statusKind() === 'testing'}
              <span class="chip-spinner" aria-hidden="true"></span>
            {:else}
              <span class="chip-dot" aria-hidden="true"></span>
            {/if}
            {statusLabel()}
          </span>
        {/if}
        <button
          class="prompt-btn-ghost"
          type="button"
          disabled={loading || testState.status === 'testing'}
          onclick={handleReset}
        >Reset</button>
        <button
          class="prompt-btn"
          type="button"
          disabled={loading || testState.status === 'testing'}
          onclick={() => handleSave(false)}
        >{testState.status === 'testing' ? 'Testing…' : 'Save'}</button>
      </div>
    </div>

    <!-- Error / warning panel -->
    {#if allErrors.length > 0}
      <div
        class="prompt-result"
        class:prompt-result--warn={testState.status !== 'failed'}
        class:prompt-result--fail={testState.status === 'failed'}
        transition:slide={{ duration: motionMs(MOTION_MS.fast), easing: cubicOut }}
      >
        <ul>
          {#each allErrors as msg}
            <li>{msg}</li>
          {/each}
        </ul>
        {#if showSaveAnyway}
          <div class="prompt-result-actions">
            <button class="prompt-btn-ghost" type="button" onclick={() => handleSave(true)}>Save anyway</button>
          </div>
        {/if}
      </div>
    {/if}

    <!-- Tags -->
    <div class="prompt-tags-row">
      {#each CLEANUP_PROMPT_TAGS as tag}
        <span class="prompt-tag">{tag}</span>
      {/each}
    </div>

    <!-- Editor -->
    <div class="prompt-editor-body">
      {#if loading}
        <p class="prompt-loading">Loading…</p>
      {:else}
        <textarea
          class="prompt-textarea scroll-styled"
          style="--scroll-thumb-border: var(--paper)"
          value={draft}
          oninput={onInput}
          spellcheck={false}
          disabled={testState.status === 'testing'}
          aria-label="Cleanup prompt template"
        ></textarea>
      {/if}
    </div>
  </div>
</div>

<style>
  /*
   * Fixed and portalled to the body: the settings page animates with a
   * transform, which would otherwise become this dialog's containing block.
   * The card also no longer borrows .ui-modal-card — that class is
   * position:fixed with left/top:50% and translate:-50% -50%, and those offsets
   * survived the local position:relative override, which is what parked the
   * dialog in the bottom-right corner.
   */
  .prompt-modal-wrap {
    position: fixed;
    inset: 0;
    z-index: 70;
    display: grid;
    place-items: center;
  }

  .prompt-modal-backdrop {
    position: absolute;
    inset: 0;
    background: var(--overlay);
    backdrop-filter: blur(2px);
  }

  .prompt-modal-card {
    position: relative;
    z-index: 1;
    width: min(800px, 92vw);
    height: min(580px, 88vh);
    background: var(--bg-elev);
    border: 1px solid var(--line);
    border-radius: var(--r-lg);
    box-shadow: var(--shadow-elev);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  /* ── Close button — matches .settings-close exactly ── */
  .prompt-modal-close {
    position: absolute;
    top: 10px;
    right: 10px;
    width: 30px;
    height: 30px;
    border: 1px solid var(--line-strong);
    border-radius: 7px;
    background: var(--paper);
    color: var(--ink-mute);
    display: grid;
    place-items: center;
    cursor: pointer;
    z-index: 2;
  }
  .prompt-modal-close:hover { color: var(--ink-strong); background: var(--control-hover); }
  .prompt-modal-close:focus-visible { outline: 2px solid var(--accent); outline-offset: 1px; }

  /* ── Header bar ── */
  .prompt-head {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 11px 44px 11px 16px;
  }

  .prompt-head-info {
    display: flex;
    align-items: baseline;
    gap: 8px;
    min-width: 0;
  }

  .prompt-head-provider {
    font-family: var(--sans);
    font-size: 15px;
    font-weight: 600;
    letter-spacing: -0.01em;
    color: var(--ink);
    white-space: nowrap;
  }

  /* Prose now, not a model id — sans, per the mono-sparingly rule. */
  .prompt-head-model {
    font-family: var(--sans);
    font-size: 11px;
    color: var(--ink-mute);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .prompt-head-actions {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
  }

  /* ── Status chip — matches .summary-item vocabulary ── */
  .status-chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 8px;
    border-radius: 999px;
    font-family: var(--sans);
    font-size: 10.5px;
    font-weight: 500;
    letter-spacing: 0.02em;
    border: 1px solid var(--line-strong);
    color: var(--ink-soft);
    background: color-mix(in srgb, var(--paper) 50%, var(--bg-elev));
    cursor: default;
    transition: background 0.15s, color 0.15s, border-color 0.15s;
    white-space: nowrap;
  }

  .status-chip--clean {
    background: var(--success-bg);
    color: var(--success);
    border-color: var(--success-line);
  }
  .status-chip--warn {
    background: var(--warning-bg);
    color: var(--warning);
    border-color: var(--warning-line);
  }
  .status-chip--error,
  .status-chip--failed {
    background: var(--danger-bg);
    color: var(--danger);
    border-color: var(--danger-line);
  }
  .status-chip--testing {
    background: color-mix(in oklab, var(--accent) 10%, var(--paper));
    color: var(--accent-ink);
    border-color: color-mix(in oklab, var(--accent) 28%, transparent);
  }
  .status-chip--passed {
    background: var(--success-bg);
    color: var(--success);
    border-color: var(--success-line);
  }

  .chip-dot {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: currentColor;
    flex-shrink: 0;
  }

  .chip-spinner {
    width: 8px;
    height: 8px;
    border: 1.5px solid currentColor;
    border-top-color: transparent;
    border-radius: 50%;
    animation: spin 0.7s linear infinite;
    flex-shrink: 0;
  }

  @keyframes spin { to { transform: rotate(360deg); } }

  /* ── Buttons — match existing prompt-btn patterns ── */
  .prompt-btn {
    font-size: 12px;
    font-family: var(--sans);
    font-weight: 500;
    padding: 5px 12px;
    border: none;
    border-radius: 6px;
    background: var(--accent);
    color: var(--on-accent);
    cursor: pointer;
    transition: opacity 0.15s;
    white-space: nowrap;
  }
  .prompt-btn:disabled { opacity: 0.5; cursor: not-allowed; }
  .prompt-btn:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }

  .prompt-btn-ghost {
    font-size: 12px;
    font-family: var(--sans);
    font-weight: 500;
    padding: 5px 12px;
    border: 1px solid var(--line-strong);
    border-radius: 6px;
    background: transparent;
    color: var(--ink-soft);
    cursor: pointer;
    transition: background 0.15s, color 0.15s;
    white-space: nowrap;
  }
  .prompt-btn-ghost:hover:not(:disabled) { background: var(--bg-elev); color: var(--ink); }
  .prompt-btn-ghost:disabled { opacity: 0.5; cursor: not-allowed; }
  .prompt-btn-ghost:focus-visible { outline: 2px solid var(--accent); outline-offset: 1px; }

  /* ── Error / warning result ── */
  .prompt-result {
    flex-shrink: 0;
    padding: 8px 16px 10px;
    font-family: var(--sans);
    font-size: 12px;
    overflow: hidden;
    border-bottom: 1px solid transparent;
  }
  .prompt-result--fail {
    background: var(--danger-bg);
    color: var(--danger);
    border-color: var(--danger-line);
  }
  .prompt-result--warn {
    background: var(--warning-bg);
    color: var(--warning);
    border-color: var(--warning-line);
  }
  .prompt-result ul {
    margin: 0;
    padding-left: 16px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .prompt-result-actions {
    display: flex;
    justify-content: flex-end;
    margin-top: 8px;
  }

  /* ── Tags ── */
  .prompt-tags-row {
    flex-shrink: 0;
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    padding: 4px 16px 8px;
  }

  .prompt-tag {
    font-family: var(--mono);
    font-size: 10px;
    padding: 2px 6px;
    border-radius: 4px;
    background: var(--accent-soft);
    color: var(--accent-ink);
  }

  /* ── Editor ── */
  .prompt-editor-body {
    flex: 1;
    display: flex;
    min-height: 0;
    padding: 12px 16px 16px;
  }

  .prompt-loading {
    font-family: var(--sans);
    font-size: 13px;
    color: var(--ink-mute);
    align-self: center;
    margin: auto;
  }

  .prompt-textarea {
    flex: 1;
    font-family: var(--mono);
    font-size: 11.5px;
    line-height: 1.6;
    width: 100%;
    resize: none;
    padding: 10px 12px;
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    background: var(--paper);
    color: var(--ink);
    box-sizing: border-box;
    transition: border-color 0.15s;
  }
  .prompt-textarea:focus { outline: none; border-color: var(--accent); }
  .prompt-textarea:disabled { opacity: 0.6; cursor: not-allowed; }
</style>
