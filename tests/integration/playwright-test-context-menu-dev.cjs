'use strict';

const { chromium } = require('playwright');
const { tauriMock, APP_VERSION } = require('../smoke/_tauri-mock.cjs');

const TARGET_URL = process.env.TEST_URL || 'http://localhost:1420';
const TIMEOUT = 10_000;

(async () => {
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage();
  const errors = [];

  await page.addInitScript(tauriMock, { appVersion: APP_VERSION });
  page.on('pageerror', (error) => errors.push(`Page exception: ${error.message}`));
  page.on('console', (message) => {
    if (message.type() === 'error') errors.push(`Console error: ${message.text()}`);
  });

  try {
    await page.goto(TARGET_URL, { waitUntil: 'domcontentloaded', timeout: TIMEOUT });

    const row = page.locator('.ctx-row-wrap').filter({ hasText: 'Work' }).first();
    await row.waitFor({ state: 'visible', timeout: TIMEOUT });
    await row.hover();

    const kebab = row.getByRole('button', { name: 'More actions for Work' });
    await kebab.click();

    const menu = page.locator('.ctx-menu');
    await menu.waitFor({ state: 'visible', timeout: 2_000 });
    await menu.getByRole('menuitem', { name: 'Edit' }).click();

    const modal = page.locator('.context-modal[role="dialog"]');
    await modal.waitFor({ state: 'visible', timeout: 3_000 });
    if (await modal.locator('#context-name').inputValue() !== 'Work') {
      errors.push('Edit opened the wrong context group');
    }

    await modal.getByRole('button', { name: 'Cancel' }).click();
    await modal.waitFor({ state: 'hidden', timeout: 3_000 });

    const home = page.getByRole('button', { name: 'Home' });
    await home.click();
    await page.locator('h1.page-h:has-text("Welcome back")').waitFor({ state: 'visible', timeout: 3_000 });

    if (errors.length > 0) {
      console.error('\nFAIL:');
      errors.forEach((error) => console.error(`  ${error}`));
      process.exitCode = 1;
    }
    console.log('PASS - pinned context menu opens, edits, closes, and releases the UI.');
  } catch (error) {
    console.error(`FAIL - test threw: ${error.message}`);
    process.exitCode = 1;
  } finally {
    await browser.close();
  }
})();
