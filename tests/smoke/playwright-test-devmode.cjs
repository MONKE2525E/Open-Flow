const { chromium } = require('playwright');
const { tauriMock } = require('./_tauri-mock.cjs');

const TARGET_URL = 'http://localhost:1420';
const TIMEOUT = 8_000;

(async () => {
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

    await page.locator('.nav-item:has-text("Settings")').click();
    await page.locator('.settings-modal').waitFor({ state: 'visible', timeout: TIMEOUT });

    const devTabBefore = page.locator('.settings-nav-item:has-text("Developer")');
    if (await devTabBefore.count() > 0) {
      errors.push('Developer tab should not be visible before unlock.');
    }

    await page.locator('.settings-nav-item:has-text("About")').click();
    const versionBtn = page.locator('.version-tap');
    await versionBtn.waitFor({ state: 'visible', timeout: TIMEOUT });
    for (let i = 0; i < 10; i++) {
      await versionBtn.click();
    }

    await page.locator('.settings-nav-item:has-text("Developer")').waitFor({ state: 'visible', timeout: TIMEOUT });
    await page.locator('.settings-nav-item:has-text("Developer")').click();
    await page.locator('h2.settings-h:has-text("Developer")').waitFor({ state: 'visible', timeout: TIMEOUT });

    if (errors.length > 0) {
      console.error('\nFAIL - errors:');
      errors.forEach(e => console.error('  ' + e));
      process.exit(1);
    }
    console.log('PASS - dev mode unlock smoke test passed.');
  } catch (err) {
    console.error('FAIL - test threw:', err.message);
    process.exit(1);
  } finally {
    await browser.close();
  }
})();
