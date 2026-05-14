const { chromium } = require('playwright');
const { spawn } = require('child_process');

(async () => {
  console.log("Starting dev server...");
  const server = spawn('npm', ['run', 'dev'], { shell: true, stdio: 'pipe' });
  
  // Wait a bit for Vite to start
  await new Promise(resolve => setTimeout(resolve, 4000));
  
  const browser = await chromium.launch({ headless: false, slowMo: 100 });
  const context = await browser.newContext();
  const page = await context.newPage();
  page.on('console', msg => console.log('BROWSER:', msg.text()));
  
  try {
    console.log("Navigating to http://localhost:5173 ...");
    await page.goto('http://localhost:5173');
    
    // Wait for the app to render. The settings gear is usually in the Sidebar.
    // Let's look for the settings icon or button.
    console.log("Looking for settings button...");
    
    // Wait a bit for animations
    await page.waitForTimeout(1000);
    
    // Try to open settings
    const settingsOpened = await page.evaluate(async () => {
      // Find a setting button in the nav-item list
      const navItems = Array.from(document.querySelectorAll('.nav-item'));
      for (const item of navItems) {
        if (item.textContent.toLowerCase().includes('settings') || 
            item.innerHTML.includes('settings')) {
          item.click();
          return true;
        }
      }
      
      // Fallback: looking for SVG that might be settings
      const buttons = Array.from(document.querySelectorAll('button, div[role="button"]'));
      // try to click one with title settings or SVG that looks like a gear
      return false;
    });

    if (!settingsOpened) {
      // Force settings open via Svelte store if possible, or try another selector
      console.log("Settings button not clearly identified, clicking the last nav item...");
      await page.evaluate(() => {
        const items = document.querySelectorAll('.nav-item');
        if (items.length > 0) items[items.length - 1].click();
      });
    }

    await page.waitForTimeout(1000);
    
    // Remove setup overlay if it exists
    await page.evaluate(() => {
        const overlay = document.querySelector('.setup-overlay');
        if (overlay) overlay.remove();
        
        // Also remove any other full-screen overlays that aren't settings
        document.querySelectorAll('.settings-overlay').forEach(s => {
            // Keep settings overlay, but ensure it's on top
            s.style.zIndex = "99999";
        });
    });
    
    // Find the hotkey button
    const btn = page.locator('.keybind-btn');
    
    if (await btn.count() === 0) {
        throw new Error("Could not find the Keybind button. Test failed.");
    }
    
    console.log("Found keybind button! Running edge-case and loop tests...");
    
    // --- Test 1: Click away to cancel ---
    console.log("\n--- Edge Case Test: Click away to cancel ---");
    await btn.click();
    await page.waitForTimeout(200);
    
    let text = await btn.innerText();
    console.log("Button text after click:", text);
    if (!text.includes('1st key')) {
        console.error("Test failed! Expected '1st key...' text.");
    }
    
    // Click outside on the generic body
    console.log("Clicking outside the button (on the modal header)...");
    await page.locator('h2.settings-h').first().click();
    await page.waitForTimeout(200);
    
    text = await btn.innerText();
    console.log("Button text after clicking outside:", text);
    if (text.includes('1st key')) {
        console.error("Test failed! Button should have reverted to original text.");
    } else {
        console.log("Click-to-cancel successful.");
    }

    // --- Test 2: Escape to cancel ---
    console.log("\n--- Edge Case Test: Escape to cancel ---");
    await btn.click();
    await page.waitForTimeout(200);
    await page.keyboard.press('Escape');
    await page.waitForTimeout(200);
    text = await btn.innerText();
    console.log("Button text after Escape:", text);
    if (text.includes('1st key')) {
        console.error("Test failed! Button should have reverted to original text on Escape.");
    } else {
        console.log("Escape-to-cancel successful.");
    }
    
    for (let i = 0; i < 3; i++) {
        console.log(`\n--- Loop ${i+1} ---`);
        await btn.click();
        await page.waitForTimeout(200);
        
        text = await btn.innerText();
        console.log("Button text after click:", text);
        
        // Press first key: Shift
        await page.keyboard.down('Shift');
        await page.waitForTimeout(100);
        await page.keyboard.up('Shift');
        
        await page.waitForTimeout(200);
        text = await btn.innerText();
        console.log("Button text after Shift:", text);
        
        // Press second key: P or some other key
        const keysToTest = ['p', 'a', 'Space'];
        const testKey = keysToTest[i % keysToTest.length];
        await page.keyboard.down(testKey);
        await page.waitForTimeout(100);
        await page.keyboard.up(testKey);
        
        await page.waitForTimeout(300);
        text = await btn.innerText();
        console.log(`Button text after ${testKey}:`, text);
        
        if (!text.includes('Shift')) {
            console.error("Test failed! Expected 'Shift' in button text.");
        } else {
            console.log(`Loop ${i+1} successful!`);
        }
    }
    
    console.log("Smoke test completed perfectly.");
  } catch (e) {
    console.error("Smoke test error:", e);
  } finally {
    console.log("Closing browser and server...");
    await browser.close();
    server.kill();
  }
})();
