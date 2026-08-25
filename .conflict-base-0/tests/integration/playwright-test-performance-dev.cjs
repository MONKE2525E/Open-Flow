'use strict';

const { chromium } = require('playwright');
const baseline = require('../baselines/ui-performance.json');
const { TARGET_URL, TIMEOUT, seedDevState } = require('./_dev-helpers.cjs');
const { finish, message } = require('./_regression-result.cjs');

const expected = 'Warm browser startup and common settings interactions stay within the checked-in regression budgets.';
const round = (value) => Math.round(value * 100) / 100;

(async () => {
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
  const failures = [];
  const pageErrors = [];

  await seedDevState(page, { settings: { setup_complete: true, legacy_features_enabled: true } });
  await page.addInitScript(() => {
    window.__verenuLongTasks = 0;
    try {
      new PerformanceObserver((list) => {
        window.__verenuLongTasks += list.getEntries().filter((entry) => entry.duration > 50).length;
      }).observe({ type: 'longtask', buffered: true });
    } catch {}
  });
  page.on('pageerror', (error) => pageErrors.push(error.message));

  try {
    const start = performance.now();
    await page.goto(TARGET_URL, { waitUntil: 'domcontentloaded', timeout: TIMEOUT });
    await page.locator('.nav-item').first().waitFor({ state: 'visible', timeout: TIMEOUT });
    const navigationVisible = performance.now() - start;

    const settingsStart = performance.now();
    await page.locator('.nav-item:has-text("Settings")').click({ timeout: TIMEOUT });
    await page.locator('.settings-page').waitFor({ state: 'visible', timeout: TIMEOUT });
    const settingsOpen = performance.now() - settingsStart;

    const sectionTimes = [];
    for (const label of ['Models', 'Privacy', 'Audio', 'About', 'General']) {
      const sectionStart = performance.now();
      await page.locator('.settings-nav-item', { hasText: label }).click({ timeout: TIMEOUT });
      await page.locator('h2.settings-h', { hasText: label }).waitFor({ state: 'visible', timeout: TIMEOUT });
      sectionTimes.push(performance.now() - sectionStart);
    }
    const sorted = [...sectionTimes].sort((a, b) => a - b);
    const sectionP95 = sorted[Math.ceil(sorted.length * 0.95) - 1] || 0;

    const closeStart = performance.now();
    await page.locator('.settings-back').click({ timeout: TIMEOUT });
    await page.locator('.settings-page').waitFor({ state: 'hidden', timeout: TIMEOUT });
    const settingsClose = performance.now() - closeStart;
    const longTasks = await page.evaluate(() => window.__verenuLongTasks || 0);

    const measurements = {
      navigation_visible_ms: round(navigationVisible),
      settings_open_ms: round(settingsOpen),
      section_change_p95_ms: round(sectionP95),
      settings_close_ms: round(settingsClose),
      long_tasks_over_50ms: longTasks,
      uncaught_errors: pageErrors.length,
    };
    const mapping = {
      navigation_visible_ms: 'navigation_visible',
      settings_open_ms: 'settings_open',
      section_change_p95_ms: 'section_change_p95',
      settings_close_ms: 'settings_close',
    };
    for (const [measurement, budgetKey] of Object.entries(mapping)) {
      const limit = baseline.budgets_ms[budgetKey];
      if (typeof limit !== 'number' || !Number.isFinite(limit)) {
        failures.push(`Missing numeric performance budget for ${budgetKey}`);
      } else if (measurements[measurement] > limit) {
        failures.push(`${measurement} was ${measurements[measurement]}ms, budget ${limit}ms`);
      }
    }
    if (longTasks > baseline.limits.long_tasks_over_50ms) failures.push(`${longTasks} long tasks exceeded limit ${baseline.limits.long_tasks_over_50ms}`);
    if (pageErrors.length > baseline.limits.uncaught_errors) failures.push(`${pageErrors.length} uncaught errors exceeded limit ${baseline.limits.uncaught_errors}`);

    finish({
      status: failures.length ? 'failed' : 'passed',
      expected,
      observed: failures.length ? failures.join('; ') : 'All startup, interaction, long-task, and error budgets held',
      regressionArea: 'browser startup and settings interaction performance',
      measurements,
      baseline,
      failureKind: failures.length ? 'product' : null,
    });
  } catch (error) {
    finish({ status: 'failed', expected, observed: message(error), regressionArea: 'performance test execution', failureKind: 'infrastructure', baseline });
  } finally {
    await browser.close();
  }
})();
