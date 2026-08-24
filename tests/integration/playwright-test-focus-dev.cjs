'use strict';

const { chromium } = require('playwright');
const { TARGET_URL, TIMEOUT, seedDevState, openSettings } = require('./_dev-helpers.cjs');
const { finish, message } = require('./_regression-result.cjs');

const expected = 'The model picker receives focus, traps Tab navigation, closes with Escape, and restores focus to its trigger.';

(async () => {
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
  const failures = [];

  await seedDevState(page, { settings: { setup_complete: true, advanced_model_ui: true } });
  try {
    await page.goto(TARGET_URL, { waitUntil: 'domcontentloaded', timeout: TIMEOUT });
    await openSettings(page);
    await page.locator('.settings-nav-item', { hasText: 'Models' }).click({ timeout: TIMEOUT });
    const trigger = page.locator('button.tile-btn-primary', { hasText: 'Change' }).first();
    await trigger.waitFor({ state: 'visible', timeout: TIMEOUT });
    await trigger.focus();
    await page.keyboard.press('Enter');

    const dialog = page.locator('.picker-card[role="dialog"][aria-modal="true"]');
    await dialog.waitFor({ state: 'visible', timeout: TIMEOUT });
    await page.waitForTimeout(50);
    const initialInside = await page.evaluate(() => document.querySelector('.picker-card')?.contains(document.activeElement) ?? false);
    if (!initialInside) failures.push('focus did not move into the model picker');

    let escaped = false;
    let tabCycles = 0;
    for (let index = 0; index < 30; index += 1) {
      await page.keyboard.press('Tab');
      tabCycles += 1;
      const inside = await page.evaluate(() => document.querySelector('.picker-card')?.contains(document.activeElement) ?? false);
      if (!inside) {
        escaped = true;
        break;
      }
    }
    if (escaped) failures.push('Tab focus escaped the modal dialog');

    await page.keyboard.press('Escape');
    const closedWithEscape = await dialog.waitFor({ state: 'hidden', timeout: 1000 }).then(() => true).catch(() => false);
    if (!closedWithEscape) failures.push('Escape did not close the model picker');
    let restored = false;
    if (closedWithEscape) {
      await page.waitForTimeout(80);
      restored = await trigger.evaluate((element) => document.activeElement === element);
      if (!restored) failures.push('focus did not return to the Change button after Escape');
    }

    finish({
      status: failures.length ? 'failed' : 'passed',
      expected,
      observed: failures.length ? failures.join('; ') : 'Focus entered the dialog, remained contained, and returned to the trigger',
      regressionArea: 'modal keyboard and focus management',
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

