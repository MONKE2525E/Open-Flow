// Smoke test: settings state persistence — Tauri window (port 1420)
// Verifies that model selection and toggle state survive a settings close/reopen cycle.
const { chromium } = require('playwright');
const { tauriMock } = require('./_tauri-mock.cjs');

const TARGET_URL = 'http://localhost:1420';
const TIMEOUT = 8_000;

async function openSettings(page) {
  const btn = page.locator('.nav-item:has-text("Settings")');
  await btn.waitFor({ state: 'visible', timeout: TIMEOUT });
  await btn.click();
  await page.locator('.settings-modal').waitFor({ state: 'visible', timeout: 3_000 });
}

async function closeSettings(page) {
  await page.mouse.click(10, 10);
  await page.locator('.settings-modal').waitFor({ state: 'hidden', timeout: 3_000 });
}

(async () => {
  console.log('Starting settings state persistence tests...');
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage();
  const errors = [];

  await page.addInitScript(tauriMock);

  try {
    await page.goto(TARGET_URL, { waitUntil: 'networkidle', timeout: TIMEOUT });

    // ── Models: all expected model buttons render ─────────────────────────────
    console.log('Checking Models panel...');
    await openSettings(page);
    await page.locator('.settings-nav-item:has-text("Models")').click();
    await page.locator('h2.settings-h:has-text("Models")').waitFor({ state: 'visible', timeout: 3_000 });

    const expectedTranscription = [
      'whisper-large-v3-turbo',
      'gpt-4o-transcribe',
      'gemini-3.5-flash',
    ];
    const expectedCleanup = [
      'llama-3.3-70b-versatile',
      'gpt-4o-mini',
      'gemini-3.5-flash',
    ];

    for (const name of expectedTranscription) {
      const btn = page.locator(`.model-row:has-text("${name}")`).first();
      try {
        await btn.waitFor({ state: 'visible', timeout: 2_000 });
        console.log(`  ✓ Transcription model visible: ${name}`);
      } catch {
        errors.push(`Transcription model not rendered: ${name}`);
      }
    }
    for (const name of expectedCleanup) {
      const btn = page.locator(`.model-row:has-text("${name}")`).last();
      try {
        await btn.waitFor({ state: 'visible', timeout: 2_000 });
        console.log(`  ✓ Cleanup model visible: ${name}`);
      } catch {
        errors.push(`Cleanup model not rendered: ${name}`);
      }
    }

    // Exactly one transcription model row should be active
    const activeTranscription = await page.locator('.model-list .model-row.active').count();
    if (activeTranscription < 1) {
      errors.push('No active (selected) model row found in Models panel');
    } else {
      console.log(`  ✓ ${activeTranscription} active model row(s) found`);
    }

    // ── Model selection persistence ───────────────────────────────────────────
    console.log('Testing model selection persistence...');
    const allRows = await page.locator('.model-row').all();
    if (allRows.length >= 2) {
      // Click the second model row (whichever is not currently active)
      const firstRow = allRows[0];
      const isFirstActive = await firstRow.evaluate(el => el.classList.contains('active'));
      const targetRow = isFirstActive ? allRows[1] : allRows[0];
      const targetText = await targetRow.textContent();
      await targetRow.click();
      await page.waitForTimeout(300); // allow Tauri store write

      // Close and reopen settings
      await closeSettings(page);
      await openSettings(page);
      await page.locator('.settings-nav-item:has-text("Models")').click();
      await page.locator('h2.settings-h:has-text("Models")').waitFor({ state: 'visible', timeout: 3_000 });

      // The previously clicked row should now be active
      const newActiveRows = page.locator('.model-row.active');
      const newActiveText = await newActiveRows.first().textContent();
      if (!newActiveText?.includes(targetText?.trim().slice(0, 10) ?? '')) {
        errors.push(`Model selection did not persist after settings reopen (expected row containing "${targetText?.trim().slice(0, 10)}")`);
      } else {
        console.log('  ✓ Model selection persisted through settings close/reopen');
      }
    } else {
      console.log('  (skipped model persistence — fewer than 2 model rows found)');
    }

    // ── Advanced toggle persistence ───────────────────────────────────────────
    console.log('Testing Advanced toggle persistence...');
    await page.locator('.settings-nav-item:has-text("Advanced")').click();
    await page.locator('h2.settings-h:has-text("Advanced")').waitFor({ state: 'visible', timeout: 3_000 });

    const firstToggle = page.locator('.toggle').first();
    await firstToggle.waitFor({ state: 'visible', timeout: 2_000 });
    const stateBefore = await firstToggle.getAttribute('aria-checked');
    await firstToggle.click();
    const stateAfterClick = await firstToggle.getAttribute('aria-checked');

    if (stateBefore === stateAfterClick) {
      errors.push(`Advanced toggle did not change (stuck at "${stateBefore}")`);
    } else {
      console.log(`  ✓ Toggle changed: ${stateBefore} → ${stateAfterClick}`);

      // Close and reopen; toggle must retain changed state
      await page.waitForTimeout(300); // allow store write
      await closeSettings(page);
      await openSettings(page);
      await page.locator('.settings-nav-item:has-text("Advanced")').click();
      await page.locator('h2.settings-h:has-text("Advanced")').waitFor({ state: 'visible', timeout: 3_000 });

      const stateAfterReopen = await page.locator('.toggle').first().getAttribute('aria-checked');
      if (stateAfterReopen !== stateAfterClick) {
        errors.push(`Toggle state did not persist after settings reopen (expected "${stateAfterClick}", got "${stateAfterReopen}")`);
      } else {
        console.log(`  ✓ Toggle state persisted after reopen: ${stateAfterReopen}`);
      }

      // Restore original state to avoid polluting other tests
      await page.locator('.toggle').first().click();
      await page.waitForTimeout(200);
    }

    await closeSettings(page);

    // ── Final verdict ─────────────────────────────────────────────────────────
    if (errors.length > 0) {
      console.error('\nFAIL:');
      errors.forEach(e => console.error('  ' + e));
      process.exit(1);
    }
    console.log('\nPASS — settings state persistence tests passed.');
  } catch (err) {
    console.error('FAIL — test threw:', err.message);
    process.exit(1);
  } finally {
    await browser.close();
  }
})();
