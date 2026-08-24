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
    await page.waitForFunction(() => document.querySelector('.task-tile.task-open') != null, null, { timeout: TIMEOUT });
    const initialInside = await page.evaluate(() => document.querySelector('.task-tile.task-open')?.contains(document.activeElement) ?? false);
    if (!initialInside) failures.push('focus did not remain within the expanded model tile');

    let escaped = false;
    let tabCycles = 0;
    if (initialInside) {
      for (let index = 0; index < 30; index += 1) {
        await page.keyboard.press('Tab');
        tabCycles += 1;
        const inside = await page.evaluate(() => document.querySelector('.task-tile.task-open')?.contains(document.activeElement) ?? false);
        if (!inside) {
          escaped = true;
          break;
        }
      }
    }
    if (escaped) failures.push('Tab focus escaped the modal dialog');

    await page.keyboard.press('Escape');
    const closedWithEscape = await page.waitForFunction(() => !document.querySelector('.task-tile.task-open'), null, { timeout: TIMEOUT }).then(() => true).catch(() => false);
    if (!closedWithEscape) failures.push('Escape did not collapse the model tile');
    let restored = false;
    if (closedWithEscape) {
      await page.waitForFunction(() => document.activeElement?.matches('button.tile-head'), null, { timeout: TIMEOUT }).catch(() => {});
      restored = await trigger.evaluate((element) => document.activeElement === element);
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
