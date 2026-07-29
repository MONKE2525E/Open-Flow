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
      cleanup_default_model: 'groq/llama-3.3-70b-versatile',
      cleanup_model: 'groq/llama-3.3-70b-versatile',
      cleanup_models_by_provider: {
        groq: ['llama-3.3-70b-versatile', 'llama-3.1-8b-instant'],
        openai: ['gpt-4o-mini', 'gpt-4o'],
        google: ['gemini-2.5-flash', 'gemini-3.5-flash'],
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

    const subheads = await page.locator('h2.settings-h:has-text("Models") ~ .settings-subhead').evaluateAll((els) => els.map((el) => el.textContent?.trim()).filter(Boolean));
    const expectedOrder = ['Model selection', 'Local models', 'Model settings'];
    if (subheads.join('|') !== expectedOrder.join('|')) {
      errors.push(`Models subsection order mismatch: ${subheads.join(' | ')}`);
    }

    const tileCount = await page.locator('.task-tile').count();
    if (tileCount !== 3) errors.push(`Expected 3 top-level model tiles, found ${tileCount}`);

    const cleanupTile = page.locator('.task-tile').nth(1);
    const cleanupHead = cleanupTile.locator('.tile-head');
    if ((await cleanupHead.getAttribute('aria-expanded')) !== 'true') {
      await cleanupHead.click();
    }
    await cleanupTile.locator('.simple-provider:has-text("Local")').waitFor({ state: 'visible', timeout: TIMEOUT });
    if (!(await cleanupTile.locator('.model-row:has-text("Qwen 2.5 3B Instruct")').isVisible().catch(() => false))) {
      errors.push('Clean-up tile did not show the installed local cleanup model');
    }
    if (await cleanupTile.locator('.model-row:has-text("Gemma 4 E2B")').count()) {
      errors.push('Clean-up tile showed a non-installed local cleanup model');
    }

    await cleanupTile.locator('.model-row:has-text("Qwen 2.5 3B Instruct")').click();
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
    const providerChip = (await cleanupTile.locator('.summary-item').first().textContent()) || '';
    if (!providerChip.toLowerCase().includes('local')) {
      errors.push('Clean-up summary chip did not update to Local after selecting a local cleanup model');
    }

    const downloadsTile = page.locator('.task-tile').nth(2);
    const downloadsHead = downloadsTile.locator('.tile-head');
    if ((await downloadsHead.getAttribute('aria-expanded')) !== 'true') {
      await downloadsHead.click();
    }
    await downloadsTile.locator('h3:has-text("Transcription models")').waitFor({ state: 'visible', timeout: TIMEOUT });
    await downloadsTile.locator('h3:has-text("Cleanup models")').waitFor({ state: 'visible', timeout: TIMEOUT });

    const advancedToggle = page.locator('[role="switch"][aria-label="Advanced Models"]');
    await advancedToggle.waitFor({ state: 'visible', timeout: TIMEOUT });
    if ((await advancedToggle.getAttribute('aria-checked')) !== 'true') {
      await advancedToggle.click();
      await page.waitForFunction(
        () => document.querySelector('[role="switch"][aria-label="Advanced Models"]')?.getAttribute('aria-checked') === 'true',
        null,
        { timeout: TIMEOUT },
      );
    }

    const cleanupBlock = downloadsTile.locator('#cleanup-models-block');
    if (!(await cleanupBlock.locator('button:has-text("Show 6 more")').isVisible().catch(() => false))) {
      errors.push('Cleanup downloads block did not render its Show more control');
    }

    const gemmaCard = cleanupBlock.locator('[data-model-type="cleanup"][data-model-id="gemma-4-e2b"]').first();
    await gemmaCard.locator('[data-testid="edit-prompt"]').waitFor({ state: 'visible', timeout: TIMEOUT });
    await gemmaCard.locator('[data-testid="edit-prompt"]').click();
    await page.locator('.prompt-modal-card').waitFor({ state: 'visible', timeout: TIMEOUT });
    await page.locator('.prompt-modal-card .prompt-btn:has-text("Save")').click();
    await page.locator('.prompt-modal-card').waitFor({ state: 'hidden', timeout: TIMEOUT });

    await gemmaCard.locator('[data-testid="download-model"]').click();
    await gemmaCard.locator('[data-testid="cancel-model-download"]').waitFor({ state: 'visible', timeout: TIMEOUT });
    await gemmaCard.locator('[data-testid="delete-model"]').waitFor({ state: 'visible', timeout: TIMEOUT });

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
