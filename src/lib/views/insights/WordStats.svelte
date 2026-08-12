<script lang="ts">
  import { slide } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { motionMs } from '../../motion';
  import { fmtNumber } from './helpers';
  import AnimatedNumber from './AnimatedNumber.svelte';
  import type { InsightsCleanup, InsightsTotals, InsightsWords } from './types';

  let {
    words,
    cleanup,
    totals,
  }: { words: InsightsWords; cleanup: InsightsCleanup; totals: InsightsTotals } = $props();

  const max = $derived(Math.max(...words.top.map((w) => w.count), 0));

  /* Negative means cleanup trimmed the dictation; positive means it grew
     (snippet expansions can outweigh filler removal). */
  const trimPct = $derived(
    cleanup.raw_words > 0 ? ((cleanup.clean_words - cleanup.raw_words) / cleanup.raw_words) * 100 : null
  );

  /*
   * The card truncates the longest word with CSS ellipsis by default — that
   * alone handles normal-length words. The click-to-expand is the safety net
   * for a malformed dictation (e.g. a run of CJK text with no spaces, which
   * counts as one "word"): a hard char cap keeps even that case from ever
   * rendering thousands of characters into the DOM.
   */
  const HARD_CHAR_CAP = 300;
  let wordExpanded = $state(false);
  const rawWord = $derived.by(() => {
    const w = words.longest_word ?? '';
    return w.length > HARD_CHAR_CAP ? `${w.slice(0, HARD_CHAR_CAP)}…` : w;
  });
</script>

<section class="card">
  <header class="card-head">
    <div>
      <h2 class="card-h">Your vocabulary</h2>
      <p class="card-sub">Most-used words, everyday filler excluded</p>
    </div>
  </header>

  <div class="split">
    {#if words.top.length > 0}
      <ul class="word-list">
        {#each words.top as entry}
          <li>
            <span class="word-bar" style:width={`${max > 0 ? Math.max(14, (entry.count / max) * 100) : 0}%`}>
              <span class="word">{entry.word}</span>
            </span>
            <span class="word-count">{fmtNumber(entry.count)}</span>
          </li>
        {/each}
      </ul>
    {:else}
      <p class="foot">Not enough dictation yet to find your favourite words.</p>
    {/if}

    <dl class="figures">
      <div>
        <dt>Unique words</dt>
        <dd><AnimatedNumber value={words.unique_words} /></dd>
      </div>
      <div>
        <dt>Average word length</dt>
        <dd>
          {#if words.avg_word_length > 0}
            <AnimatedNumber value={words.avg_word_length} format={(n) => `${n.toFixed(1)} chars`} />
          {:else}
            —
          {/if}
        </dd>
      </div>
      <div class="longest-word-field">
        <dt>Longest word</dt>
        {#if !rawWord}
          <dd class="dd-word">—</dd>
        {:else}
          <dd class="dd-word">
            <button
              type="button"
              class="word-toggle"
              class:open={wordExpanded}
              aria-expanded={wordExpanded}
              onclick={() => (wordExpanded = !wordExpanded)}
              title={wordExpanded ? 'Click to collapse' : 'Click to see the full word'}
            >
              <span class="word-toggle-text">{rawWord}</span>
              <svg class="word-chevron" width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                <path d="m6 9 6 6 6-6"/>
              </svg>
            </button>
            {#if wordExpanded}
              <div class="word-full scroll-styled" transition:slide={{ duration: motionMs(220), easing: cubicOut }}>
                {rawWord}
              </div>
            {/if}
          </dd>
        {/if}
      </div>
      <div>
        <dt>Words per dictation</dt>
        <dd><AnimatedNumber value={totals.avg_words_per_transcription} /></dd>
      </div>
      <div>
        <dt>Cleanup effect</dt>
        <dd>
          {#if trimPct === null}
            —
          {:else if trimPct < 0}
            trimmed <AnimatedNumber value={Math.abs(trimPct)} format={(n) => `${n.toFixed(1)}%`} />
          {:else}
            grew <AnimatedNumber value={trimPct} format={(n) => `${n.toFixed(1)}%`} />
          {/if}
        </dd>
      </div>
      <div>
        <dt>Dictations</dt>
        <dd><AnimatedNumber value={totals.total_transcriptions} /></dd>
      </div>
    </dl>
  </div>
</section>

<style>
  /* .card / .card-head / .card-h / .card-sub are owned by Insights.svelte. */

  .split {
    display: grid;
    grid-template-columns: minmax(0, 1.4fr) minmax(0, 1fr);
    gap: 24px;
    align-items: start;
  }

  .word-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
  }
  .word-list li {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }

  .word-bar {
    display: flex;
    align-items: center;
    height: 24px;
    padding: 0 9px;
    border-radius: 5px;
    background: color-mix(in srgb, var(--accent) 30%, transparent);
    min-width: 0;
    overflow: hidden;
  }

  .word {
    font-size: 11.5px;
    color: var(--ink);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .word-count {
    font-size: 11.5px;
    color: var(--ink-mute);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  .figures {
    margin: 0;
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 14px 18px;
  }
  .figures div { min-width: 0; }
  .figures dt {
    font-size: 10.5px;
    letter-spacing: 0.05em;
    text-transform: uppercase;
    color: var(--ink-mute);
    margin-bottom: 4px;
  }
  .figures dd {
    margin: 0;
    font-family: var(--serif);
    font-size: 18px;
    font-weight: 500;
    color: var(--ink);
    line-height: 1.15;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .dd-word { font-size: 15px; }

  .longest-word-field { overflow: hidden; }

  .word-toggle {
    display: flex;
    align-items: center;
    gap: 6px;
    max-width: 100%;
    background: none;
    border: 0;
    padding: 0;
    margin: 0;
    font: inherit;
    cursor: pointer;
    color: var(--ink);
    text-align: left;
  }
  .word-toggle-text {
    min-width: 0;
    flex: 1;
    font-family: var(--serif);
    font-size: 15px;
    font-weight: 500;
    color: inherit;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    border-bottom: 1px dashed var(--line-strong);
    transition: color var(--ui-duration-fast) var(--ui-ease-out), border-color var(--ui-duration-fast) var(--ui-ease-out);
  }
  .word-toggle:hover .word-toggle-text {
    color: var(--accent-ink);
    border-color: var(--accent-ink);
  }
  .word-chevron {
    flex: 0 0 auto;
    color: var(--ink-mute);
    transition: transform var(--ui-duration-base) var(--ui-ease-out);
  }
  .word-toggle.open .word-chevron { transform: rotate(180deg); }

  .word-full {
    margin-top: 8px;
    padding-right: 6px;
    max-height: 130px;
    overflow-y: auto;
    font-family: var(--serif);
    font-size: 14px;
    font-weight: 500;
    color: var(--ink);
    line-height: 1.45;
    word-break: break-word;
  }

  .foot {
    margin: 0;
    font-size: 11.5px;
    color: var(--ink-mute);
  }

  @media (max-width: 860px) {
    .split { grid-template-columns: 1fr; gap: 20px; }
  }
</style>
