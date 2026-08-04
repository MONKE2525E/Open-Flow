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
      appearance_mode: 'dark',
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

    // API key step opens on the have-a-key fork rather than dropping straight
    // into a password field. Take the tutorial branch, page through it, then
    // land on the paste field without entering a key.
    await page.locator('.fork-card:has-text("walk me through")').click();
    await page.locator('.shot-frame').waitFor({ state: 'visible', timeout: TIMEOUT });
    const firstCaption = (await page.locator('.shot-text').textContent()) || '';
    await page.locator('.shot-nav').last().click();
    const secondCaption = (await page.locator('.shot-text').textContent()) || '';
    if (firstCaption === secondCaption) errors.push('Tutorial carousel did not advance between steps');
    await page.locator('.btn-got-key').click();
    await page.locator('.key-input').waitFor({ state: 'visible', timeout: TIMEOUT });
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

    // Models step reuses the Settings preset picker. No key was saved above, so
    // only the local presets are offered — and every one needs a download, which
    // must NOT be pre-selected on the user's behalf.
    await page.locator('.preset-grid').waitFor({ state: 'visible', timeout: TIMEOUT });
    if (await page.locator('.preset-card.preset-active').count()) {
      errors.push('A preset requiring a download was pre-selected during setup');
    }
    await page.getByRole('button', { name: 'Next' }).click();

    // Writing Style merges cleanup intensity and tone onto one screen.
    // Selecting "Off" must also disable tone, since tone is a cleanup instruction.
    const offCard = page.locator('button.pick-card', { has: page.locator('.card-name', { hasText: 'Off' }) });
    await offCard.click();
    const toneGridInert = await page.locator('.tone-grid').evaluate((el) => el.hasAttribute('inert'));
    if (!toneGridInert) errors.push('Tone group was not disabled when cleanup intensity was Off');

    await page.locator('button.pick-card', { has: page.locator('.card-name', { hasText: 'Strong' }) }).click();
    const toneReenabled = await page.locator('.tone-grid').evaluate((el) => !el.hasAttribute('inert'));
    if (!toneReenabled) errors.push('Tone group stayed disabled after leaving Off');
    await page.locator('button.pick-card', { has: page.locator('.card-name', { hasText: 'Formal' }) }).click();
    await page.getByRole('button', { name: 'Next' }).click();

    // Spoken language is its own step now — a searchable list, not chips.
    await page.locator('.lang-search-input').fill('span');
    await page.locator('.lang-row:has-text("Spanish")').click();
    await page.getByRole('button', { name: 'Next' }).click();

    // Audio environment: "Speakers" is what turns on mute + media pause.
    await page.locator('.env-card:has-text("Speakers")').click();
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
      await page.locator('.btn-skip').click();
    }

    // Try It step: skip the hands-on hotkey test in automated runs.
    const tryItField = page.locator('.tryit-field');
    let hasTryItStep = false;
    try {
      await tryItField.waitFor({ state: 'visible', timeout: 1000 });
      hasTryItStep = true;
    } catch (e) {
      // Try It field did not appear
    }
    if (hasTryItStep) {
      await page.locator('.btn-skip').click();
    }

    await page.locator('.done-summary').waitFor({ state: 'visible', timeout: TIMEOUT });

    const summaryText = (await page.locator('.done-summary').textContent()) || '';
    if (!summaryText.includes('OpenAI')) errors.push('Done summary did not keep provider choice');
    if (!summaryText.includes('Strong')) errors.push('Done summary did not keep cleanup choice');
    if (!summaryText.includes('Formal')) errors.push('Done summary did not keep tone choice');
    if (!summaryText.includes('Spanish')) errors.push('Done summary did not keep language choice');
    if (!summaryText.includes('Speakers')) errors.push('Done summary did not keep audio environment choice');

    await page.getByRole('button', { name: 'Start dictating' }).click();
    await page.locator('h1.page-h:has-text("Welcome back")').waitFor({ state: 'visible', timeout: TIMEOUT });

    const persisted = await page.evaluate(() => JSON.parse(localStorage.getItem('verenu:dev-settings') || '{}'));
    if (persisted.transcription_provider !== 'openai') errors.push('Provider did not persist to dev settings');
    if (persisted.cleanup_intensity !== 'high') errors.push('Cleanup intensity did not persist as Strong/high');
    if (persisted.cleanup_enabled !== true) errors.push('cleanup_enabled should follow a non-Off intensity');
    if (persisted.default_tone !== 'formal') errors.push('Tone did not persist');
    if (persisted.transcription_language !== 'es') errors.push('Language did not persist');
    // Appearance is no longer asked; the wizard pins it to system.
    if (persisted.appearance_mode !== 'system') errors.push('Appearance should default to system');
    if (persisted.mute_audio !== true) errors.push('Speakers answer did not enable mute_audio');
    if (persisted.pause_media_during_dictation !== true) errors.push('Speakers answer did not enable pause_media_during_dictation');

    // Smart processing is no longer a page of toggles — it is all on by default.
    for (const key of [
      'noise_reduction',
      'contextual_caps_enabled',
      'auto_spacing_enabled',
      'caps_lock_uppercase_enabled',
      'app_context_hint',
      'auto_learn_enabled',
    ]) {
      if (persisted[key] !== true) errors.push(`Smart-processing default ${key} did not persist as true`);
    }

    if (persisted.setup_complete !== true) errors.push('Setup completion did not persist');

    if (errors.length > 0) {
      console.error('FAIL');
      for (const error of errors) console.error(`  ${error}`);
      process.exit(1);
    }

    console.log('PASS - onboarding flow persisted provider, cleanup, tone, language, audio environment, and smart-processing defaults.');
  } catch (err) {
    console.error(`FAIL - onboarding flow threw: ${err.message}`);
    process.exit(1);
  } finally {
    await browser.close();
  }
})();
