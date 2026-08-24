'use strict';

const { chromium } = require('playwright');
const { TARGET_URL, TIMEOUT, seedDevState, openSettings } = require('./_dev-helpers.cjs');
const { finish, message } = require('./_regression-result.cjs');

const expected = 'Visible controls have accessible names, unique IDs, valid tab order, and keyboard-operable switches.';

function auditSurface() {
  const visible = (element) => {
    const style = getComputedStyle(element);
    const rect = element.getBoundingClientRect();
    return style.visibility !== 'hidden' && style.display !== 'none' && rect.width > 0 && rect.height > 0;
  };
  const nameFor = (element) => {
    const labelledBy = element.getAttribute('aria-labelledby');
    if (labelledBy) {
      const value = labelledBy.split(/\s+/).map((id) => document.getElementById(id)?.textContent || '').join(' ').trim();
      if (value) return value;
    }
    const direct = element.getAttribute('aria-label') || element.getAttribute('title');
    if (direct?.trim()) return direct.trim();
    if (element.id) {
      const label = document.querySelector(`label[for="${CSS.escape(element.id)}"]`);
      if (label?.textContent?.trim()) return label.textContent.trim();
    }
    if (element instanceof HTMLInputElement && element.value?.trim()) return element.value.trim();
    if (element instanceof HTMLInputElement && element.placeholder?.trim()) return element.placeholder.trim();
    return (element.textContent || '').replace(/\s+/g, ' ').trim();
  };

  const interactive = [...document.querySelectorAll(
    'button, a[href], input:not([type="hidden"]), textarea, select, [role="button"], [role="switch"], [role="option"], [role="tab"]',
  )].filter(visible);
  const unnamed = interactive
    .filter((element) => !nameFor(element))
    .map((element) => `${element.tagName.toLowerCase()}${element.className ? `.${String(element.className).trim().replace(/\s+/g, '.')}` : ''}`)
    .slice(0, 20);
  const positiveTabindex = interactive
    .filter((element) => Number(element.getAttribute('tabindex')) > 0)
    .map((element) => nameFor(element) || element.tagName.toLowerCase());
  const ids = [...document.querySelectorAll('[id]')].map((element) => element.id).filter(Boolean);
  const duplicateIds = [...new Set(ids.filter((id, index) => ids.indexOf(id) !== index))];
  const invalidSwitches = interactive
    .filter((element) => element.getAttribute('role') === 'switch')
    .filter((element) => !['true', 'false'].includes(element.getAttribute('aria-checked')) || element.tabIndex < 0)
    .map((element) => nameFor(element) || 'unnamed switch');
  return {
    interactiveCount: interactive.length,
    unnamed,
    positiveTabindex,
    duplicateIds,
    invalidSwitches,
  };
}

(async () => {
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
  const findings = [];
  const measurements = { surfaces: 0, controls: 0, switchesTested: 0 };

  await seedDevState(page, { settings: { setup_complete: true, legacy_features_enabled: true } });
  try {
    await page.goto(TARGET_URL, { waitUntil: 'domcontentloaded', timeout: TIMEOUT });
    await page.locator('.nav-item').first().waitFor({ state: 'visible', timeout: TIMEOUT });

    const collect = async (surface) => {
      const audit = await page.evaluate(auditSurface);
      measurements.surfaces += 1;
      measurements.controls += audit.interactiveCount;
      for (const [kind, values] of Object.entries(audit)) {
        if (!Array.isArray(values)) continue;
        for (const value of values) findings.push(`${surface}: ${kind}: ${value}`);
      }
    };

    await collect('home');
    await openSettings(page);
    const sections = await page.locator('.settings-nav-item').allTextContents();
    for (const rawLabel of sections) {
      const label = rawLabel.trim();
      if (!label) continue;
      await page.locator('.settings-nav-item', { hasText: label }).first().click({ timeout: TIMEOUT });
      await page.waitForTimeout(60);
      await collect(`settings/${label}`);
    }

    await page.locator('.settings-nav-item', { hasText: 'Privacy' }).click({ timeout: TIMEOUT });
    const switchControl = page.getByRole('switch', { name: 'App context hint' }).first();
    if (await switchControl.count()) {
      const switchName = await switchControl.getAttribute('aria-label') || 'unnamed switch';
      const before = await switchControl.getAttribute('aria-checked');
      await switchControl.focus();
      await page.keyboard.press('Space');
      await page.waitForTimeout(80);
      const after = await switchControl.getAttribute('aria-checked');
      measurements.switchesTested = 1;
      const confirmationOpened = await page.locator('[role="dialog"]:visible').count() > 0;
      if (before === after && !confirmationOpened) findings.push(`settings: keyboard: Space neither changed "${switchName}" nor opened its confirmation`);
      measurements.switchName = switchName;
    }

    finish({
      status: findings.length ? 'failed' : 'passed',
      expected,
      observed: findings.length ? findings.join('; ') : `Audited ${measurements.controls} visible controls across ${measurements.surfaces} surfaces`,
      regressionArea: 'accessibility semantics and keyboard input',
      measurements,
      failureKind: findings.length ? 'product' : null,
    });
  } catch (error) {
    finish({ status: 'failed', expected, observed: message(error), regressionArea: 'accessibility test execution', failureKind: 'infrastructure' });
  } finally {
    await browser.close();
  }
})();
