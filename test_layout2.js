import { chromium } from 'playwright';

(async () => {
  try {
    const browser = await chromium.launch({ headless: true });
    const page = await browser.newPage();
    
    await page.setViewportSize({ width: 900, height: 600 });
    
    console.log(`Navigating to http://localhost:1420...`);
    await page.goto('http://localhost:1420');
    
    console.log('Progressing to Step 6...');
    
    // Wait for intro ready
    await page.waitForTimeout(1000);
    
    for (let i = 0; i < 6; i++) {
      try {
        const btn = await page.locator('.step-wrap.visible .btn-primary').first();
        await btn.waitFor({ state: 'visible', timeout: 5000 });
        await btn.click();
        await page.waitForTimeout(600); // wait for animation
        console.log(`Clicked next. Step ${i + 1}`);
      } catch (e) {
        console.log('Could not find next button or timeout:', e.message);
      }
    }
    
    await page.waitForTimeout(1000);
    await page.screenshot({ path: 'setup_step6.png' });
    console.log('Screenshot saved to setup_step6.png');
    
    const isGrid = await page.evaluate(() => {
      const qsCards = document.querySelector('.qs-cards');
      if (!qsCards) return false;
      const style = window.getComputedStyle(qsCards);
      return style.display === 'grid' && style.gridTemplateColumns.includes('1fr');
    });
    console.log('Grid Layout active:', isGrid);
    
    await browser.close();
  } catch (e) {
    console.error(e);
  }
})();
