// Smoke test: app mount + DOM structure
// Verifies the app mounts without JS errors and exposes required DOM structure.
const { chromium } = require('playwright');
const { tauriMock } = require('./_tauri-mock.cjs');

const TARGET_URL = 'http://localhost:1420';
const TIMEOUT = 10_000;

(async () => {
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage();
  const errors = [];

  // Inject Tauri IPC mock before the page loads so setup_complete = true
  await page.addInitScript(tauriMock);

  page.on('pageerror', err => errors.push(`Page exception: ${err.message}`));
  page.on('console', msg => {
    if (msg.type() === 'error') errors.push(`Console error: ${msg.text()}`);
  });

  try {
    await page.goto(TARGET_URL, { waitUntil: 'networkidle', timeout: TIMEOUT });

    // App must mount and render at least one nav item within 5 s
    await page.locator('.nav-item').first().waitFor({ state: 'visible', timeout: 5_000 });
    const count = await page.locator('.nav-item').count();
    if (count < 4) errors.push(`Expected ≥4 .nav-item elements, got ${count}`);

    // Title bar or root wrapper must exist
    const appDiv = page.locator('.app');
    if (!(await appDiv.isVisible())) errors.push('.app root element not found');

    // Screenshot for visual inspection
    await page.screenshot({ path: 'G:\\Verenu\\screenshot.png', fullPage: true });
    console.log('Screenshot saved to screenshot.png');

    // Fail if any JS errors occurred during load or interaction
    if (errors.length > 0) {
      console.error('FAIL — errors found:');
      errors.forEach(e => console.error('  ' + e));
      process.exit(1);
    }

    console.log('PASS — app loaded, nav mounted, no JS errors.');
  } catch (err) {
    console.error('FAIL — test threw:', err.message);
    process.exit(1);
  } finally {
    await browser.close();
  }
})();
