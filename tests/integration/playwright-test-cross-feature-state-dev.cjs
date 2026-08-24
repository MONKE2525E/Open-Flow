'use strict';

const { chromium } = require('playwright');
const { TARGET_URL, TIMEOUT, openSettings, closeSettings } = require('./_dev-helpers.cjs');
const { tauriMock, APP_VERSION } = require('../smoke/_tauri-mock.cjs');
const { finish, message } = require('./_regression-result.cjs');

const expected = 'Legacy mode and Contexts remain mutually exclusive before and after reload, with the setting reversible.';

(async () => {
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
  const failures = [];

  await page.addInitScript(tauriMock, { appVersion: APP_VERSION });
  try {
    await page.goto(TARGET_URL, { waitUntil: 'domcontentloaded', timeout: TIMEOUT });
    await page.locator('.nav-item').first().waitFor({ state: 'visible', timeout: TIMEOUT });
    if ((await page.locator('.ctx-head-label:has-text("Contexts")').count()) !== 1) failures.push('Contexts was missing in default mode');
    if (await page.locator('.nav-item:has-text("Dictionary")').count()) failures.push('Dictionary was visible while Legacy mode was off');

    await openSettings(page);
    await page.locator('.settings-nav-item', { hasText: 'General' }).click({ timeout: TIMEOUT });
    await page.getByRole('switch', { name: 'Legacy pages' }).click();
    const confirm = page.getByRole('dialog', { name: 'Turn on Legacy pages?' });
    await confirm.waitFor({ state: 'visible', timeout: TIMEOUT });
    await confirm.getByRole('button', { name: 'Turn on' }).click();
    await closeSettings(page);
    await page.locator('.ctx-head-label:has-text("Contexts")').waitFor({ state: 'hidden', timeout: TIMEOUT }).catch(() => {});
    if (await page.locator('.ctx-head-label:has-text("Contexts")').count()) failures.push('Contexts remained visible in Legacy mode');
    for (const label of ['Dictionary', 'Snippets']) {
      if ((await page.locator(`.nav-item:has-text("${label}")`).count()) !== 1) failures.push(`${label} was missing in Legacy mode`);
    }

    await page.reload({ waitUntil: 'domcontentloaded', timeout: TIMEOUT });
    await page.locator('.nav-item').first().waitFor({ state: 'visible', timeout: TIMEOUT });
    await page.locator('.ctx-head-label:has-text("Contexts")').waitFor({ state: 'hidden', timeout: TIMEOUT }).catch(() => {});
    if (await page.locator('.ctx-head-label:has-text("Contexts")').count()) failures.push('Contexts returned after reloading Legacy mode');

    await openSettings(page);
    await page.locator('.settings-nav-item', { hasText: 'General' }).click({ timeout: TIMEOUT });
    await page.getByRole('switch', { name: 'Legacy pages' }).click();
    await closeSettings(page);
    await page.locator('.ctx-head-label:has-text("Contexts")').waitFor({ state: 'visible', timeout: TIMEOUT }).catch(() => {});
    if ((await page.locator('.ctx-head-label:has-text("Contexts")').count()) !== 1) failures.push('Contexts did not return after disabling Legacy mode');

    finish({
      status: failures.length ? 'failed' : 'passed',
      expected,
      observed: failures.length ? failures.join('; ') : 'Both navigation modes stayed exclusive and persisted through reload',
      regressionArea: 'cross-feature settings and navigation state',
      measurements: { reloads: 1, modeTransitions: 2 },
      failureKind: failures.length ? 'product' : null,
    });
  } catch (error) {
    finish({ status: 'failed', expected, observed: message(error), regressionArea: 'cross-feature test execution', failureKind: 'infrastructure' });
  } finally {
    await browser.close();
  }
})();
