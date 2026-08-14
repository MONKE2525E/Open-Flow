'use strict';

const path = require('path');
const os = require('os');

// History search + app filter UX. Exercises the real Home/HistoryList/
// HistoryToolbar against a filtering `get_recent`/`get_history_apps` mock so
// the search → SQLite-arg wiring, the app dropdown, the combined filter, the
// row metadata, and the "No matches" empty state are all covered end-to-end.

const { chromium } = require('playwright');
const { tauriMock } = require('../smoke/_tauri-mock.cjs');

const FAILURE_SCREENSHOT = path.join(os.tmpdir(), 'verenu-history-filter-fail.png');

// Overrides the frozen smoke mock's history commands with a filtering variant
// that carries app names + durations, so the frontend's real flow is exercised.
const historyMockWrap = function () {
  const entries = [
    { id: 6, clean_text: 'Send the quarterly report to accounting', raw_text: 'send the quarterly report to accounting', words: 7, duration_ms: 8400, app_name: 'outlook.exe', created_at: '2026-05-31 08:05:00' },
    { id: 5, clean_text: 'Refactor the login flow for the new design', raw_text: 'refactor the login flow', words: 9, duration_ms: 12200, app_name: 'code.exe', created_at: '2026-05-31 09:12:00' },
    { id: 4, clean_text: 'Remind me to call the accountant tomorrow', raw_text: 'remind me to call the accountant', words: 7, duration_ms: 6100, app_name: 'outlook.exe', created_at: '2026-05-30 16:40:00' },
    { id: 3, clean_text: 'Set up a meeting with the design team', raw_text: 'set up a meeting with the design team', words: 8, duration_ms: 7300, app_name: 'outlook.exe', created_at: '2026-05-30 10:00:00' },
    { id: 2, clean_text: 'Rename the tests to match the new spec', raw_text: 'rename the tests', words: 8, duration_ms: 5600, app_name: 'code.exe', created_at: '2026-05-29 14:22:00' },
    { id: 1, clean_text: 'Draft a response to the vendor', raw_text: 'draft a response to the vendor', words: 6, duration_ms: 4800, app_name: null, created_at: '2026-05-29 09:05:00' },
  ];
  const baseInvoke = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args) {
    if (cmd === 'get_recent') {
      const limit = Number(args?.limit ?? 100);
      const offset = Number(args?.offset ?? 0);
      const search = typeof args?.search === 'string' ? args.search.trim().toLowerCase() : '';
      const appName = typeof args?.appName === 'string' ? args.appName : (typeof args?.app_name === 'string' ? args.app_name : null);
      let filtered = entries.filter((e) => {
        if (search && !e.clean_text.toLowerCase().includes(search) && !e.raw_text.toLowerCase().includes(search)) {
          return false;
        }
        if (appName && e.app_name !== appName) {
          return false;
        }
        return true;
      });
      return Promise.resolve(filtered.slice(offset, offset + limit));
    }
    if (cmd === 'get_history_apps') {
      return Promise.resolve(['code.exe', 'outlook.exe']);
    }
    return baseInvoke(cmd, args);
  };
};

(async () => {
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
  const errors = [];

  page.on('pageerror', (err) => errors.push(`Page exception: ${err.message}`));
  page.on('console', (msg) => {
    if (msg.type() === 'error') errors.push(`Console error: ${msg.text()}`);
  });

  await page.addInitScript(tauriMock, {});
  await page.addInitScript(historyMockWrap);

  try {
    await page.goto('http://localhost:1420', { waitUntil: 'networkidle', timeout: 15_000 });
    await page.locator('h1.page-h:has-text("Welcome back")').waitFor({ state: 'visible', timeout: 12_000 });

    // Search is intentionally collapsed to a bare icon until it is needed.
    await page.getByRole('button', { name: 'Search history' }).click();
    const search = page.getByRole('textbox', { name: 'Search history' });
    await search.waitFor({ state: 'visible', timeout: 5_000 });

    const dayRows = page.locator('.day-row');
    const visibleCleanTexts = async () => {
      await page.waitForTimeout(150);
      return page.locator('.day-text').allInnerTexts();
    };

    // Row metadata: app label + compact duration under the text.
    const metaFirst = await page.locator('.day-meta').first().innerText();
    if (!/Outlook · \d+s/.test(metaFirst)) errors.push(`row meta wrong: "${metaFirst}"`);
    // Row with no app but a duration still shows just the duration.
    const metaNoApp = await page.locator('.day-row:has-text("Draft a response to the vendor") .day-meta').innerText();
    if (metaNoApp !== '5s') errors.push(`no-app row meta wrong: "${metaNoApp}"`);
    if ((await page.locator('.day-row').count()) !== 6) errors.push('all history rows should render unfiltered');

    // Search: partial + case-insensitive, finds cleaned text.
    await search.fill('quarterly');
    await page.waitForTimeout(500);
    const searched = await visibleCleanTexts();
    if (searched.length !== 1 || !searched[0].includes('quarterly')) {
      errors.push(`search "quarterly" wrong: ${JSON.stringify(searched)}`);
    }

    // Combined search + app filter.
    await page.locator('.history-app-dropdown button').click();
    await page.waitForTimeout(200);
    await page.locator('#history-app-menu [role="option"]:has-text("Outlook")').click();
    await page.waitForTimeout(500);
    const combined = await visibleCleanTexts();
    if (combined.length !== 1 || !combined[0].includes('accounting')) {
      errors.push(`search+app combined wrong: ${JSON.stringify(combined)}`);
    }

    // App filter alone.
    await page.getByRole('button', { name: 'Clear search' }).click();
    await page.waitForTimeout(500);
    const byApp = await visibleCleanTexts();
    if (byApp.length !== 3 || !byApp.every((t) => /accounting|call the accountant|design team/.test(t))) {
      errors.push(`app filter wrong: ${JSON.stringify(byApp)}`);
    }

    // No matches → empty state; reset via the search × and the dropdown's
    // "All apps" option (there is deliberately no separate "Clear filters"
    // button).
    await search.fill('zzzz nothing matches');
    await page.waitForTimeout(500);
    const noMatch = await page.locator('.empty-state .empty-h').innerText();
    if (noMatch !== 'No matches') errors.push(`empty state heading wrong: "${noMatch}"`);
    await page.getByRole('button', { name: 'Clear search' }).click();
    await page.waitForTimeout(300);
    await page.locator('.history-app-dropdown button').click();
    await page.waitForTimeout(200);
    await page.locator('#history-app-menu [role="option"]:has-text("All apps")').click();
    await page.waitForTimeout(500);
    if ((await page.locator('.day-row').count()) !== 6) errors.push('resetting to All apps must restore all rows');
    if ((await page.locator('.empty-state').count()) !== 0) errors.push('empty state must disappear after resetting to All apps');

    // The explicit clear action enters with a horizontal transition and resets
    // both controls together.
    await search.fill('accounting');
    await page.waitForTimeout(500);
    await page.getByRole('button', { name: 'Clear filters' }).click();
    await page.waitForTimeout(500);
    if ((await page.locator('.day-row').count()) !== 6) errors.push('Clear filters must restore all rows');

    // Pagination contract intact: "Load older" only when a full page is present
    // is exercised by the frozen smoke test; here just ensure no error surfaced.
    if (errors.length > 0) {
      console.error('FAILURES:');
      errors.forEach((e) => console.error('  ' + e));
      await page.screenshot({ path: FAILURE_SCREENSHOT, fullPage: true });
      process.exitCode = 1;
      return;
    }
    console.log('PASS — history search, app filter, metadata, and clear-all work together.');
  } catch (err) {
    console.error('FAIL — test threw:', err.message);
    process.exitCode = 1;
  } finally {
    await browser.close();
  }
})();
