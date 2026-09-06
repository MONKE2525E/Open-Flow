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
      <em class="hero-em">email, messages, docs</em> &mdash; or anywhere else.
    </p>
  </div>
</div>

<style>
  .hero-photo {
    position: relative;
    border-radius: var(--r-lg);
    overflow: hidden;
    margin-bottom: 16px;
    height: clamp(104px, 12vw, 136px);
    background: var(--arm-950);
  }

  .hero-photo::before {
    content: '';
    position: absolute;
    right: -80px; top: -80px;
    width: 360px; height: 360px;
    background: radial-gradient(circle, color-mix(in srgb, var(--hero-accent) 22%, transparent) 0%, transparent 60%);
    pointer-events: none;
  }

  .hero-photo-content { position: relative; padding: 18px 24px; max-width: min(500px, 100%); }

  .hero-photo-title {
    font-family: var(--sans);
    font-size: 19px;
    font-weight: 500;
    letter-spacing: -0.02em;
    margin: 0 0 8px;
    line-height: 1.15;
    color: #ffffff;
  }

  .hero-photo-title :global(kbd) {
    background: rgb(255 255 255 / 10%);
    border: 1px solid rgb(255 255 255 / 18%);
    border-radius: 5px;
    font-family: var(--mono);
    font-size: 13px;
    padding: 1px 6px;
    color: var(--hero-accent);
    font-weight: 500;
  }

  .hero-photo-sub {
    font-size: 12.5px;
    color: rgb(255 255 255 / 70%);
    margin: 0;
    line-height: 1.5;
  }

  .hero-em {
    font-family: var(--sans);
    font-style: italic;
    color: rgb(255 255 255 / 90%);
  }

  @media (max-width: 720px) {
    .hero-photo-content { padding: 18px 20px; }
    .hero-photo-title { font-size: 18px; }
  }
</style>
