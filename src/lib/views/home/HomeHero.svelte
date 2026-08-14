<script lang="ts">
  export let hk1: string;
  export let hk2: string;
</script>

<div class="hero-photo">
  <div class="hero-photo-content">
    <h2 class="hero-photo-title">
      Hold <kbd>{hk1}</kbd> <kbd>{hk2}</kbd> to dictate
    </h2>
    <p class="hero-photo-sub">
      Verenu works in any app. Try it in
      <em class="hero-em">email, messages, docs</em> â€” or anywhere else.
    </p>
  </div>
</div>

<style>
  .hero-photo {
    position: relative;
    border-radius: var(--r-lg);
    overflow: hidden;
    margin-bottom: 16px;
    height: clamp(112px, 14vw, 160px);
    background: var(--arm-950);
  }

  .hero-photo::before {
    content: '';
    position: absolute;
    right: -80px; top: -80px;
    width: 360px; height: 360px;
    background: radial-gradient(circle, rgba(217,119,87,0.22) 0%, transparent 60%);
    pointer-events: none;
  }

  .hero-photo-content {
    position: relative;
    padding: 22px 28px;
    max-width: min(520px, 100%);
  }

  .hero-photo-title {
    font-family: var(--serif);
    font-size: 22px;
    font-weight: 500;
    letter-spacing: -0.02em;
    margin: 0 0 8px;
    line-height: 1.15;
    color: var(--pill-fg);
  }

  .hero-photo-title :global(kbd) {
    background: color-mix(in srgb, var(--pill-fg) 10%, transparent);
    border: 1px solid color-mix(in srgb, var(--pill-fg) 18%, transparent);
    border-radius: 5px;
    font-family: var(--mono);
    font-size: 13px;
    padding: 1px 6px;
    color: var(--jap-300);
    font-weight: 500;
  }

  .hero-photo-sub {
    font-size: 12.5px;
    color: color-mix(in srgb, var(--pill-fg) 58%, transparent);
    margin: 0;
    line-height: 1.5;
  }

  .hero-em {
    font-family: var(--serif);
    font-style: italic;
    color: color-mix(in srgb, var(--pill-fg) 82%, transparent);
  }

  @media (max-width: 720px) {
    .hero-photo-content {
      padding: 18px 20px;
    }

    .hero-photo-title {
      font-size: 20px;
    }
  }
</style>
