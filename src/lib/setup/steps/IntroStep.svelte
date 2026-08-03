<script lang="ts">
  import { onMount } from 'svelte';
  import { isMac } from '../../platform';
  import LogoMark from '../../components/layout/LogoMark.svelte';

  const hkKey1 = isMac ? 'fn' : 'Ctrl';
  const hkKey2 = isMac ? 'Control' : 'Windows';
  const platformTagline = isMac ? 'macOS' : 'Windows';

  let introReady = $state(false);
  onMount(() => {
    const t = setTimeout(() => { introReady = true; }, 60);
    return () => clearTimeout(t);
  });
</script>

<div class="step intro-step">
  <div class="intro-brand" class:ready={introReady}>
    <div class="intro-lockup">
      <div class="intro-mark">
        <LogoMark />
      </div>
      <div class="intro-wordmark">
        <h1 class="brand-name">Verenu</h1>
        <p class="brand-tagline">open-source AI dictation for {platformTagline}</p>
      </div>
    </div>
  </div>

  <div class="how-it-works" class:ready={introReady}>
    <p class="how-label">How it works</p>
    <div class="how-steps">
      <div class="how-step">
        <div class="how-num">1</div>
        <div>
          <strong>Hold <kbd>{hkKey1}</kbd> + <kbd>{hkKey2}</kbd></strong>
          <p>Start recording. A floating pill shows your audio level.</p>
        </div>
      </div>
      <div class="how-step">
        <div class="how-num">2</div>
        <div>
          <strong>Release to transcribe</strong>
          <p>Your speech is sent to the AI provider and converted to text.</p>
        </div>
      </div>
      <div class="how-step">
        <div class="how-num">3</div>
        <div>
          <strong>Text appears instantly</strong>
          <p>Cleaned text is injected into whatever app you're focused on.</p>
        </div>
      </div>
    </div>
  </div>

  <p class="intro-note" class:ready={introReady}>Takes about 2 minutes · You can change anything later</p>
</div>

<style>
  .intro-step {
    align-items: center;
    text-align: center;
    max-width: 480px;
  }

  .intro-brand {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    opacity: 0;
    transform: translateY(14px);
    transition: opacity 0.5s ease, transform 0.5s ease;
  }

  .intro-brand.ready { opacity: 1; transform: none; }

  .intro-lockup { display: flex; align-items: center; gap: 16px; }

  .intro-mark {
    width: 48px;
    height: 40px;
    flex-shrink: 0;
    color: var(--accent);
  }

  .intro-mark :global(svg) { display: block; }

  .intro-wordmark { display: flex; flex-direction: column; gap: 2px; text-align: left; }

  .brand-name {
    font-family: var(--serif);
    font-size: 28px;
    font-weight: 500;
    color: var(--ink-strong);
    margin: 0;
    letter-spacing: -0.3px;
    line-height: 1.1;
  }

  .brand-tagline { font-size: 13px; color: var(--ink-mute); margin: 0; line-height: 1.3; }

  .how-it-works {
    background: var(--paper-2);
    border: 1px solid var(--line);
    border-radius: var(--r-lg);
    padding: 18px 22px;
    text-align: left;
    opacity: 0;
    transform: translateY(10px);
    transition: opacity 0.5s 0.15s ease, transform 0.5s 0.15s ease;
  }

  .how-it-works.ready { opacity: 1; transform: none; }

  .how-label {
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--ink-faint);
    margin: 0 0 14px;
  }

  .how-steps { display: flex; flex-direction: column; gap: 14px; }

  .how-step { display: flex; gap: 14px; align-items: flex-start; }

  .how-num {
    width: 24px;
    height: 24px;
    border-radius: 50%;
    background: var(--accent-soft);
    color: var(--accent-ink);
    font-size: 12px;
    font-weight: 600;
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    margin-top: 1px;
  }

  .how-step strong { display: block; font-size: 13px; color: var(--ink-soft); margin-bottom: 2px; }
  .how-step p { margin: 0; font-size: 12.5px; color: var(--ink-mute); line-height: 1.4; }

  .intro-note {
    font-size: 12px;
    color: var(--ink-faint);
    margin: 0;
    opacity: 0;
    transform: translateY(8px);
    transition: opacity 0.5s 0.28s ease, transform 0.5s 0.28s ease;
  }

  .intro-note.ready { opacity: 1; transform: none; }
</style>
