'use strict';

const { chromium } = require('playwright');
const { TARGET_URL, TIMEOUT, seedDevState } = require('./_dev-helpers.cjs');

(async () => {
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage();

  await seedDevState(page, {
    settings: {
      setup_complete: true,
      force_setup_on_launch: false,
    },
  });

  await page.addInitScript(() => {
    Object.defineProperty(window.navigator, 'onLine', {
      configurable: true,
      get: () => false,
    });
  });

  try {
    await page.goto(TARGET_URL, { waitUntil: 'networkidle', timeout: TIMEOUT });
    await page.locator('.offline-toast:has-text("No internet connection")').waitFor({
      state: 'visible',
      timeout: TIMEOUT,
    });
    console.log('PASS - offline toast renders when browser dev mode reports navigator.offLine.');
  } catch (err) {
    console.error(`FAIL - offline toast test threw: ${err.message}`);
    process.exit(1);
  } finally {
    await browser.close();
  }
})();
