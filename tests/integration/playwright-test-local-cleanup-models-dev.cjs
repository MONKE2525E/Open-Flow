'use strict';

const { chromium } = require('playwright');
const { TARGET_URL, TIMEOUT, seedDevState, openSettings } = require('./_dev-helpers.cjs');

(async () => {
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage();
  const errors = [];

  await seedDevState(page, {
    settings: {
      setup_complete: true,
      force_setup_on_launch: false,
      advanced_model_ui: true,
      cleanup_provider: 'groq',
      cleanup_default_model: 'groq/qwen/qwen3.6-27b',
      cleanup_model: 'groq/qwen/qwen3.6-27b',
      cleanup_models_by_provider: {
        groq: ['qwen/qwen3.6-27b', 'openai/gpt-oss-20b'],
        openai: ['gpt-4o-mini', 'gpt-4o'],
        google: ['gemini-2.5-flash'],
        local: [],
      },
    },
    localSttModels: {
      'parakeet-v3': { downloaded: true },
    },
    localLlmModels: {
      'qwen2.5-3b-instruct': { downloaded: true },
    },
  });

  page.on('pageerror', (err) => errors.push(`Page exception: ${err.message}`));
  page.on('console', (msg) => {
    if (msg.type() === 'error') errors.push(`Console error: ${msg.text()}`);
  });

  try {
    await page.goto(TARGET_URL, { waitUntil: 'networkidle', timeout: TIMEOUT });
    await openSettings(page);
    await page.locator('.settings-nav-item:has-text("Models")').click();
    await page.locator('h2.settings-h:has-text("Models")').waitFor({ state: 'visible', timeout: TIMEOUT });

    const expectedOrder = ['Model selection', 'Model settings'];
    const subheads = await page.locator('.settings-page .panel h3.settings-subhead:visible').evaluateAll((els) => els
      .map((el) => el.textContent?.trim())
      .filter((text) => ['Model selection', 'Model settings'].includes(text)));
    if (subheads.join('|') !== expectedOrder.join('|')) {
      errors.push(`Models subsection order mismatch: ${subheads.join(' | ')}`);
    }

    const tiles = page.locator('.task-tile');
    if (await tiles.count() !== 2) errors.push(`Expected transcription and clean-up model tiles, found ${await tiles.count()}`);
    const cleanupTile = tiles.filter({ has: page.locator('.head-title', { hasText: 'Clean-up' }) }).first();

    await cleanupTile.locator('.tile-btn-primary').click();
    const picker = page.locator('.picker-card');
    await picker.waitFor({ state: 'visible', timeout: TIMEOUT });
    await picker.locator('.rail-item:has-text("Local")').click();
    const installedLocalRow = picker.locator('.model-row').filter({ hasText: 'Qwen 2.5 3B Instruct' });
    await installedLocalRow.waitFor({ state: 'visible', timeout: TIMEOUT });
    if (!(await installedLocalRow.locator('.row-note:has-text("Installed")').isVisible().catch(() => false))) {
      errors.push('Picker did not mark the installed local cleanup model as installed');
    }

    // Clicking the installed local model row selects it as the active cleanup
    await installedLocalRow.locator('.row-main').click();
    await page.waitForFunction(
      () => {
        try {
          return JSON.parse(localStorage.getItem('verenu:dev-settings') || '{}').cleanup_default_model === 'local/qwen2.5-3b-instruct';
        } catch {
          return false;
        }
      },
      null,
      { timeout: TIMEOUT },
    );
    const stored = await page.evaluate(() => JSON.parse(localStorage.getItem('verenu:dev-settings') || '{}'));
    if (stored.cleanup_default_model !== 'local/qwen2.5-3b-instruct') {
      errors.push(`cleanup_default_model did not persist local selection: ${stored.cleanup_default_model}`);
    }
    const providerChip = (await cleanupTile.locator('.provider-chip').textContent()) || '';
    if (!providerChip.toLowerCase().includes('local')) {
      errors.push('Clean-up summary chip did not update to Local after selecting a local cleanup model');
    }

    await cleanupTile.locator('.tile-btn-primary').click();
    await picker.waitFor({ state: 'visible', timeout: TIMEOUT });
    await picker.locator('.rail-item:has-text("Local")').click();
    const gemmaRow = picker.locator('.model-row').filter({ hasText: 'Gemma 4 E2B' });
    await gemmaRow.locator('[data-testid="download-model"]').click();
    await gemmaRow.locator('[data-testid="cancel-model-download"]').waitFor({ state: 'visible', timeout: TIMEOUT });
    await gemmaRow.locator('[data-testid="cancel-model-download"]').click();
    await gemmaRow.locator('[data-testid="download-model"]').waitFor({ state: 'visible', timeout: TIMEOUT });

    // One cleanup prompt covers every model now, so it is edited from the tile
    // rather than from a per-model button inside the picker.
    await picker.locator('.picker-close').click();
    await picker.waitFor({ state: 'hidden', timeout: TIMEOUT });
    await cleanupTile.locator('.tile-btn', { hasText: 'Edit prompt' }).click();
    const promptCard = page.locator('.prompt-modal-card');
    await promptCard.waitFor({ state: 'visible', timeout: TIMEOUT });
    await promptCard.locator('.prompt-btn:has-text("Save")').click();
    await promptCard.waitFor({ state: 'hidden', timeout: TIMEOUT });

    if (errors.length) {
      console.error('FAIL');
      for (const error of errors) console.error(`  ${error}`);
      process.exitCode = 1;
    } else {
      console.log('PASS - local cleanup models UI behaved correctly in browser dev mode.');
    }
  } catch (err) {
    console.error(`FAIL - local cleanup models test threw: ${err.message}`);
    process.exitCode = 1;
  } finally {
    await browser.close();
  }
})();

