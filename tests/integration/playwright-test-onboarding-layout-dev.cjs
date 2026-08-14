'use strict';

const fs = require('fs');
const path = require('path');
const { chromium } = require('playwright');
const { TARGET_URL, TIMEOUT, seedDevState } = require('./_dev-helpers.cjs');

const VIEWPORTS = [
  { width: 1100, height: 720, label: '1100x720' },
  { width: 900, height: 600, label: '900x600' },
];

const EXPECTED_ASSETS = [
  'groq-1-signin.png', 'groq-2-keys.png', 'groq-3-create.png', 'groq-4-copy.png',
  'google-1-signin.png', 'google-2-apikey.png', 'google-3-create.png', 'google-4-copy.png',
  'openai-1-signin.png', 'openai-2-keys.png', 'openai-3-create.png', 'openai-4-copy.png',
];

async function seed(page) {
  await seedDevState(page, {
    settings: { force_setup_on_launch: true, setup_complete: false, appearance_mode: 'system' },
  });
}

async function layoutState(page) {
  return page.evaluate(() => {
    const overlay = document.querySelector('.setup-overlay');
    const content = document.querySelector('.setup-content');
    const actionbar = document.querySelector('.setup-actionbar');
    const visibleScrollers = [...(overlay?.querySelectorAll('*') ?? [])]
      .filter((el) => {
        if (el.hasAttribute('data-scroll-region')) return false;
        const style = getComputedStyle(el);
        return el.getClientRects().length > 0 &&
          ['auto', 'scroll'].includes(style.overflowY) && el.scrollHeight > el.clientHeight;
      })
      .map((el) => el.className);
    return {
      pageOverflow: document.documentElement.scrollWidth > document.documentElement.clientWidth ||
        document.documentElement.scrollHeight > document.documentElement.clientHeight,
      contentOverflow: !!content && content.scrollHeight > content.clientHeight,
      collision: !!content && !!actionbar && content.getBoundingClientRect().bottom > actionbar.getBoundingClientRect().top + 1,
      actionbarTop: actionbar?.getBoundingClientRect().top ?? null,
      visibleScrollers,
    };
  });
}

(async () => {
  const errors = [];
  const assetDir = path.join(__dirname, '../../src/assets/setup');
  for (const name of EXPECTED_ASSETS) {
    const file = path.join(assetDir, name);
    if (!fs.existsSync(file)) errors.push(`Missing tutorial asset: ${name}`);
    else if (fs.statSync(file).size > 205 * 1024) errors.push(`Tutorial asset exceeds 205KB: ${name}`);
  }

  const browser = await chromium.launch({ headless: true });
  try {
    for (const viewport of VIEWPORTS) {
      const page = await browser.newPage({ viewport });
      await seed(page);
      await page.goto(TARGET_URL, { waitUntil: 'networkidle', timeout: TIMEOUT });
      const actionbarTops = [];

      for (let index = 0; index < 11; index += 1) {
        await page.waitForTimeout(340);
        const state = await layoutState(page);
        const heading = (await page.locator('.setup-header h2, .brand-name, .done-title').first().textContent())?.trim() || `step ${index}`;
        if (state.pageOverflow) errors.push(`[${viewport.label}] ${heading}: page overflow`);
        if (state.contentOverflow) errors.push(`[${viewport.label}] ${heading}: setup content overflow`);
        if (state.collision) errors.push(`[${viewport.label}] ${heading}: content collides with action bar`);
        if (state.visibleScrollers.length) errors.push(`[${viewport.label}] ${heading}: unexpected scroller ${state.visibleScrollers.join(', ')}`);
        if (state.actionbarTop != null) actionbarTops.push(state.actionbarTop);

        if (await page.locator('.fork-card').count()) {
          await page.locator('.fork-card:has-text("walk me through")').click();
          const image = page.locator('.shot-img');
          await image.waitFor({ state: 'visible', timeout: TIMEOUT });
          await image.evaluate((img) => img.complete && img.naturalWidth > 0
            ? true
            : new Promise((resolve) => img.addEventListener('load', () => resolve(true), { once: true })));
          const size = await image.evaluate((img) => ({ width: img.naturalWidth, height: img.naturalHeight, alt: img.alt }));
          if (size.width !== 1200 || size.height !== 675 || !size.alt.trim()) errors.push(`[${viewport.label}] tutorial image metadata is invalid`);
          const before = await page.locator('.shot-text').textContent();
          await page.getByRole('button', { name: 'Next step' }).press('ArrowRight');
          const after = await page.locator('.shot-text').textContent();
          if (before === after) errors.push(`[${viewport.label}] ArrowRight did not advance tutorial`);
          await page.locator('.btn-got-key').click();
        }

        const final = page.getByRole('button', { name: 'Start dictating' });
        if (await final.count()) break;
        const next = page.locator('.setup-actionbar .btn-primary').first();
        if (!(await next.count())) break;
        await next.hover();
        const colors = await next.evaluate((button) => {
          const style = getComputedStyle(button);
          return { background: style.backgroundColor, accent: getComputedStyle(document.documentElement).getPropertyValue('--accent').trim() };
        });
        if (colors.background.includes('rgb(13, 10, 8)')) errors.push(`[${viewport.label}] onboarding primary inherited the dark global hover`);
        await next.click();
      }

      if (actionbarTops.some((top) => Math.abs(top - actionbarTops[0]) > 1)) {
        errors.push(`[${viewport.label}] action bar moved between steps`);
      }
      await page.close();
    }

    const local = await browser.newPage({ viewport: VIEWPORTS[1] });
    await seed(local);
    await local.addInitScript(() => {
      localStorage.removeItem('verenu:dev-local-stt-models');
      localStorage.removeItem('verenu:dev-local-transcription-state');
    });
    await local.goto(TARGET_URL, { waitUntil: 'networkidle', timeout: TIMEOUT });
    await local.getByRole('button', { name: 'Get Started' }).click();
    await local.locator('.provider-card:has-text("On this device")').click();
    await local.getByRole('button', { name: 'Next' }).click();
    const localDownload = local.getByRole('button', { name: 'Download model' });
    await localDownload.waitFor({ state: 'visible', timeout: TIMEOUT });
    const localInitial = await layoutState(local);
    if (localInitial.pageOverflow || localInitial.contentOverflow || localInitial.collision) {
      errors.push('[900x600] local model setup does not fit before download');
    }
    await localDownload.click();
    await local.locator('.local-progress').waitFor({ state: 'visible', timeout: TIMEOUT });
    const localDownloading = await layoutState(local);
    if (localDownloading.pageOverflow || localDownloading.contentOverflow || localDownloading.collision) {
      errors.push('[900x600] local model progress does not fit');
    }
    await local.getByText('Ready', { exact: true }).waitFor({ state: 'visible', timeout: 10000 });
    await local.getByRole('button', { name: 'Continue' }).click();
    await local.locator('.preset-grid').waitFor({ state: 'visible', timeout: TIMEOUT });
    if (!(await local.locator('.preset-action-btn').count())) errors.push('Local model presets lost their inline actions');
    const chooseSetup = local.getByRole('button', { name: 'Choose a setup' });
    if (!(await chooseSetup.isDisabled())) errors.push('Local setup should require an explicit preset choice');
    await local.locator('.preset-select').first().click();
    await local.getByRole('button', { name: 'Next' }).waitFor({ state: 'visible', timeout: TIMEOUT });
    if (await local.getByRole('button', { name: 'Next' }).isDisabled()) errors.push('Choosing a local preset did not unlock the flow');
    if (!(await local.locator('.preset-action-btn:has-text("Downloading")').count())) errors.push('Choosing a local preset did not start its missing download');
    await local.close();

    const reduced = await browser.newPage({ reducedMotion: 'reduce', viewport: VIEWPORTS[0] });
    await seed(reduced);
    await reduced.goto(TARGET_URL, { waitUntil: 'networkidle', timeout: TIMEOUT });
    const transition = await reduced.locator('.intro-brand').evaluate((el) => getComputedStyle(el).transitionDuration);
    if (transition !== '0s') errors.push('Reduced motion did not remove intro transitions');
    await reduced.close();
  } finally {
    await browser.close();
  }

  if (errors.length) {
    console.error('FAIL - onboarding layout/style');
    errors.forEach((error) => console.error(`  ${error}`));
    process.exit(1);
  }
  console.log('PASS - onboarding layout, assets, keyboard tutorial, control states, and reduced motion');
})().catch((error) => {
  console.error(`FAIL - onboarding layout/style threw: ${error.message}`);
  process.exit(1);
});
