import { chromium } from 'playwright';

(async () => {
  try {
    const browser = await chromium.launch({ headless: true });
    const page = await browser.newPage();
    
    await page.setViewportSize({ width: 900, height: 600 }); // min window size
    
    console.log(`Navigating to http://localhost:1420...`);
    await page.goto('http://localhost:1420');
    
    await page.waitForTimeout(1000);
    
    for (let i = 0; i < 6; i++) {
      try {
        const btn = await page.locator('.step-wrap.visible .btn-primary').first();
        await btn.waitFor({ state: 'visible', timeout: 5000 });
        await btn.click();
        await page.waitForTimeout(600);
      } catch (e) {
        console.log('Error clicking next:', e.message);
      }
    }
    
    await page.waitForTimeout(1000);
    
    // Check if step 6 is visible
    const setupOverlay = await page.locator('.setup-overlay').boundingBox();
    const qsCardsBox = await page.locator('.qs-cards').boundingBox();
    
    console.log('Setup Overlay Box:', setupOverlay);
    console.log('QS Cards Box:', qsCardsBox);
    
    if (qsCardsBox.y + qsCardsBox.height > setupOverlay.y + setupOverlay.height) {
      console.log('FAILED: Content goes off the page.');
      process.exit(1);
    } else {
      console.log('SUCCESS: Content fits cleanly within the page.');
    }
    
    await browser.close();
    process.exit(0);
  } catch (e) {
    console.error(e);
    process.exit(1);
  }
})();
