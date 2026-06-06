// Smoke test: App Mappings add flow (Settings → App Mappings)
// Verifies that picking an app and clicking Add creates a visible mapping entry.
const { chromium } = require('playwright');
const { tauriMock } = require('./_tauri-mock.cjs');

const TARGET_URL = 'http://localhost:1420';
const TIMEOUT = 10_000;

(async () => {
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage();
  const errors = [];

  await page.addInitScript(tauriMock);

  page.on('pageerror', err => errors.push(`Page exception: ${err.message}`));
  page.on('console', msg => {
    if (msg.type() === 'error') errors.push(`Console error: ${msg.text()}`);
  });

  try {
    await page.goto(TARGET_URL, { waitUntil: 'networkidle', timeout: TIMEOUT });

    // Open Settings
    const settingsBtn = page.locator('.nav-item:has-text("Settings")');
    await settingsBtn.waitFor({ state: 'visible', timeout: 5_000 });
    await settingsBtn.click();
    await page.locator('.settings-modal').waitFor({ state: 'visible', timeout: 3_000 });
    console.log('Settings opened.');

    // Navigate to App Mappings section
    const appsSection = page.locator('.settings-nav-item:has-text("App Mappings")');
    await appsSection.waitFor({ state: 'visible', timeout: 3_000 });
    await appsSection.click();
    await page.locator('h2.settings-h:has-text("App Mappings")').waitFor({ state: 'visible', timeout: 3_000 });
    console.log('App Mappings section open.');

    // Click the search input to open the app picker
    const searchInput = page.locator('.app-search-input');
    await searchInput.waitFor({ state: 'visible', timeout: 3_000 });
    await searchInput.click();

    // App picker should show the mocked installed apps
    const chromeItem = page.locator('.app-picker-item:has-text("chrome.exe")');
    await chromeItem.waitFor({ state: 'visible', timeout: 3_000 });
    console.log('App picker opened with mock apps.');

    // Pick Google Chrome
    await chromeItem.click();
    console.log('Picked chrome.exe from picker.');

    // Select profile using the native <select>
    await page.locator('.profile-select').selectOption('casual');

    // Add button is now enabled (addExe is set)
    const addBtn = page.locator('button.btn-ghost.add-btn, button:has-text("Add")').first();
    await addBtn.waitFor({ state: 'visible', timeout: 2_000 });
    await addBtn.click();
    console.log('Clicked Add.');

    // Mapping must appear in the list
    await page.locator('.mapping-exe-pill:has-text("chrome.exe")').waitFor({ state: 'visible', timeout: 3_000 });
    console.log('chrome.exe mapping visible in list.');

    // Deleting should animate out instead of disappearing in a single frame
    const mappingRow = page.locator('.mapping-row').filter({ hasText: 'chrome.exe' }).first();
    const deleteBtn = mappingRow.locator('.mapping-delete-btn');
    await deleteBtn.click();
    await page.waitForTimeout(40);
    const rowsDuringDelete = await page.locator('.mapping-row').count();
    if (rowsDuringDelete < 1) {
      errors.push('Mapping row unmounted immediately after delete click');
    } else {
      const rowOpacity = await mappingRow.evaluate((el) => Number.parseFloat(getComputedStyle(el).opacity)).catch(() => NaN);
      if (!Number.isNaN(rowOpacity) && !(rowOpacity > 0 && rowOpacity < 1)) {
        errors.push(`Mapping row should be mid-animation after delete click, got opacity ${rowOpacity}`);
      } else {
        console.log(`chrome.exe mapping animating out with ${rowsDuringDelete} row in DOM.`);
      }
    }
    await mappingRow.waitFor({ state: 'hidden', timeout: 3_000 });
    console.log('chrome.exe mapping removed after animation.');

    if (errors.length > 0) {
      console.error('FAIL — JS errors during test:');
      errors.forEach(e => console.error('  ' + e));
      process.exit(1);
    }

    console.log('PASS — App Mappings add flow works end-to-end.');
  } catch (err) {
    console.error('FAIL — test threw:', err.message);
    process.exit(1);
  } finally {
    await browser.close();
  }
})();
