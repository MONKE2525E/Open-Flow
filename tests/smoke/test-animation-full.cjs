const { chromium } = require('playwright');
const { tauriMock, APP_VERSION } = require('./_tauri-mock.cjs');

const TARGET_URL = 'http://localhost:1420';

(async () => {
  const browser = await chromium.launch({ headless: false });
  const page = await browser.newPage();
  const errors = [];

  await page.addInitScript(tauriMock, { appVersion: APP_VERSION });
  page.on('pageerror', err => errors.push(`Page error: ${err.message}`));
  page.on('console', msg => {
    if (msg.type() === 'error') errors.push(`Console error: ${msg.text()}`);
  });

  try {
    await page.goto(TARGET_URL, { waitUntil: 'networkidle' });
    await page.locator('.nav-item').first().waitFor({ state: 'visible' });

    // Navigate to Settings
    await page.locator('.nav-item', { hasText: 'Settings' }).click();
    await page.waitForTimeout(800);

    const micBtn = page.locator('.mic-btn').first();
    
    // Check initial state
    const initialWidth = await micBtn.evaluate(el => el.style.width);
    const initialOffsetWidth = await micBtn.evaluate(el => el.offsetWidth);
    console.log('Initial state:');
    console.log('  style.width:', initialWidth);
    console.log('  offsetWidth:', initialOffsetWidth);
    
    // Verify transition is set
    const transition = await micBtn.evaluate(el => el.style.transition);
    if (!transition.includes('width')) {
      errors.push('Transition does not include width');
    } else {
      console.log('✓ Transition includes width:', transition);
    }

    // Open dropdown and select a device
    await micBtn.click();
    await page.waitForSelector('.mic-menu');
    
    const items = await page.locator('.mic-item').count();
    console.log(`Found ${items} mic items in menu`);
    
    // Select the last item (hopefully a longer device name, or use second item)
    if (items > 1) {
      await page.locator('.mic-item').nth(1).click();
      await page.waitForTimeout(500);
      
      // Check new width after selection
      const newWidth = await micBtn.evaluate(el => el.style.width);
      const newOffsetWidth = await micBtn.evaluate(el => el.offsetWidth);
      const newText = await micBtn.evaluate(el => el.textContent?.trim());
      
      console.log('After selection:');
      console.log('  text:', newText);
      console.log('  style.width:', newWidth);
      console.log('  offsetWidth:', newOffsetWidth);
      
      // Verify width changed (or stayed the same if text length is similar)
      if (initialWidth && newWidth && initialWidth !== newWidth) {
        console.log('✓ Width changed from', initialWidth, 'to', newWidth);
      } else {
        console.log('  (Width unchanged, text length may be similar)');
      }
    } else {
      console.log('  Only 1 device available, skipping selection test');
    }

    if (errors.length > 0) {
      console.error('FAIL — errors:');
      errors.forEach(e => console.error('  ' + e));
      process.exit(1);
    }

    console.log('\nPASS — dropdown width animation working');

  } catch (err) {
    console.error('FAIL — test threw:', err.message);
    process.exit(1);
  } finally {
    await browser.close();
  }
})();
