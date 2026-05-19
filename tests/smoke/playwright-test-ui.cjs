// Smoke test: UI navigation & interaction — Tauri window (port 1420)
// Verifies nav routing, settings open/close, and toggle state changes.
const { chromium } = require('playwright');
const { tauriMock } = require('./_tauri-mock.cjs');

const TARGET_URL = 'http://localhost:1420';
const TIMEOUT = 8_000;

(async () => {
  console.log('Starting UI interaction tests...');
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage();
  const errors = [];

  await page.addInitScript(tauriMock);

  page.on('pageerror', err => errors.push(`Page exception: ${err.message}`));
  page.on('console', msg => {
    if (msg.type() === 'error') errors.push(`Console error: ${msg.text()}`);
  });

  try {
    await page.goto(TARGET_URL, { waitUntil: 'networkidle', timeout: TIMEOUT });
    console.log('Page loaded.');

    // ── Navigation: each click must actually change the visible view ──────────
    const navMap = [
      { label: 'Home',       heading: 'Welcome back' },
      { label: 'Dictionary', heading: 'Dictionary'   },
      { label: 'Snippets',   heading: 'Snippets'     },
      { label: 'Style',      heading: 'Style'        },
    ];

    for (const { label, heading } of navMap) {
      console.log(`Clicking nav: ${label}`);
      const btn = page.locator(`.nav-item:has-text("${label}")`);
      await btn.waitFor({ state: 'visible', timeout: TIMEOUT });
      await btn.click();

      // The view must render its h1.page-h with the expected text
      const h1 = page.locator(`h1.page-h:has-text("${heading}")`);
      await h1.waitFor({ state: 'visible', timeout: 3_000 });
      console.log(`  ✓ ${label} view rendered heading "${heading}"`);
    }

    // ── Settings: open ────────────────────────────────────────────────────────
    console.log('Opening Settings...');
    const settingsBtn = page.locator('.nav-item:has-text("Settings")');
    await settingsBtn.waitFor({ state: 'visible', timeout: TIMEOUT });
    await settingsBtn.click();

    const modal = page.locator('.settings-modal');
    await modal.waitFor({ state: 'visible', timeout: 3_000 });
    console.log('  ✓ Settings modal opened');

    // ── Settings sections: each click must show the correct h2 ────────────────
    const sections = ['General', 'API Keys', 'Models', 'Privacy', 'Microphone', 'About'];
    for (const sec of sections) {
      console.log(`  Clicking Settings section: ${sec}`);
      const secBtn = page.locator(`.settings-nav-item:has-text("${sec}")`);
      await secBtn.waitFor({ state: 'visible', timeout: TIMEOUT });
      await secBtn.click();

      const h2 = page.locator(`h2.settings-h:has-text("${sec}")`);
      await h2.waitFor({ state: 'visible', timeout: 3_000 });
      console.log(`    ✓ "${sec}" panel rendered`);
    }

    // ── Privacy toggles: verify state actually changes on click ───────────────
    console.log('  Testing Privacy toggles...');
    await page.locator('.settings-nav-item:has-text("Privacy")').click();
    await page.locator('h2.settings-h:has-text("Privacy")').waitFor({ state: 'visible', timeout: 3_000 });
    // Wait for the previous section's out-transition (350 ms) to fully complete
    // so we don't grab toggles that are animating out and about to detach.
    await page.waitForTimeout(450);

    const toggles = await page.locator('.toggle').all();
    if (toggles.length === 0) {
      errors.push('Privacy section has no .toggle elements');
    } else {
      console.log(`  Found ${toggles.length} toggle(s) on Privacy panel`);
      for (let i = 0; i < toggles.length; i++) {
        const before = await toggles[i].getAttribute('aria-checked');
        await toggles[i].click();
        const after = await toggles[i].getAttribute('aria-checked');
        if (before === after) {
          errors.push(`Toggle ${i} did not change aria-checked (stuck at "${before}")`);
        } else {
          console.log(`    ✓ Toggle ${i}: ${before} → ${after}`);
        }
        // Restore original state
        await toggles[i].click();
      }
    }

    // ── Settings: close by clicking outside (10, 10) ─────────────────────────
    console.log('  Closing Settings via outside click...');
    await page.mouse.click(10, 10);
    await modal.waitFor({ state: 'hidden', timeout: 3_000 });
    console.log('  ✓ Settings modal closed');

    // ── Final verdict ─────────────────────────────────────────────────────────
    if (errors.length > 0) {
      console.error('\nFAIL — errors:');
      errors.forEach(e => console.error('  ' + e));
      process.exit(1);
    }
    console.log('\nPASS — all UI interaction tests passed.');
  } catch (err) {
    console.error('FAIL — test threw:', err.message);
    process.exit(1);
  } finally {
    await browser.close();
  }
})();
