<script lang="ts">
  import { onMount } from 'svelte';
  import { transcriptionLanguages, type TranscriptionLanguageCode } from '../../transcriptionLanguages';
  import { focusListboxOption, handleListboxOptionKeydown } from '../../components/appMappings/listbox';

  let { language = $bindable() }: { language: TranscriptionLanguageCode } = $props();

  const LANG_MENU_ID = 'lang-listbox';

  let query = $state('');
  let listEl = $state<HTMLDivElement | null>(null);
  let searchInput = $state<HTMLInputElement | null>(null);

  // Common dictation languages float to the top of an otherwise A–Z list, so
  // the usual answer is one click away without hiding the other 51.
  const commonCodes = new Set<TranscriptionLanguageCode>(['en', 'es', 'fr', 'de', 'pt', 'zh']);
  const ordered = [
    ...transcriptionLanguages.filter((l) => commonCodes.has(l.code)),
    ...transcriptionLanguages.filter((l) => !commonCodes.has(l.code)),
  ];

  const filtered = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (!q) return ordered;
    return ordered.filter((l) => l.label.toLowerCase().includes(q) || l.code.includes(q));
  });

  const selectedLabel = $derived(
    transcriptionLanguages.find((l) => l.code === language)?.label ?? 'English'
  );

  function pick(code: TranscriptionLanguageCode) {
    language = code;
  }

  // Land keyboard focus on the selected (or first) option whenever the list
  // (re)appears, and send Escape/arrow keys back to the search box.
  onMount(() => {
    void focusListboxOption(LANG_MENU_ID);
  });

  function restoreToSearchOrTrigger() {
    searchInput?.focus();
  }

  function clearSearch() {
    query = '';
    void focusListboxOption(LANG_MENU_ID);
  }

  // Enter on the search box takes the only remaining match — faster than
  // reaching for the mouse once you have typed enough to narrow it down.
  function onSearchKey(event: KeyboardEvent) {
    if (event.key === 'Enter' && !event.isComposing && filtered.length === 1) {
      event.preventDefault();
      pick(filtered[0].code);
      listEl?.scrollTo({ top: 0 });
    }
  }
</script>

<div class="step language-step">
  <div class="lang-panel">
    <div class="lang-search">
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true"><circle cx="11" cy="11" r="7"/><line x1="21" y1="21" x2="16.65" y2="16.65"/></svg>
      <input
        bind:this={searchInput}
        class="lang-search-input"
        type="text"
        bind:value={query}
        onkeydown={onSearchKey}
        placeholder="Search {transcriptionLanguages.length} languages…"
        aria-label="Search languages"
        spellcheck="false"
        autocomplete="off"
      />
      {#if query}
        <button class="lang-clear ui-focus-ring" onclick={clearSearch} aria-label="Clear search">
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
        </button>
      {/if}
    </div>

    <!-- data-scroll-region: scrolling is this control's design, not a layout
         overflow — see tests/manual/setup-layout-bounds.js -->
    <div
      id={LANG_MENU_ID}
      class="lang-list scroll-styled"
      data-scroll-region
      bind:this={listEl}
      role="listbox"
      aria-label="Dictation language"
      tabindex="-1"
    >
      {#each filtered as lang (lang.code)}
        <button
          class="lang-row"
          class:selected={language === lang.code}
          role="option"
          aria-selected={language === lang.code}
          tabindex={language === lang.code || (!filtered.some((item) => item.code === language) && lang === filtered[0]) ? 0 : -1}
          onclick={() => pick(lang.code)}
          onkeydown={(event) => handleListboxOptionKeydown(event, LANG_MENU_ID, restoreToSearchOrTrigger)}
        >
          <span class="lang-name">{lang.label}</span>
          <span class="lang-code">{lang.code}</span>
          <div class="pick-radio" class:checked={language === lang.code}></div>
        </button>
      {:else}
        <p class="lang-empty">No language matches "{query}".</p>
      {/each}
    </div>
  </div>

  <p class="lang-note">
    Verenu will expect to hear <strong>{selectedLabel}</strong> when you dictate.
    This does not change the language of the app itself — the interface stays in English.
  </p>
</div>

<style>
  .language-step { gap: 12px; }

  .lang-panel {
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    background: var(--bg-elev);
    overflow: hidden;
  }

  .lang-search {
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 10px 13px;
    border-bottom: 1px solid var(--line);
    color: var(--ink-faint);
  }

  .lang-search:focus-within { color: var(--accent); }

  .lang-search-input {
    flex: 1;
    min-width: 0;
    border: none;
    background: transparent;
    outline: none;
    font-family: var(--sans);
    font-size: 13.5px;
    color: var(--ink);
  }

  .lang-search-input::placeholder { color: var(--ink-faint); }

  .lang-clear {
    background: none;
    border: none;
    padding: 2px;
    display: flex;
    color: var(--ink-faint);
    cursor: pointer;
    transition: color 0.15s;
  }

  .lang-clear:hover { color: var(--ink-strong); }

  .lang-list {
    height: 232px;
    overflow-y: auto;
    padding: 4px;
  }

  .lang-row {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 10px;
    border: none;
    border-radius: var(--r-sm);
    background: transparent;
    font-family: var(--sans);
    text-align: left;
    cursor: pointer;
    transition: background 0.14s ease;
  }

  .lang-row:hover { background: var(--paper-2); }
  .lang-row.selected { background: var(--accent-soft); }
  .lang-row:focus-visible { outline: 2px solid var(--accent); outline-offset: -2px; }

  .lang-name { flex: 1; min-width: 0; font-size: 13px; color: var(--ink-soft); }
  .lang-row.selected .lang-name { color: var(--ink-strong); font-weight: 500; }

  .lang-code {
    font-family: var(--mono);
    font-size: 10.5px;
    text-transform: uppercase;
    color: var(--ink-faint);
    letter-spacing: 0.04em;
  }

  .lang-empty { margin: 0; padding: 18px 12px; font-size: 12.5px; color: var(--ink-faint); text-align: center; }

  .lang-note { margin: 0; font-size: 12px; color: var(--ink-mute); line-height: 1.55; }
  .lang-note strong { color: var(--ink-soft); font-weight: 600; }

  @media (max-height: 660px) {
    .lang-list { height: 196px; }
    .lang-row { padding-block: 6px; }
  }
</style>
