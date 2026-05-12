const { chromium } = require('playwright');

const TARGET_URL = 'http://localhost:1420';

(async () => {
  console.log('Starting UI Button Tests for Open Flow...');
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage();
  
  const errors = [];
  page.on('pageerror', err => {
    errors.push(`Page Error: ${err.message}`);
  });
  page.on('console', msg => {
    if (msg.type() === 'error' || msg.type() === 'warning') {
      errors.push(`Console ${msg.type()}: ${msg.text()}`);
    }
  });

  try {
    await page.goto(TARGET_URL, { waitUntil: 'networkidle' });
    console.log('Page loaded successfully');

    // Test Navigation Buttons
    const navItems = ['Home', 'Dictionary', 'Snippets', 'Style'];
    for (const item of navItems) {
      console.log(`Testing nav button: ${item}`);
      const btn = page.locator(`.nav-item:has-text("${item}")`);
      await btn.click();
      await page.waitForTimeout(200); // give time for any UI updates or errors
    }

    // Test Settings Button
    console.log('Testing Settings button...');
    const settingsBtn = page.locator(`.nav-item:has-text("Settings")`);
    await settingsBtn.click();
    await page.waitForTimeout(500);

    // Verify Settings modal is visible
    const settingsModal = page.locator('.settings-modal');
    if (await settingsModal.isVisible()) {
      console.log('Settings modal opened successfully.');
      
      // Test Settings Navigation
      const settingsSections = ['General', 'API Keys', 'Models', 'Privacy', 'Advanced', 'About'];
      for (const section of settingsSections) {
        console.log(`Clicking Settings -> ${section}`);
        const secBtn = page.locator(`.settings-nav-item:has-text("${section}")`);
        await secBtn.click();
        await page.waitForTimeout(300);
      }
      
      // Test toggling Auto-cleanup
      console.log('Testing toggles in Privacy...');
      const privacyBtn = page.locator(`.settings-nav-item:has-text("Privacy")`);
      await privacyBtn.click();
      await page.waitForTimeout(300);
      
      const toggles = await page.locator('.toggle').all();
      console.log(`Found ${toggles.length} toggles on Privacy page`);
      for (let i = 0; i < toggles.length; i++) {
        await toggles[i].click();
        await page.waitForTimeout(200);
      }
      
      // Close Settings
      console.log('Closing settings...');
      // Click on overlay to close
      await page.mouse.click(10, 10);
      await page.waitForTimeout(500);
    } else {
      console.log('Settings modal did not open!');
      errors.push('Settings modal did not open');
    }

    console.log('\n--- Test Results ---');
    if (errors.length > 0) {
      console.log('Errors found:');
      errors.forEach(e => console.log('- ' + e));
    } else {
      console.log('No UI errors found during basic interaction.');
    }

  } catch (error) {
    console.error('Test execution failed:', error.message);
  } finally {
    await browser.close();
  }
})();
