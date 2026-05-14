const { chromium } = require('playwright');

const TARGET_URL = 'http://localhost:5173'; // Vite default

(async () => {
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage();
  
  page.on('console', msg => {
    if (msg.type() === 'error') {
      console.error('Browser Error:', msg.text());
    }
  });

  page.on('pageerror', err => {
    console.error('Page Exception:', err.message);
  });

  try {
    await page.goto(TARGET_URL);
    console.log('Page loaded:', await page.title());
    
    await page.waitForTimeout(2000);
    
    const appBody = await page.innerHTML('body');
    if (!appBody.includes('app')) {
      console.log('App might not be mounted.');
    } else {
      console.log('App body contains content.');
    }
    
    await page.screenshot({ path: 'G:\\Open Flow\\screenshot.png', fullPage: true });
    console.log('Screenshot saved');
  } catch (error) {
    console.error('Error during test:', error.message);
  } finally {
    await browser.close();
  }
})();
