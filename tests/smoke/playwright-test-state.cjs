const { chromium } = require('playwright');
const { tauriMock, APP_VERSION } = require('./_tauri-mock.cjs');

const TARGET_URL = 'http://localhost:1420';
const TIMEOUT = 10000;

async function openSettings(page) {
  const btn = page.locator('.nav-item:has-text("Settings")');
  await btn.waitFor({ state: 'visible', timeout: TIMEOUT });
  await btn.click();
  await page.locator('.settings-page').waitFor({ state: 'visible', timeout: 3000 });
}

// Leaves via the rail's own control rather than a click in the app gutter:
// settings is a full-screen page now, so this is the affordance a user reaches
// for, and it doesn't depend on the window's margin geometry. The gutter
// dismiss still has dedicated coverage in playwright-test-ui.cjs.
async function closeSettings(page) {
  await page.locator('.settings-back').click({ timeout: TIMEOUT });
  await page.locator('.settings-page').waitFor({ state: 'hidden', timeout: 3000 });
}

(async () => {
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage();
  const errors = [];

  await page.addInitScript(tauriMock, { appVersion: APP_VERSION });

  try {
    await page.goto(TARGET_URL, { waitUntil: 'networkidle', timeout: TIMEOUT });
    await openSettings(page);
    await page.locator('.settings-nav-item:has-text("Models")').click();
    await page.locator('h2.settings-h:has-text("Models")').waitFor({ state: 'visible', timeout: 3000 });

    // Transcription, Cleanup, and Local Models (the Models tab redesign
    // unified cloud+local selection into the Transcription tile and gave
    // local model download/management its own always-visible tile).
    const tiles = page.locator('.task-tile');
    const tileCount = await tiles.count();
    if (tileCount !== 3) errors.push(`Expected 3 model task tiles, found ${tileCount}`);

    const transcriptionTileBtn = page.locator('.tile-head').first();
    await transcriptionTileBtn.click();
    await page.waitForTimeout(200);

    const openTile = page.locator('.task-tile.task-open').first();
    await openTile.waitFor({ state: 'visible', timeout: 3000 });

    const customInput = openTile.locator('.model-input').first();
    await customInput.fill('custom-test-model');
    await customInput.press('Enter');
    await page.waitForTimeout(250);

    const added = openTile.locator('.model-name:has-text("custom-test-model")').first();
    if (!(await added.count())) errors.push('Custom model was not added');

    const addFallback = openTile.locator('.model-row').filter({ hasText: 'custom-test-model' }).locator('button:has-text("Add fallback")').first();
    if (await addFallback.count()) {
      await addFallback.click();
      await page.waitForTimeout(250);
    } else {
      errors.push('Add fallback button missing for custom model');
    }

    const fallbackRows = openTile.locator('.chain-row');
    if ((await fallbackRows.count()) < 1) errors.push('Fallback chain did not show added model');

    await closeSettings(page);
    await openSettings(page);
    await page.locator('.settings-nav-item:has-text("Models")').click();

    const tileSummary = page.locator('.task-tile').first().locator('.summary-item').nth(2);
    const summaryText = (await tileSummary.textContent()) || '';
    if (!summaryText.includes('1')) errors.push('Fallback summary count did not persist');

    if (errors.length) {
      console.error('\nFAIL:');
      for (const err of errors) console.error(`  ${err}`);
      process.exit(1);
    }

    console.log('PASS - model tile persistence and fallback flow passed.');
  } catch (err) {
    console.error(`FAIL - test threw: ${err.message}`);
    process.exit(1);
  } finally {
    await browser.close();
  }
})();
