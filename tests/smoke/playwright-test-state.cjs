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

    // Transcription and Clean-up. Local models no longer get a section of
    // their own: downloading, deleting and prompt-editing them happens on the
    // model's own row inside each task's picker.
    const tiles = page.locator('.task-tile');
    const tileCount = await tiles.count();
    if (tileCount !== 2) errors.push(`Expected 2 model task tiles, found ${tileCount}`);

    // Model choice lives in a modal picker: Change sets the active model,
    // Add fallback appends to the chain and can never replace the default.
    const transcriptionTile = tiles.first();
    await transcriptionTile.locator('.tile-btn-primary').click();

    const picker = page.locator('.picker-card');
    await picker.waitFor({ state: 'visible', timeout: 3000 });

    const customInput = picker.locator('.custom-input');
    await customInput.fill('custom-test-model');
    await customInput.press('Enter');
    await page.waitForTimeout(300);

    // A hand-typed id becomes the active model, so it shows in the summary row.
    const activeModel = transcriptionTile.locator('.summary-item.model-chip');
    const activeText = (await activeModel.textContent()) || '';
    if (!activeText.includes('custom-test-model')) {
      errors.push(`Custom model did not become active, summary read "${activeText}"`);
    }

    await transcriptionTile.locator('.add-fallback').click();
    await picker.waitFor({ state: 'visible', timeout: 3000 });

    // Rows that still need a key or a download are calls to action, not
    // choices — clicking one opens setup instead of adding a fallback.
    const fallbackChoice = picker
      .locator('.row-main:visible')
      .filter({ hasNot: picker.locator('.row-state-cta') })
      .first();
    if (!(await fallbackChoice.count())) {
      errors.push('Fallback picker offered no models');
    } else {
      await fallbackChoice.click();
      await page.waitForTimeout(300);
    }

    const fallbackRows = transcriptionTile.locator('.fallback-chip-item');
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
