'use strict';

const { chromium } = require('playwright');
const { TARGET_URL, TIMEOUT, seedDevState } = require('./_dev-helpers.cjs');

(async () => {
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage();
  const errors = [];

  await seedDevState(page, {
    settings: {
      force_setup_on_launch: true,
      setup_complete: false,
      appearance_mode: 'system',
      transcription_language: 'en',
    },
  });

  page.on('pageerror', (err) => errors.push(`Page exception: ${err.message}`));
  page.on('console', (msg) => {
    if (msg.type() === 'error') errors.push(`Console error: ${msg.text()}`);
  });

  try {
    await page.goto(TARGET_URL, { waitUntil: 'networkidle', timeout: TIMEOUT });

    await page.getByRole('button', { name: 'Get Started' }).click();
    await page.locator('.provider-card:has-text("OpenAI")').click();
    await page.getByRole('button', { name: 'Next' }).click();
    await page.getByRole('button', { name: 'Continue' }).click();

    const permissionHeading = page.getByRole('heading', { name: 'Check your macOS permissions' });
    let hasPermissionStep = false;
    try {
      await permissionHeading.waitFor({ state: 'visible', timeout: 1000 });
      hasPermissionStep = true;
    } catch (e) {
      // Heading did not appear (not on macOS or step skipped)
    }
    if (hasPermissionStep) {
      await page.getByRole('button', { name: 'Next' }).click();
    }

    await page.locator('button.option-card', { has: page.locator('.option-name', { hasText: 'Direct' }) }).click();
    await page.getByRole('button', { name: 'Next' }).click();

    await page.locator('.tone-card:has-text("Formal")').click();
    await page.getByRole('button', { name: 'Next' }).click();

    await page.locator('.appearance-mode-card:has-text("Dark")').click();
    await page.getByRole('button', { name: 'Next' }).click();

    const languageChip = page.locator('.setup-language-chip:has-text("Spanish")');
    await languageChip.click();
    const autoLearnToggle = page.getByLabel('Auto-learn corrections');
    await autoLearnToggle.click();
    const muteToggle = page.getByLabel('Mute while recording');
    await muteToggle.click();
    await page.getByRole('button', { name: 'Next' }).click();

    const calibrationBox = page.locator('.calibration-box');
    let hasCalibrationStep = false;
    try {
      await calibrationBox.waitFor({ state: 'visible', timeout: 1000 });
      hasCalibrationStep = true;
    } catch (e) {
      // Calibration box did not appear
    }
    if (hasCalibrationStep) {
      await page.locator('.step-footer .btn-skip').click();
    }

    await page.locator('.done-summary').waitFor({ state: 'visible', timeout: TIMEOUT });

    const summaryText = (await page.locator('.done-summary').textContent()) || '';
    if (!summaryText.includes('OpenAI')) errors.push('Done summary did not keep provider choice');
    if (!summaryText.includes('Direct')) errors.push('Done summary did not keep cleanup choice');
    if (!summaryText.includes('Formal')) errors.push('Done summary did not keep tone choice');
    if (!summaryText.includes('Spanish')) errors.push('Done summary did not keep language choice');
    if (!summaryText.includes('Dark')) errors.push('Done summary did not keep appearance choice');

    await page.getByRole('button', { name: 'Start dictating' }).click();
    await page.locator('h1.page-h:has-text("Welcome back")').waitFor({ state: 'visible', timeout: TIMEOUT });

    const persisted = await page.evaluate(() => JSON.parse(localStorage.getItem('open-flow:dev-settings') || '{}'));
    if (persisted.transcription_provider !== 'openai') errors.push('Provider did not persist to dev settings');
    if (persisted.cleanup_intensity !== 'high') errors.push('Cleanup intensity did not persist as Direct/high');
    if (persisted.default_tone !== 'formal') errors.push('Tone did not persist');
    if (persisted.transcription_language !== 'es') errors.push('Language did not persist');
    if (persisted.appearance_mode !== 'dark') errors.push('Appearance did not persist');
    if (persisted.auto_learn_enabled !== true) errors.push('Quick setting auto-learn did not persist');
    if (persisted.mute_audio !== true) errors.push('Quick setting mute audio did not persist');
    if (persisted.setup_complete !== true) errors.push('Setup completion did not persist');

    if (errors.length > 0) {
      console.error('FAIL');
      for (const error of errors) console.error(`  ${error}`);
      process.exit(1);
    }

    console.log('PASS - onboarding flow persisted provider, cleanup, tone, language, appearance, and quick settings.');
  } catch (err) {
    console.error(`FAIL - onboarding flow threw: ${err.message}`);
    process.exit(1);
  } finally {
    await browser.close();
  }
})();
