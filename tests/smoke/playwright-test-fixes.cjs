const { chromium } = require('playwright');
const TARGET_URL = 'http://localhost:1420';

(async () => {
  console.log('Starting Fix Assertions for Open Flow...');
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage();
  const errors = [];
  
  try {
    await page.goto(TARGET_URL, { waitUntil: 'networkidle' });

    console.log('Testing Settings page badges...');
    await page.locator('.nav-item:has-text("Settings")').click();
    await page.waitForTimeout(500);

    // Advanced tab badges
    await page.locator('.settings-nav-item:has-text("Advanced")').click();
    await page.waitForTimeout(500);
    const historyBadge = page.locator('div.badge:has-text("30 days")');
    if (!(await historyBadge.isVisible())) errors.push('30 days badge not found or not a div.badge!');

    const clipboardBadge = page.locator('div.badge:has-text("Clipboard (Ctrl+V)")');
    if (!(await clipboardBadge.isVisible())) errors.push('Clipboard badge not found or not a div.badge!');

    // General tab kbd badge
    await page.locator('.settings-nav-item:has-text("General")').click();
    await page.waitForTimeout(500);
    const hotkeyBadge = page.locator('kbd.badge.key-badge:has-text("Alt Space")');
    if (!(await hotkeyBadge.isVisible())) errors.push('Alt Space badge not found or not a kbd.badge!');

    // About tab GitHub link
    await page.locator('.settings-nav-item:has-text("About")').click();
    await page.waitForTimeout(500);
    const githubBtn = page.locator('button.btn-ghost:has-text("github.com/MONKE2525E/Open-Flow")');
    if (!(await githubBtn.isVisible())) errors.push('GitHub button not found!');

    if (errors.length === 0) {
      console.log('✅ All fix assertions passed successfully!');
    } else {
      console.log('❌ Errors found:');
      errors.forEach(e => console.log('- ' + e));
    }
  } catch(e) {
    console.error('Test execution failed:', e.message);
  } finally {
    await browser.close();
  }
})();
