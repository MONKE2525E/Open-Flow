'use strict';

const { chromium } = require('playwright');
const { TARGET_URL, TIMEOUT, seedDevState } = require('./_dev-helpers.cjs');

const MAC_USER_AGENT =
  'Mozilla/5.0 (Macintosh; Intel Mac OS X 14_6) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36';

async function reachPermissionsStep(page) {
  await page.getByRole('button', { name: 'Get Started' }).click();
  await page.getByRole('button', { name: 'Next' }).click();
  await page.getByRole('button', { name: 'Continue' }).click();
  await page.getByRole('heading', { name: 'Check your macOS permissions' }).waitFor({ state: 'visible', timeout: TIMEOUT });
}

(async () => {
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage({ userAgent: MAC_USER_AGENT });
  const errors = [];

  page.on('pageerror', (err) => errors.push(`Page exception: ${err.message}`));
  page.on('console', (msg) => {
    if (msg.type() === 'error') errors.push(`Console error: ${msg.text()}`);
  });

  try {
    await seedDevState(page, {
      settings: {
        force_setup_on_launch: true,
        setup_complete: false,
        appearance_mode: 'system',
        transcription_language: 'en',
        accessibility_permission_status: 'denied',
        input_monitoring_permission_status: 'authorized',
        microphone_permission_status: 'authorized',
      },
    });

    await page.goto(TARGET_URL, { waitUntil: 'networkidle', timeout: TIMEOUT });
    await reachPermissionsStep(page);

    const primary = page.locator('.setup-actionbar .btn-primary');
    if (await primary.textContent() !== 'Grant permissions to continue') {
      errors.push('Permission step did not show the hard-gate button label.');
    }
    if (!(await primary.isDisabled())) {
      errors.push('Permission step Next button was enabled before all core permissions were granted.');
    }

    await page.evaluate(() => {
      const saved = JSON.parse(localStorage.getItem('verenu:dev-settings') || '{}');
      saved.accessibility_permission_status = 'authorized';
      saved.input_monitoring_permission_status = 'authorized';
      saved.microphone_permission_status = 'authorized';
      saved.global_input_seen = true;
      localStorage.setItem('verenu:dev-settings', JSON.stringify(saved));
    });
    // The visible permission surface must reconcile with macOS on its own;
    // returning from System Settings must not require a manual refresh click.
    await page.locator('.setup-actionbar .btn-primary:has-text("Next")').waitFor({ state: 'visible', timeout: TIMEOUT });
    if (await primary.isDisabled()) {
      errors.push('Permission step Next button stayed disabled after all core permissions were granted.');
    }

    const settingsPage = await browser.newPage({ userAgent: MAC_USER_AGENT });
    await seedDevState(settingsPage, {
      settings: {
        setup_complete: true,
        force_setup_on_launch: false,
        appearance_mode: 'system',
        accessibility_permission_status: 'authorized',
        input_monitoring_permission_status: 'authorized',
        microphone_permission_status: 'authorized',
      },
    });
    await settingsPage.goto(TARGET_URL, { waitUntil: 'networkidle', timeout: TIMEOUT });
    await settingsPage.evaluate(() => {
      window.dispatchEvent(new CustomEvent('tauri:open-flow:open-settings-section', { detail: 'permissions' }));
    });
    await settingsPage.locator('.settings-page').waitFor({ state: 'visible', timeout: TIMEOUT });
    await settingsPage.getByRole('heading', { name: 'Permissions' }).waitFor({ state: 'visible', timeout: TIMEOUT });
    await settingsPage.getByText('Available', { exact: true }).waitFor({ state: 'visible', timeout: TIMEOUT });
    const refresh = settingsPage.getByRole('button', { name: 'Refresh', exact: true });
    await refresh.click();
    if ((await refresh.textContent())?.replace(/\s/g, '') !== '↻Refresh') {
      errors.push('Refresh changed its visible label while refreshing.');
    }
    if (await settingsPage.getByText('Checking…', { exact: true }).count()) {
      errors.push('Permissions surface exposed a transient Checking state.');
    }
    await settingsPage.getByText('Available', { exact: true }).waitFor({ state: 'visible', timeout: TIMEOUT });
    await settingsPage.close();

    if (errors.length > 0) {
      console.error('FAIL');
      for (const error of errors) console.error(`  ${error}`);
      process.exit(1);
    }

    console.log('PASS - macOS permissions hard gate and settings-section event work in dev mode.');
  } catch (err) {
    console.error(`FAIL - macOS permissions smoke threw: ${err.message}`);
    process.exit(1);
  } finally {
    await browser.close();
  }
})();
