'use strict';

/*
 * Loads the app once so Vite compiles its module graph before the real tests run.
 *
 * Vite answers a request for "/" with index.html the moment it boots, so an
 * HTTP probe reports the server ready long before it has transformed the ~170
 * Svelte/TS modules the app actually imports. That first genuine page load can
 * take well over ten seconds, which the first UI test would otherwise absorb —
 * burning its single retry on startup cost and leaving nothing in reserve for a
 * real flake.
 *
 * Always exits 0: this is an optimisation, not a gate. If warming fails the
 * suite should still run and report on its own terms.
 */

const url = process.argv[2] || 'http://localhost:1420';
const BUDGET_MS = Number(process.env.WARMUP_TIMEOUT_MS || 90_000);

(async () => {
  let browser;
  try {
    const { chromium } = require('playwright');
    browser = await chromium.launch({ headless: true });
    const page = await browser.newPage();
    await page.goto(url, { waitUntil: 'domcontentloaded', timeout: BUDGET_MS });
    // .app only exists once the Svelte root has actually mounted, so waiting on
    // it proves the graph is compiled rather than merely served.
    await page.locator('.app').waitFor({ state: 'attached', timeout: BUDGET_MS });
  } catch {
    // Non-fatal by design.
  } finally {
    if (browser) await browser.close().catch(() => {});
  }
  process.exit(0);
})();
