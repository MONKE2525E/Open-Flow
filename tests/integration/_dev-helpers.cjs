'use strict';

const TARGET_URL = process.env.TEST_URL || 'http://localhost:1420';
const TIMEOUT = 12_000;

async function seedDevState(page, {
  settings = {},
  snippets = [],
  dictionary = [],
} = {}) {
  await page.addInitScript(({ settings, snippets, dictionary }) => {
    localStorage.setItem('open-flow:dev-settings', JSON.stringify(settings));
    localStorage.setItem('open-flow:dev-snippets', JSON.stringify(snippets));
    localStorage.setItem('open-flow:dev-dictionary', JSON.stringify(dictionary));
  }, { settings, snippets, dictionary });
}

async function openSettings(page) {
  const button = page.locator('.nav-item:has-text("Settings")');
  await button.waitFor({ state: 'visible', timeout: TIMEOUT });
  await button.click();
  await page.locator('.settings-modal').waitFor({ state: 'visible', timeout: TIMEOUT });
}

async function closeSettings(page) {
  await page.mouse.click(10, 10);
  await page.locator('.settings-modal').waitFor({ state: 'hidden', timeout: TIMEOUT });
}

module.exports = {
  TARGET_URL,
  TIMEOUT,
  seedDevState,
  openSettings,
  closeSettings,
};
