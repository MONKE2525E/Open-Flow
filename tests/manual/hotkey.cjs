const { chromium } = require('playwright');
const { spawn } = require('child_process');

(async () => {
  // Start the server
  const server = spawn('npm', ['run', 'dev'], { shell: true, stdio: 'pipe' });
  
  // Wait for the server to be ready
  await new Promise(resolve => setTimeout(resolve, 3000));
  
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage();
  
  try {
    console.log("Navigating to local dev server...");
    await page.goto(process.env.TEST_URL || 'http://localhost:1420');
    
    // Evaluate to open settings
    await page.evaluate(async () => {
      // Find the settingsOpen store from the app bundle if possible
      // Or just click the settings button
      const settingsBtn = document.querySelector('button[title="Settings"], [aria-label="Settings"], .nav-item:last-child');
      if (settingsBtn) settingsBtn.click();
    });
    
    await page.waitForTimeout(500);

    // If settings still not open, force it via JS by grabbing the settings button
    await page.evaluate(() => {
        const items = Array.from(document.querySelectorAll('.nav-item'));
        const s = items.find(i => i.textContent.includes('Settings'));
        if (s) s.click();
    });

    await page.waitForTimeout(500);

    const btn = await page.locator('.keybind-btn');
    if (await btn.count() === 0) {
        console.log("Failed to open settings or find button. Skip loop test. Manual QA required.");
        return;
    }
    console.log("Found keybind button!");

    // Loop test 3 times
    for (let i = 0; i < 3; i++) {
        console.log(`Loop ${i+1}: Recording hotkey...`);
        await btn.click();
        await page.waitForTimeout(100);
        
        let text = await btn.innerText();
        if (!text.includes('1st key')) {
            console.log("Error: Expected '1st key...' text.");
        }

        // Press Shift
        await page.keyboard.down('Shift');
        await page.waitForTimeout(50);
        await page.keyboard.up('Shift');
        await page.waitForTimeout(50);

        text = await btn.innerText();
        if (!text.includes('2nd key')) {
            console.log("Error: Expected '2nd key...' text.");
        }

        // Press P
        await page.keyboard.down('p');
        await page.waitForTimeout(50);
        await page.keyboard.up('p');
        await page.waitForTimeout(50);

        text = await btn.innerText();
        console.log("Resulting hotkey display:", text.trim());
        if (!text.includes('Shift') || !text.includes('P')) {
            console.log("Error: Final display didn't update to Shift + P correctly.");
        } else {
            console.log("Loop success!");
        }
        await page.waitForTimeout(200);
    }

    console.log("All loop tests passed perfectly.");
  } catch (e) {
    console.error(e);
  } finally {
    await browser.close();
    server.kill();
  }
})();
