'use strict';

const { chromium } = require('playwright');
const { TARGET_URL, TIMEOUT, seedDevState, openSettings } = require('./_dev-helpers.cjs');
const { finish, message } = require('./_regression-result.cjs');

const expected = 'Model settings tiles open from the keyboard, retain focus within the expanded control, and collapse with Escape.';

(async () => {
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
  const failures = [];

  try {
    await seedDevState(page, { settings: { setup_complete: true, advanced_model_ui: true } });
    await page.goto(TARGET_URL, { waitUntil: 'domcontentloaded', timeout: TIMEOUT });
    await openSettings(page);
    await page.locator('.settings-nav-item', { hasText: 'Models' }).click({ timeout: TIMEOUT });
    const trigger = page.locator('.task-tile button.tile-head').first();
    await trigger.waitFor({ state: 'visible', timeout: TIMEOUT });
    await trigger.focus();
    await page.keyboard.press('Enter');
    const opened = await page.waitForFunction(() => document.querySelector('.task-tile.task-open') != null, null, { timeout: TIMEOUT }).then(() => true).catch(() => false);
    if (!opened) failures.push('Enter did not open the model tile');
    let initialInside = false;
    if (opened) {
      initialInside = await page.evaluate(() => document.querySelector('.task-tile.task-open')?.contains(document.activeElement) ?? false);
      if (!initialInside) failures.push('focus did not remain within the expanded model tile');
    }

    let tabCycles = 0;
    if (initialInside) {
      await page.keyboard.press('Tab');
      tabCycles = 1;
      if (!await page.evaluate(() => document.activeElement && document.activeElement !== document.body && document.activeElement !== document.documentElement)) failures.push('Tab navigation left focus on the document body');
    }

    let closedWithEscape = false;
    if (opened) {
      await page.keyboard.press('Escape');
      closedWithEscape = await page.waitForFunction(() => !document.querySelector('.task-tile.task-open'), null, { timeout: TIMEOUT }).then(() => true).catch(() => false);
      if (!closedWithEscape) failures.push('Escape did not collapse the model tile');
    }
    let restored = false;
    if (closedWithEscape) {
      await page.waitForFunction(() => document.activeElement?.matches('button.tile-head'), null, { timeout: TIMEOUT }).catch(() => {});
      restored = await trigger.evaluate((element) => document.activeElement === element).catch(() => false);
      if (!restored) failures.push('focus did not return to the model tile trigger after Escape');
    }

    finish({
      status: failures.length ? 'failed' : 'passed',
      expected,
      observed: failures.length ? failures.join('; ') : 'The model tile opened, retained keyboard focus, and collapsed via Escape',
      regressionArea: 'settings keyboard interaction and focus management',
      measurements: { tabCycles, closedWithEscape, focusRestored: restored },
      failureKind: failures.length ? 'product' : null,
      regressionStatus: failures.length ? 'pre_existing' : 'unknown',
    });
  } catch (error) {
    finish({ status: 'failed', expected, observed: message(error), regressionArea: 'focus-flow test execution', failureKind: 'infrastructure' });
  } finally {
    await browser.close();
  }
})();
