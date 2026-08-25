'use strict';

/*
 * Full-screen settings coverage.
 *
 * The frozen smoke tests in tests/smoke/ still describe settings in its old
 * modal terms — they assert .settings-page goes visible/hidden, that the
 * backdrop fades, and that a click in the window gutter dismisses it. All of
 * that still holds and must keep holding, so those files are left alone.
 *
 * What they cannot describe is what actually changed: the rail now lives in the
 * sidebar and morphs, settings renders as a page rather than a card, the
 * outgoing view animates out, and the version line is a centred footer shown
 * only on About. This file covers that, and guards the two regressions that
 * were easy to reintroduce (duplicate contract elements on a fast reopen, and
 * settings rows spanning the full window with no measure).
 *
 * Assertions are structural and directional on purpose — durations, easings and
 * exact offsets are design tuning and are deliberately not pinned here.
 */

const { chromium } = require('playwright');
const { TARGET_URL, TIMEOUT, seedDevState } = require('./_dev-helpers.cjs');

const APP_NAV_LABELS = ['Home', 'Dictionary', 'Snippets', 'Style'];
const SECTION_LABELS = ['General', 'App Mappings', 'API Keys', 'Models', 'Privacy', 'Audio', 'About'];

const errors = [];
const check = (ok, message) => { if (!ok) errors.push(message); };

// Only report a step as ok when nothing failed since the previous step, so the
// running log can't claim success for a group that just recorded a failure.
let notedErrors = 0;
const note = (message) => {
  if (errors.length === notedErrors) console.log(`  ok  ${message}`);
  notedErrors = errors.length;
};

// Clicks carry an explicit timeout so a missing rail fails in seconds rather
// than sitting on Playwright's 30s default.
async function openSettings(page) {
  await page.locator('.nav-item:has-text("Settings")').click({ timeout: TIMEOUT });
  await page.locator('.settings-page').waitFor({ state: 'visible', timeout: TIMEOUT });
  await page.waitForTimeout(600); // let the morph settle
}

async function gotoSection(page, label) {
  await page.locator(`.settings-nav-item:has-text("${label}")`).click({ timeout: TIMEOUT });
  await page.locator(`h2.settings-h:has-text("${label}")`).waitFor({ state: 'visible', timeout: TIMEOUT });
  await page.waitForTimeout(450);
}

(async () => {
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });

  page.on('pageerror', (err) => errors.push(`Uncaught page error: ${err.message}`));
  page.on('console', (msg) => {
    if (msg.type() === 'error') errors.push(`console.error: ${msg.text()}`);
  });

  await seedDevState(page, {
    settings: { setup_complete: true, force_setup_on_launch: false, legacy_features_enabled: true },
  });

  try {
    await page.goto(TARGET_URL, { waitUntil: 'domcontentloaded', timeout: TIMEOUT });
    await page.locator('.nav-item').first().waitFor({ state: 'visible', timeout: TIMEOUT });

    // ── 1. The rail swaps its contents in place ───────────────────────────
    check(
      (await page.locator('.sidebar .settings-nav-item').count()) === 0,
      'Settings sections should not be in the rail before settings is opened',
    );

    await openSettings(page);

    const railSections = await page.locator('.sidebar .settings-nav-item').count();
    check(railSections >= SECTION_LABELS.length,
      `Expected the settings sections to render inside .sidebar, found ${railSections}`);
    check(
      (await page.locator('.settings-page .settings-nav-item').count()) === 0,
      'The section rail should live in the sidebar, not inside the settings shell',
    );
    for (const label of APP_NAV_LABELS) {
      check(
        (await page.locator(`.sidebar .nav-item:has-text("${label}")`).count()) === 0,
        `App nav entry "${label}" should be replaced while settings is open`,
      );
    }
    check(
      (await page.locator('.sidebar .settings-back:has-text("Back to app")').count()) === 1,
      'Expected a "Back to app" control in the rail while settings is open',
    );
    note(`rail morphs in place (${railSections} sections, app nav replaced)`);

    // ── 2. The rail stays above the wash and stays interactive ────────────
    const layering = await page.evaluate(() => {
      const z = (sel) => {
        const el = document.querySelector(sel);
        return el ? parseInt(getComputedStyle(el).zIndex, 10) : NaN;
      };
      return { sidebar: z('.sidebar'), wash: z('.settings-overlay-wrap') };
    });
    check(
      Number.isFinite(layering.sidebar) && layering.sidebar > layering.wash,
      `Sidebar must stack above the settings wash to stay usable ` +
      `(sidebar z-index ${layering.sidebar}, wash ${layering.wash})`,
    );
    note(`sidebar stacks above the wash (${layering.sidebar} > ${layering.wash})`);

    // ── 3. Settings presents as a page, not a floating card ───────────────
    const surface = await page.evaluate(() => {
      const s = getComputedStyle(document.querySelector('.settings-page'));
      return { bg: s.backgroundColor, borderWidth: s.borderTopWidth };
    });
    const transparentBg = surface.bg === 'rgba(0, 0, 0, 0)' || surface.bg === 'transparent';
    check(transparentBg, `Settings should not paint its own card background (got ${surface.bg})`);
    check(parseFloat(surface.borderWidth) === 0,
      `Settings should not draw a card border (got ${surface.borderWidth})`);
    note('settings renders as a page surface, not a card');

    // ── 4. The content column keeps a readable measure ────────────────────
    const measure = await page.evaluate(() => {
      const inner = document.querySelector('.panel-inner');
      const panel = document.querySelector('.panel');
      if (!inner || !panel) return null;
      const a = inner.getBoundingClientRect();
      const b = panel.getBoundingClientRect();
      return {
        width: Math.round(a.width),
        offCentre: Math.round(Math.abs((a.left + a.width / 2) - (b.left + b.width / 2))),
      };
    });
    check(measure !== null, 'Expected a .panel-inner measure wrapper inside the settings panel');
    if (measure) {
      check(measure.width > 0 && measure.width <= 900,
        `Settings content should stay within a readable measure, got ${measure.width}px`);
      check(measure.offCentre <= 14,
        `Settings content column should be centred in the panel (off by ${measure.offCentre}px)`);
      note(`content column ${measure.width}px, centred`);
    }

    // ── 5. The outgoing view actually animates out ────────────────────────
    const outgoing = await page.evaluate(() => {
      const el = document.querySelector('.content');
      const s = getComputedStyle(el);
      return { opacity: parseFloat(s.opacity), y: Math.round(new DOMMatrixReadOnly(s.transform).m42) };
    });
    check(outgoing.opacity === 0,
      `The app view should fade out behind settings (opacity ${outgoing.opacity})`);
    check(Math.abs(outgoing.y) > 0,
      'The app view should translate on the vertical axis when settings opens, not just disappear');
    note(`app view exits (opacity 0, translateY ${outgoing.y}px)`);

    // ── 6. The travelling highlight ───────────────────────────────────────
    check((await page.locator('.rail-pill').count()) === 1,
      'Expected exactly one .rail-pill highlight in the rail');

    const pillTravel = await page.evaluate(async () => {
      const pill = document.querySelector('.rail-pill');
      const target = [...document.querySelectorAll('.settings-nav-item')]
        .find((b) => b.textContent.includes('Privacy'));
      // Report rather than throw, so a broken rail fails with a readable message.
      if (!pill || !target) return null;
      const top = () => parseFloat(getComputedStyle(pill).top);
      const before = top();
      target.click();
      const seen = new Set();
      const t0 = performance.now();
      await new Promise((res) => {
        const f = () => {
          seen.add(Math.round(top()));
          if (performance.now() - t0 < 400) requestAnimationFrame(f); else res();
        };
        requestAnimationFrame(f);
      });
      const active = document.querySelector('.settings-nav-item.active');
      return {
        before,
        distinct: seen.size,
        end: Math.round(top()),
        activeTop: active ? Math.round(active.offsetTop) : null,
      };
    });
    check(pillTravel !== null,
      'Expected a .rail-pill and section entries in the rail to measure the highlight against');
    if (pillTravel) {
      check(pillTravel.distinct > 2,
        `The highlight should slide between sections rather than jump (saw ${pillTravel.distinct} positions)`);
      check(pillTravel.activeTop !== null && Math.abs(pillTravel.end - pillTravel.activeTop) <= 2,
        `The highlight should settle on the active section (pill ${pillTravel.end}, item ${pillTravel.activeTop})`);
      note(`highlight slides ${pillTravel.before} -> ${pillTravel.end} across ${pillTravel.distinct} positions`);
    }

    // ── 7. Version footer belongs to About and nowhere else ───────────────
    await gotoSection(page, 'General');
    check((await page.locator('.settings-foot').count()) === 0,
      'Version footer should not render outside the About section');

    await gotoSection(page, 'About');
    const footText = await page.locator('.settings-foot').textContent();
    check(/v\d+\.\d+\.\d+/.test(footText || ''),
      `Version footer should carry the app version (got "${footText}")`);
    const footOnAbout = await page.evaluate(() => {
      const f = document.querySelector('.settings-foot');
      const shell = document.querySelector('.settings-page').getBoundingClientRect();
      const range = document.createRange();
      range.selectNodeContents(f);
      const t = range.getBoundingClientRect();
      return {
        offCentre: Math.round(Math.abs((t.left + t.width / 2) - (shell.left + shell.width / 2))),
        gapFromBottom: Math.round(shell.bottom - t.bottom),
      };
    });
    check(footOnAbout.offCentre <= 14,
      `Version footer should be centred on the settings page (off by ${footOnAbout.offCentre}px)`);
    check(footOnAbout.gapFromBottom >= 0 && footOnAbout.gapFromBottom <= 60,
      `Version footer should sit at the bottom of the page (gap ${footOnAbout.gapFromBottom}px)`);
    note(`version footer centred on About only (off-centre ${footOnAbout.offCentre}px)`);

    // ── 8. No duplicate contract elements on a fast close/reopen ──────────
    // Two {#if} branches used to leave outgoing rail entries mounted alongside
    // incoming ones, which resolves as a Playwright strict-mode violation.
    const peaks = await page.evaluate(async () => {
      const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
      const byText = (sel, text) =>
        [...document.querySelectorAll(sel)].find((e) => e.textContent.includes(text));
      const worst = { foot: 0, modal: 0, overlay: 0, pill: 0, models: 0 };
      for (const gap of [0, 40, 90, 150, 220]) {
        document.querySelector('.settings-overlay')?.click();
        await sleep(gap);
        byText('.nav-item', 'Settings')?.click();
        const t0 = performance.now();
        while (performance.now() - t0 < 500) {
          worst.foot = Math.max(worst.foot, document.querySelectorAll('.settings-foot').length);
          worst.modal = Math.max(worst.modal, document.querySelectorAll('.settings-page').length);
          worst.overlay = Math.max(worst.overlay, document.querySelectorAll('.settings-overlay').length);
          worst.pill = Math.max(worst.pill, document.querySelectorAll('.rail-pill').length);
          worst.models = Math.max(worst.models, [...document.querySelectorAll('.settings-nav-item')]
            .filter((e) => e.textContent.includes('Models')).length);
          await new Promise((r) => requestAnimationFrame(r));
        }
        await sleep(400);
      }
      return worst;
    });
    for (const [name, count] of Object.entries(peaks)) {
      check(count <= 1, `Fast close/reopen left ${count} "${name}" elements in the DOM; must stay at 1`);
    }
    note('no duplicate contract elements across fast close/reopen');

    // ── 9. Back to app restores the rail ──────────────────────────────────
    await page.locator('.settings-back').click();
    await page.locator('.settings-page').waitFor({ state: 'hidden', timeout: TIMEOUT });
    await page.waitForTimeout(500);
    for (const label of APP_NAV_LABELS) {
      check(
        (await page.locator(`.sidebar .nav-item:has-text("${label}")`).count()) === 1,
        `App nav entry "${label}" should return to the rail after leaving settings`,
      );
    }
    check((await page.locator('.sidebar .settings-nav-item').count()) === 0,
      'Settings sections should be gone from the rail after leaving settings');
    const restored = await page.evaluate(
      () => parseFloat(getComputedStyle(document.querySelector('.content')).opacity));
    check(restored === 1, `The app view should be restored after leaving settings (opacity ${restored})`);
    note('"Back to app" restores the app rail and view');

    if (errors.length) {
      console.error('FAIL - full-screen settings:');
      for (const e of errors) console.error(`  ✗ ${e}`);
      process.exitCode = 1;
    }
    console.log('PASS - full-screen settings behaves as expected.');
  } catch (err) {
    console.error(`FAIL - full-screen settings test threw: ${err.message}`);
    process.exitCode = 1;
  } finally {
    await browser.close();
  }
})();
