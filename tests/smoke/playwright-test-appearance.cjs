// Smoke test: appearance mode persistence and live theme application.
const { chromium } = require('playwright');
const { tauriMock, APP_VERSION } = require('./_tauri-mock.cjs');

const TARGET_URL = 'http://localhost:1420';
const TIMEOUT = 8_000;

(async () => {
  console.log('Starting appearance tests...');
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

    const initialTheme = await page.locator('html').getAttribute('data-theme');
    if (!['light', 'dark'].includes(initialTheme)) {
      errors.push(`Initial data-theme was not resolved: "${initialTheme}"`);
    }

    await page.evaluate(async () => {
      await window.__TAURI_INTERNALS__.invoke('save_setting', { key: 'appearance_mode', value: 'dark' });
    });
    await page.reload({ waitUntil: 'networkidle', timeout: TIMEOUT });
    await page.locator('html[data-theme="dark"]').waitFor({ state: 'attached', timeout: 3_000 });

    const settingsBtn = page.locator('.nav-item:has-text("Settings")');
    await settingsBtn.click();
    await page.locator('.settings-modal').waitFor({ state: 'visible', timeout: 3_000 });

    const lightOption = page.locator('.appearance-option:has-text("Light")');
    await lightOption.waitFor({ state: 'visible', timeout: 3_000 });
    await lightOption.click();
    await page.locator('html[data-theme="light"]').waitFor({ state: 'attached', timeout: 3_000 });

    const darkOption = page.locator('.appearance-option:has-text("Dark")');
    await darkOption.click();
    await page.locator('html[data-theme="dark"]').waitFor({ state: 'attached', timeout: 3_000 });

    await page.mouse.click(10, 10);
    await page.locator('.settings-modal').waitFor({ state: 'hidden', timeout: 3_000 });

    for (const label of ['Home', 'Dictionary', 'Snippets', 'Style']) {
      const btn = page.locator(`.nav-item:has-text("${label}")`);
      await btn.waitFor({ state: 'visible', timeout: TIMEOUT });
      await btn.click();
      await page.locator('html[data-theme="dark"]').waitFor({ state: 'attached', timeout: 3_000 });
    }

    await page.reload({ waitUntil: 'networkidle', timeout: TIMEOUT });
    await page.locator('html[data-theme="dark"]').waitFor({ state: 'attached', timeout: 3_000 });

    if (errors.length > 0) {
      console.error('\nFAIL - errors:');
      errors.forEach(e => console.error('  ' + e));
      process.exit(1);
    }

    console.log('\nPASS - appearance tests passed.');
  } catch (err) {
    console.error('FAIL - test threw:', err.message);
    process.exit(1);
  } finally {
    await browser.close();
  }
})();
