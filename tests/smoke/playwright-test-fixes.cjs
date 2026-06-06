// Smoke test: element contract assertions — Tauri window (port 1420)
// Verifies exact tag + class combos for critical UI elements.
const { chromium } = require('playwright');
const { tauriMock } = require('./_tauri-mock.cjs');

const TARGET_URL = 'http://localhost:1420';
const TIMEOUT = 8_000;

async function assert(label, locator, errors) {
  try {
    await locator.waitFor({ state: 'visible', timeout: 3_000 });
    console.log(`  ✓ ${label}`);
  } catch {
    errors.push(`${label} — not found or not visible`);
    console.error(`  ✗ ${label}`);
  }
}

(async () => {
  console.log('Starting element contract assertions...');
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage();
  const errors = [];

  await page.addInitScript(tauriMock);

  try {
    await page.goto(TARGET_URL, { waitUntil: 'networkidle', timeout: TIMEOUT });

    // Open Settings
    const settingsBtn = page.locator('.nav-item:has-text("Settings")');
    await settingsBtn.waitFor({ state: 'visible', timeout: TIMEOUT });
    await settingsBtn.click();
    await page.locator('.settings-modal').waitFor({ state: 'visible', timeout: 3_000 });
    console.log('Settings opened.');

    // ── Microphone tab ────────────────────────────────────────────────────────
    console.log('Checking Microphone tab...');
    await page.locator('.settings-nav-item:has-text("Microphone")').click();
    await page.locator('h2.settings-h:has-text("Microphone")').waitFor({ state: 'visible', timeout: 3_000 });

    // At least one toggle must exist in Microphone
    await assert(
      '.toggle (aria switch) in Microphone',
      page.locator('.toggle[role="switch"]').first(),
      errors,
    );

    // Gain value display must render as "X.X×"
    await assert(
      'span.gain-value showing mic gain',
      page.locator('span.gain-value'),
      errors,
    );

    // ── General tab ──────────────────────────────────────────────────────────
    console.log('Checking General tab hotkey badge...');
    await page.locator('.settings-nav-item:has-text("General")').click();
    await page.locator('h2.settings-h:has-text("General")').waitFor({ state: 'visible', timeout: 3_000 });

    // Hotkey is button.badge.key-badge showing "Ctrl + Windows"
    await assert(
      'button.badge.key-badge containing "Ctrl"',
      page.locator('button.badge.key-badge:has-text("Ctrl")'),
      errors,
    );

    // Verify the exact text includes the separator
    const hotkeyBtn = page.locator('button.badge.key-badge');
    await hotkeyBtn.waitFor({ state: 'visible', timeout: 2_000 });
    const hotkeyText = await hotkeyBtn.textContent();
    if (!hotkeyText?.includes('+')) {
      errors.push(`Hotkey badge text "${hotkeyText}" missing expected "+" separator`);
    } else {
      console.log(`  ✓ Hotkey text: "${hotkeyText?.trim()}"`);
    }

    // ── About tab ────────────────────────────────────────────────────────────
    console.log('Checking About tab GitHub button...');
    await page.locator('.settings-nav-item:has-text("About")').click();
    await page.locator('h2.settings-h:has-text("About")').waitFor({ state: 'visible', timeout: 3_000 });

    await assert(
      'button.btn-ghost containing "github.com/MONKE2525E/Open-Flow"',
      page.locator('button.btn-ghost:has-text("github.com/MONKE2525E/Open-Flow")'),
      errors,
    );

    // ── Final verdict ─────────────────────────────────────────────────────────
    if (errors.length > 0) {
      console.error('\nFAIL — missing or wrong elements:');
      errors.forEach(e => console.error('  ' + e));
      process.exit(1);
    }
    console.log('\nPASS — all element contracts satisfied.');
  } catch (err) {
    console.error('FAIL — test threw:', err.message);
    process.exit(1);
  } finally {
    await browser.close();
  }
})();
