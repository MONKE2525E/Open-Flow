'use strict';

const TARGET_URL = process.env.TEST_URL || 'http://localhost:1420';
const TIMEOUT = 12_000;

async function seedDevState(page, {
  settings = {},
  snippets = [],
  dictionary = [],
  localSttModels = {},
  localLlmModels = {},
  localSttState = null,
  localLlmState = null,
} = {}) {
  await page.addInitScript(({ settings, snippets, dictionary, localSttModels, localLlmModels, localSttState, localLlmState }) => {
    localStorage.setItem('verenu:dev-settings', JSON.stringify(settings));
    localStorage.setItem('verenu:dev-snippets', JSON.stringify(snippets));
    localStorage.setItem('verenu:dev-dictionary', JSON.stringify(dictionary));
    localStorage.setItem('verenu:dev-local-stt-models', JSON.stringify(localSttModels));
    localStorage.setItem('verenu:dev-local-llm-models', JSON.stringify(localLlmModels));
    if (localSttState) localStorage.setItem('verenu:dev-local-stt-state', JSON.stringify(localSttState));
    if (localLlmState) localStorage.setItem('verenu:dev-local-llm-state', JSON.stringify(localLlmState));
  }, { settings, snippets, dictionary, localSttModels, localLlmModels, localSttState, localLlmState });
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
