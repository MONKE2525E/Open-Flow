'use strict';

const { chromium } = require('playwright');
const { TARGET_URL, TIMEOUT } = require('./_dev-helpers.cjs');
const { tauriMock, APP_VERSION } = require('../smoke/_tauri-mock.cjs');
const { finish, message } = require('./_regression-result.cjs');

const expected = 'Malformed persisted state and a rejected settings write do not crash the UI, leave optimistic state applied, or fail without user-visible feedback.';

(async () => {
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
  const failures = [];
  const pageErrors = [];

  await page.addInitScript(tauriMock, { appVersion: APP_VERSION });
  page.on('pageerror', (error) => pageErrors.push(error.message));

  try {
    await page.goto(TARGET_URL, { waitUntil: 'domcontentloaded', timeout: TIMEOUT });
    await page.evaluate(() => localStorage.setItem('__open_flow_tauri_mock_settings', '{broken-json'));
    await page.reload({ waitUntil: 'domcontentloaded', timeout: TIMEOUT });
    await page.evaluate(() => {
      const original = window.__TAURI_INTERNALS__.invoke;
      window.__TAURI_INTERNALS__.invoke = (command, args) => {
        if (command === 'save_setting' && args?.key === 'legacy_features_enabled') {
          return Promise.reject(new Error('simulated settings disk failure'));
        }
        return original(command, args);
      };
    });
    await page.locator('.nav-item:has-text("Settings")').click({ timeout: TIMEOUT });
    await page.locator('.settings-page').waitFor({ state: 'visible', timeout: TIMEOUT });
    await page.locator('.settings-nav-item', { hasText: 'General' }).click({ timeout: TIMEOUT });
    const toggle = page.getByRole('switch', { name: 'Legacy pages' });
    await toggle.click();
    const dialog = page.getByRole('dialog', { name: 'Turn on Legacy pages?' });
    await dialog.getByRole('button', { name: 'Turn on' }).click();
    await page.waitForFunction(() => document.querySelector('[role="switch"][aria-label="Legacy pages"]')?.getAttribute('aria-checked') === 'false', null, { timeout: TIMEOUT }).catch(() => failures.push('failed save left Legacy mode enabled'));
    if (!await page.locator('.app').isVisible()) failures.push('application surface disappeared after failed save');
    const visibleFeedback = page.locator('[role="alert"]:visible, .toast:visible, .error-toast:visible, .settings-error:visible');
    if ((await visibleFeedback.count()) === 0) failures.push('rejected settings write was only logged to the console; the user received no visible error');
    if (pageErrors.length) failures.push(`uncaught page errors: ${pageErrors.join(', ')}`);

    finish({
      status: failures.length ? 'failed' : 'passed',
      expected,
      observed: failures.length ? failures.join('; ') : 'Invalid JSON fell back safely, the rejected save rolled back state, and the UI reported the failure',
      regressionArea: 'state recovery and settings error handling',
      measurements: { simulatedFailures: 2, uncaughtErrors: pageErrors.length },
      failureKind: failures.length ? 'product' : null,
      regressionStatus: failures.length ? 'pre_existing' : 'unknown',
    });
  } catch (error) {
    finish({ status: 'failed', expected, observed: message(error), regressionArea: 'resilience test execution', failureKind: 'infrastructure' });
  } finally {
    await browser.close();
  }
})();
