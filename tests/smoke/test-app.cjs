const { chromium } = require('playwright');

const TARGET_URL = 'http://localhost:5173';

(async () => {
  console.log('Launching browser...');
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage();

  console.log('Navigating to target URL...');
  try {
    await page.goto(TARGET_URL, { waitUntil: 'networkidle', timeout: 5000 });
  } catch (e) {
    console.log('Retrying...');
    await new Promise(r => setTimeout(r, 1000));
    await page.goto(TARGET_URL, { waitUntil: 'networkidle', timeout: 5000 });
  }

  console.log('Page loaded:', await page.title());

  // Click on the Style tab in Sidebar
  // Wait, let's just see if we can click the "Style" button in the sidebar.
  // The sidebar might have a button with the text "Style" or an icon.
  // We can select it by text.
  await page.click('text=Style');
  console.log('Clicked Style tab.');

  // Now click on 'App Mappings' tab inside Style page
  await page.click('text=App Mappings');
  console.log('Clicked App Mappings tab.');

  // Verify the input exists
  await page.waitForSelector('input[placeholder="e.g. slack.exe"]');
  console.log('App Mappings UI is visible.');

  // Add a new mapping
  await page.fill('input[placeholder="e.g. slack.exe"]', 'chrome.exe');
  await page.selectOption('select', 'casual');
  await page.click('button:has-text("Add Mapping")');

  // Verify it was added
  await page.waitForSelector('text=chrome.exe');
  console.log('Mapping added successfully.');

  await browser.close();
  console.log('Test completed successfully.');
})();
