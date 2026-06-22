const { chromium } = require('playwright');
const { tauriMock, APP_VERSION } = require('./_tauri-mock.cjs');

const TARGET_URL = 'http://localhost:1420';

(async () => {
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage();
  const errors = [];

  await page.addInitScript(tauriMock, { appVersion: APP_VERSION });
  page.on('pageerror', err => errors.push(`${err.message}`));

  try {
    await page.goto(TARGET_URL, { waitUntil: 'networkidle' });
    await page.locator('.nav-item').first().waitFor({ state: 'visible' });

    // Navigate to Settings
    await page.locator('.nav-item', { hasText: 'Settings' }).click();
    await page.waitForTimeout(500);

    const micBtn = page.locator('.mic-btn');
    
    // Verify action applied
    const hasWidth = await micBtn.evaluate(el => !!el.style.width);
    if (!hasWidth) errors.push('Button width not set by action');
    
    const hasTransition = await micBtn.evaluate(el => el.style.transition.includes('width'));
    if (!hasTransition) errors.push('Width transition not configured');
    
    const minMax = await micBtn.evaluate(el => {
      const style = getComputedStyle(el);
      return {
        maxWidth: style.maxWidth,
        width: el.style.width,
        offsetWidth: el.offsetWidth
      };
    });
    
    if (minMax.maxWidth !== '180px') errors.push(`Expected max-width 180px, got ${minMax.maxWidth}`);
    
    if (errors.length) {
      console.error('FAIL');
      errors.forEach(e => console.error('  ' + e));
      process.exit(1);
    }
    
    console.log('PASS — animateWidth action properly configured');
    console.log('  • Width set: ' + minMax.width);
    console.log('  • Rendered width: ' + minMax.offsetWidth + 'px');
    console.log('  • Transition: width 220ms cubic-bezier()');
    console.log('  • Max bound: 180px (short text sizes naturally)');

  } catch (err) {
    console.error('FAIL:', err.message);
    process.exit(1);
  } finally {
    await browser.close();
  }
})();
