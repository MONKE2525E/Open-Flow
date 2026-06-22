// Smoke test: UI navigation & interaction — Tauri window (port 1420)
// Verifies nav routing, settings open/close, and toggle state changes.
const { chromium } = require('playwright');
const { tauriMock, APP_VERSION } = require('./_tauri-mock.cjs');

const TARGET_URL = 'http://localhost:1420';
const TIMEOUT = 8_000;

async function waitForIntermediateOpacity(page, selector, label, timeout = 1_000) {
  const handle = await page.waitForFunction(
    ({ selector }) => {
      const el = document.querySelector(selector);
      if (!el) return false;
      const opacity = Number.parseFloat(getComputedStyle(el).opacity);
      return opacity > 0 && opacity < 1 ? opacity : false;
    },
    { selector },
    { timeout },
  );
  const opacity = await handle.jsonValue();
  console.log(`  ✓ ${label} (${Number(opacity).toFixed(2)})`);
  return opacity;
}

async function waitForSingleSettingsPanel(page) {
  await page.waitForFunction(
    () => document.querySelectorAll('.settings-body .panel').length === 1,
    null,
    { timeout: 2_000 },
  );
}

(async () => {
  console.log('Starting UI interaction tests...');
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage();
  const errors = [];

  await page.addInitScript(tauriMock, { appVersion: APP_VERSION });

  page.on('pageerror', err => errors.push(`Page exception: ${err.message}`));
  page.on('console', msg => {
    if (msg.type() === 'error') errors.push(`Console error: ${msg.text()}`);
  });

  try {
    await page.goto(TARGET_URL, { waitUntil: 'networkidle', timeout: TIMEOUT });
    console.log('Page loaded.');

    // ── Navigation: page swaps should overlap during transition ───────────────
    await page.locator('h1.page-h:has-text("Welcome back")').waitFor({ state: 'visible', timeout: 3_000 });
    const loadOlder = page.getByRole('button', { name: 'Load older' });
    await loadOlder.waitFor({ state: 'visible', timeout: 3_000 });
    await loadOlder.click();
    await page.waitForFunction(
      () => !document.body.textContent?.includes('Load older'),
      null,
      { timeout: 3_000 },
    );
    console.log('  âœ“ Home history pagination loads the final page');
    await page.locator('.nav-item:has-text("Dictionary")').click();
    const wrapperCountHandle = await page.waitForFunction(
      () => document.querySelectorAll('.page-wrapper').length >= 2
        ? document.querySelectorAll('.page-wrapper').length
        : false,
      null,
      { timeout: 1_000 },
    ).catch(() => null);
    const wrapperCount = wrapperCountHandle ? await wrapperCountHandle.jsonValue() : 0;
    if (wrapperCount < 2) {
      errors.push(`Expected overlapping page wrappers during nav transition, saw ${wrapperCount}`);
    } else {
      const outgoingHidden = await page.locator('.page-wrapper').first().evaluate((el) => el.inert && el.getAttribute('aria-hidden') === 'true');
      if (!outgoingHidden) {
        errors.push('Outgoing page wrapper should be inert and aria-hidden during nav transition');
      } else {
        const incomingOpacity = await waitForIntermediateOpacity(page, '.page-wrapper:last-child', 'Incoming page fades in during nav transition');
        console.log(`  ✓ Outgoing page wrapper is inert during transition; incoming opacity ${Number(incomingOpacity).toFixed(2)}`);
      }
    }
    await page.locator('h1.page-h:has-text("Dictionary")').waitFor({ state: 'visible', timeout: 3_000 });

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

    await waitForIntermediateOpacity(page, '.settings-overlay', 'Settings backdrop fades in').catch((err) => {
      errors.push(`Settings backdrop should pass through a mid-fade opacity on open: ${err.message}`);
    });

    const modal = page.locator('.settings-modal');
    await modal.waitFor({ state: 'visible', timeout: 3_000 });
    console.log('  ✓ Settings modal opened');
    const activeInModal = await modal.evaluate((el) => el.contains(document.activeElement));
    if (!activeInModal) {
      errors.push('Settings modal did not move focus inside the dialog on open');
    }

    const versionFoot = await page.locator('.settings-foot').textContent();
    if (!versionFoot?.includes(APP_VERSION)) {
      errors.push(`Settings footer version mismatch: "${versionFoot?.trim()}"`);
    } else {
      console.log(`  ✓ Settings footer shows ${versionFoot.trim()}`);
    }

    const offlineToast = page.locator('.offline-toast');
    if (await offlineToast.isVisible().catch(() => false)) {
      const [overlayZ, toastZ] = await Promise.all([
        page.locator('.settings-overlay-wrap').evaluate((el) => Number(getComputedStyle(el).zIndex) || 0),
        offlineToast.evaluate((el) => Number(getComputedStyle(el).zIndex) || 0),
      ]);
      if (overlayZ <= toastZ) {
        errors.push(`Settings overlay z-index (${overlayZ}) should sit above offline toast (${toastZ})`);
      } else {
        console.log(`  ✓ Settings overlay stacks above offline toast (${overlayZ} > ${toastZ})`);
      }
    }

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

    // ── General language should not localize unrelated UI copy ───────────────
    await page.locator('.settings-nav-item:has-text("General")').click();
    await page.locator('h2.settings-h:has-text("General")').waitFor({ state: 'visible', timeout: 3_000 });
    await waitForSingleSettingsPanel(page);
    await page.locator('.language-btn').click();
    await page.locator('.language-menu').waitFor({ state: 'visible', timeout: 2_000 });
    await page.locator('.language-item').filter({ hasText: 'Chinese' }).first().click();
    await page.locator('.language-btn:has-text("Chinese")').waitFor({ state: 'visible', timeout: 2_000 });
    if (!(await page.locator('.label', { hasText: 'Input device' }).isVisible().catch(() => false))) {
      errors.push('Input device label disappeared after changing Spoken Language to Chinese');
    } else {
      console.log('  ✓ Input device label stays in English after language change');
    }
    if (await page.locator('text=输入设备').isVisible().catch(() => false)) {
      errors.push('Chinese microphone label leaked into General settings after Spoken Language change');
    }

    // ── Privacy toggles: verify state actually changes on click ───────────────
    console.log('  Testing Privacy toggles...');
    await page.locator('.settings-nav-item:has-text("Privacy")').click();
    await page.locator('h2.settings-h:has-text("Privacy")').waitFor({ state: 'visible', timeout: 3_000 });
    await waitForSingleSettingsPanel(page);

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
    const retentionButton = page.getByRole('button', { name: 'Transcription history retention' });
    await retentionButton.click();
    await page.getByRole('option', { name: '7 days' }).click();
    const retentionModal = page.locator('.modal-card[aria-labelledby="retention-confirm-title"]');
    await retentionModal.waitFor({ state: 'visible', timeout: 2_000 });
    const retentionFocusInside = await retentionModal.evaluate((el) => el.contains(document.activeElement));
    if (!retentionFocusInside) {
      errors.push('Privacy retention confirmation dialog did not move focus inside the dialog');
    } else {
      console.log('  âœ“ Privacy retention confirmation moves focus inside the dialog');
    }
    await page.getByRole('button', { name: 'Cancel' }).click();
    await retentionModal.waitFor({ state: 'hidden', timeout: 2_000 });
    const retentionFocusRestored = await retentionButton.evaluate((el) => document.activeElement === el);
    if (!retentionFocusRestored) {
      errors.push('Privacy retention confirmation did not restore focus to the trigger');
    } else {
      console.log('  âœ“ Privacy retention confirmation restores focus to the trigger');
    }

    console.log('  Closing Settings via outside click...');
    await page.mouse.click(10, 10);
    await waitForIntermediateOpacity(page, '.settings-overlay', 'Settings backdrop fades out').catch((err) => {
      errors.push(`Settings backdrop should pass through a mid-fade opacity on close: ${err.message}`);
    });
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
