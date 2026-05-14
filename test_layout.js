import { chromium } from 'playwright';
import { spawn } from 'child_process';

const vite = spawn('npm', ['run', 'dev'], { shell: true });

vite.stdout.on('data', async (data) => {
  const output = data.toString();
  console.log(output);
  if (output.includes('http://localhost:1420') || output.includes('http://localhost:5173')) {
    const urlMatch = output.match(/http:\/\/localhost:\d+/);
    if (!urlMatch) return;
    const url = urlMatch[0];
    
    try {
      const browser = await chromium.launch({ headless: true });
      const page = await browser.newPage();
      
      // Let's set a realistic window size for Tauri app
      await page.setViewportSize({ width: 900, height: 600 });
      
      console.log(`Navigating to ${url}...`);
      await page.goto(url);
      
      // Wait for app to mount
      await page.waitForTimeout(1000);
      
      // The app has a Setup page overlay
      // We need to advance to Step 6
      // Step 0 -> intro actions btn-primary
      console.log('Progressing to Step 6...');
      
      let step = 0;
      while (step < 6) {
        try {
          const btn = await page.locator('.step-wrap.visible .btn-primary').first();
          await btn.click({ timeout: 2000 });
          await page.waitForTimeout(600); // Wait for transition
          step++;
          console.log(`Now at step ${step}`);
        } catch (e) {
          console.log('Could not find next button, maybe already at step 6?');
          break;
        }
      }
      
      await page.screenshot({ path: 'setup_step6.png' });
      console.log('Screenshot saved to setup_step6.png');
      
      const isGrid = await page.evaluate(() => {
        const qsCards = document.querySelector('.qs-cards');
        if (!qsCards) return false;
        const style = window.getComputedStyle(qsCards);
        return style.display === 'grid' && style.gridTemplateColumns.includes('1fr 1fr');
      });
      console.log('Grid Layout active:', isGrid);
      
      await browser.close();
    } catch (e) {
      console.error(e);
    } finally {
      vite.kill();
      process.exit(0);
    }
  }
});
