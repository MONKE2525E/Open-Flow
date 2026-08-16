'use strict';

const { chromium } = require('playwright');
const { TARGET_URL, TIMEOUT, seedDevState, openSettings, closeSettings } = require('./_dev-helpers.cjs');

(async () => {
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage();
  const errors = [];

  await seedDevState(page, {
    settings: {
      setup_complete: true,
      cleanup_enabled: true,
      cleanup_intensity: 'medium',
      default_tone: 'casual',
      appearance_mode: 'system',
      legacy_features_enabled: true,
      history_retention: '30 days',
      force_setup_on_launch: false,
    },
  });

  page.on('pageerror', (err) => errors.push(`Page exception: ${err.message}`));
  page.on('console', (msg) => {
    if (msg.type() === 'error') errors.push(`Console error: ${msg.text()}`);
  });

  try {
    await page.goto(TARGET_URL, { waitUntil: 'networkidle', timeout: TIMEOUT });
    await page.locator('h1.page-h:has-text("Welcome back")').waitFor({ state: 'visible', timeout: TIMEOUT });

    await page.locator('.nav-item:has-text("Snippets")').click();
    await page.locator('h1.page-h:has-text("Snippets")').waitFor({ state: 'visible', timeout: TIMEOUT });
    const newSnippetButton = page.locator('.toolbar .btn-primary:has-text("New snippet")');
    await newSnippetButton.click();
    const snippetModal = page.locator('.snippet-modal-card');
    await page.waitForTimeout(260);
    const modalRect = await snippetModal.boundingBox();
    const viewport = page.viewportSize();
    if (!modalRect || !viewport || Math.abs((modalRect.x + modalRect.width / 2) - viewport.width / 2) > 1 || Math.abs((modalRect.y + modalRect.height / 2) - viewport.height / 2) > 1) {
      errors.push('Snippet modal was not centered in the viewport');
    }
    const snippetFocusInside = await snippetModal.evaluate((el) => el.contains(document.activeElement));
    if (!snippetFocusInside) errors.push('Snippet modal did not move focus inside the dialog');
    await page.keyboard.press('Escape');
    await snippetModal.waitFor({ state: 'hidden', timeout: TIMEOUT });
    if (!(await newSnippetButton.evaluate((el) => document.activeElement === el))) {
      errors.push('Snippet modal did not restore focus to the trigger');
    }
    await newSnippetButton.click();
    await page.locator('#trigger-input').fill('sig');
    await page.locator('#expansion-input').fill('Best regards,\nThe Team');
    await page.locator('#instructions-input').fill('all capitals');
    await page.locator('.snippet-modal-card .btn-primary:has-text("Add snippet")').click();
    await page.locator('.snip-row:has-text("sig")').waitFor({ state: 'visible', timeout: TIMEOUT });
    if (!(await page.locator('.insp-instructions').isVisible().catch(() => false))) {
      await page.locator('.snip-row:has-text("sig")').click();
    }
    const snippetInspector = (await page.locator('.insp-instructions').textContent()) || '';
    if (!snippetInspector.includes('all capitals')) errors.push('Snippet inspector did not show cleanup instructions');

    await page.locator('.nav-item:has-text("Dictionary")').click();
    await page.locator('h1.page-h:has-text("Dictionary")').waitFor({ state: 'visible', timeout: TIMEOUT });
    const addTermButton = page.locator('.toolbar .btn-primary:has-text("Add term")');
    await addTermButton.click();
    const dictionaryModal = page.locator('.modal-card');
    const dictionaryFocusInside = await dictionaryModal.evaluate((el) => el.contains(document.activeElement));
    if (!dictionaryFocusInside) errors.push('Dictionary modal did not move focus inside the dialog');
    await page.keyboard.press('Escape');
    await dictionaryModal.waitFor({ state: 'hidden', timeout: TIMEOUT });
    if (!(await addTermButton.evaluate((el) => document.activeElement === el))) {
      errors.push('Dictionary modal did not restore focus to the trigger');
    }
    await addTermButton.click();
    await page.locator('#dict-term').fill('Verenu');
    await page.locator('#dict-mistake').fill('verenu');
    await page.locator('.modal-card .btn-primary:has-text("Add term")').click();
    await page.locator('.dict-row:has-text("Verenu")').waitFor({ state: 'visible', timeout: TIMEOUT });
    if (!(await page.locator('.insp-often').isVisible().catch(() => false))) {
      await page.locator('.dict-row:has-text("Verenu")').click();
    }
    const dictInspector = (await page.locator('.insp-often').textContent()) || '';
    if (!dictInspector.includes('verenu')) errors.push('Dictionary inspector did not show mistake text');

    await page.locator('.nav-item:has-text("Style")').click();
    await page.locator('h1.page-h:has-text("Style")').waitFor({ state: 'visible', timeout: TIMEOUT });
    await page.locator('.style-card:has-text("Strong")').click();
    await page.locator('.tab:has-text("Personal Tone")').click();
    await page.locator('.style-card:has-text("Formal")').click();

    await openSettings(page);
    await page.locator('.settings-nav-item:has-text("Privacy")').click();
    const historyRetentionButton = page.getByRole('button', { name: 'Transcription history retention' });
    await historyRetentionButton.click();
    await page.locator('.mic-item:has-text("Forever")').click();
    await page.getByLabel('Auto-learn corrections').click();

    await page.locator('.settings-nav-item:has-text("About")').click();
    const versionButton = page.locator('.version-tap');
    for (let i = 0; i < 10; i++) {
      await versionButton.click();
    }
    await page.locator('.dev-hint:has-text("Developer mode enabled")').waitFor({ state: 'visible', timeout: TIMEOUT });

    await page.locator('.settings-nav-item:has-text("Developer")').click();
    await page.locator('h2.settings-h:has-text("Developer")').waitFor({ state: 'visible', timeout: TIMEOUT });
    await page.getByRole('button', { name: 'Download Logs' }).click();
    const exportStatus = (await page.locator('.export-status').textContent()) || '';
    if (!exportStatus.includes('browser-dev://verenu-logs.txt')) {
      errors.push('Developer log export did not report the dev download path');
    }
    await closeSettings(page);

    await openSettings(page);
    await page.locator('.settings-nav-item:has-text("Privacy")').click();
    const retentionText = (await page.getByRole('button', { name: 'Transcription history retention' }).locator('span').first().textContent()) || '';
    if (!retentionText.includes('Forever')) errors.push('Privacy retention did not persist across reopen');
    await closeSettings(page);

    const persisted = await page.evaluate(() => JSON.parse(localStorage.getItem('verenu:dev-settings') || '{}'));
    if (persisted.cleanup_intensity !== 'high') errors.push('Style cleanup choice did not persist');
    if (persisted.default_tone !== 'formal') errors.push('Style tone choice did not persist');
    if (persisted.history_retention !== 'Forever') errors.push('Privacy retention did not persist to dev settings');
    if (persisted.auto_learn_enabled !== true) errors.push('Privacy auto-learn toggle did not persist');

    const storedSnippets = await page.evaluate(() => JSON.parse(localStorage.getItem('verenu:dev-snippets') || '[]'));
    if (!storedSnippets.some((entry) => entry.trigger === 'sig')) errors.push('Snippet CRUD did not persist to local storage');
    const storedDictionary = await page.evaluate(() => JSON.parse(localStorage.getItem('verenu:dev-dictionary') || '[]'));
    if (!storedDictionary.some((entry) => entry.term === 'Verenu')) errors.push('Dictionary CRUD did not persist to local storage');

    if (errors.length > 0) {
      console.error('FAIL');
      for (const error of errors) console.error(`  ${error}`);
      process.exit(1);
    }

    console.log('PASS - snippets, dictionary, style, privacy, and developer surfaces behaved in browser dev mode.');
  } catch (err) {
    console.error(`FAIL - surface test threw: ${err.message}`);
    process.exit(1);
  } finally {
    await browser.close();
  }
})();
