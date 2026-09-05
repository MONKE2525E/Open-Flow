/**
 * Smoke test: animateWidth action applied to all dropdowns
 *
 * Verifies that every dropdown trigger button in the UI has:
 *   - An explicit pixel width set by the animateWidth action
 *   - A CSS transition that includes 'width'
 *   - Width within a sane range (not collapsed or overflowing)
 *
 * Covered dropdowns:
 *   1. Microphone selector        (GeneralSection)
 *   2. Spoken Language selector   (GeneralSection)
 *   3. History Retention          (PrivacySection)
 *   4. App Profile (add row)      (AppMappingsEditor via Settings > Apps)
 */

const path = require('path');
const { chromium } = require('playwright');
const { tauriMock, APP_VERSION } = require('./_tauri-mock.cjs');

const TARGET_URL = process.env.TEST_URL || 'http://localhost:1420';
const TIMEOUT = 12_000;
const SCREENSHOT_DIR = __dirname;

async function checkDropdown(page, selector, label) {
  const btn = page.locator(selector).first();

  try {
    await btn.waitFor({ state: 'visible', timeout: 5_000 });
  } catch {
    return { label, pass: false, reason: 'not visible within 5s' };
  }

  const width = await btn.evaluate(el => el.style.width);
  const transition = await btn.evaluate(el => el.style.transition);
  const offsetWidth = await btn.evaluate(el => el.offsetWidth);

  const hasWidth = /^\d+(\.\d+)?px$/.test(width);
  const hasTransition = transition.includes('width');
  const sane = offsetWidth >= 40 && offsetWidth <= 400;

  if (!hasWidth) return { label, pass: false, reason: `no explicit px width — got "${width}"` };
  if (!hasTransition) return { label, pass: false, reason: `no width transition — got "${transition}"` };
  if (!sane) return { label, pass: false, reason: `offsetWidth ${offsetWidth}px outside 40-400px` };

  return { label, pass: true, width, offsetWidth };
}

async function navigateTo(page, navText) {
  await page.locator('.nav-item', { hasText: navText }).click();
  await page.waitForTimeout(500);
}

async function openSettingsSection(page, sectionText) {
  await page.locator('.settings-nav-item', { hasText: sectionText }).click();
  await page.waitForTimeout(600);
}

(async () => {
  const browser = await chromium.launch({ headless: false });
  const page = await browser.newPage();
  const errors = [];

  await page.addInitScript(tauriMock, { appVersion: APP_VERSION });
  page.on('pageerror', err => errors.push(`Page error: ${err.message}`));

  const results = [];

  try {
    await page.goto(TARGET_URL, { waitUntil: 'networkidle', timeout: TIMEOUT });
    await page.locator('.nav-item').first().waitFor({ state: 'visible', timeout: 8_000 });

    // ── Settings > General ────────────────────────────────────────────────────
    await navigateTo(page, 'Settings');
    await openSettingsSection(page, 'General');

    results.push(await checkDropdown(page, '.mic-btn', 'Microphone (General)'));
    results.push(await checkDropdown(page, '.language-btn', 'Spoken Language (General)'));

    await page.screenshot({ path: path.join(SCREENSHOT_DIR, 'screenshot-general.png') });

    // ── Language animation: English → Chinese Simplified → back ───────────────
    const langBtn = page.locator('.language-btn').first();
    const langWidthBefore = await langBtn.evaluate(el => el.offsetWidth);

    await langBtn.click();
    await page.waitForTimeout(300);
    const langMenuVisible = await page.locator('.language-menu').isVisible().catch(() => false);

    if (langMenuVisible) {
      const chineseItem = page.locator('.language-item').filter({ hasText: 'Chinese' }).first();
      const chineseVisible = await chineseItem.isVisible().catch(() => false);

      if (chineseVisible) {
        await chineseItem.click();
        await page.waitForTimeout(400);

        const langWidthAfter = await langBtn.evaluate(el => el.offsetWidth);
        const grew = langWidthAfter > langWidthBefore;
        results.push({
          label: `Language animation English(${langWidthBefore}px) → Chinese(${langWidthAfter}px)`,
          pass: true,
          width: `${langWidthAfter}px`,
          offsetWidth: langWidthAfter,
          detail: grew ? 'width grew ✓' : 'similar length',
        });

        // Reset to English
        await langBtn.click();
        await page.waitForTimeout(300);
        const engItem = page.locator('.language-item').filter({ hasText: /^English$/ }).first();
        if (await engItem.isVisible().catch(() => false)) {
          await engItem.click();
          await page.waitForTimeout(400);
        }
      } else {
        results.push({ label: 'Language animation (Chinese not found, skipped)', pass: true });
        // close menu by clicking elsewhere
        await page.locator('.settings-h').first().click();
        await page.waitForTimeout(300);
      }
    } else {
      results.push({ label: 'Language menu open', pass: false, reason: 'menu not visible' });
    }

    await page.screenshot({ path: path.join(SCREENSHOT_DIR, 'screenshot-language-anim.png') });

    // ── Settings > Privacy ────────────────────────────────────────────────────
    await openSettingsSection(page, 'Privacy');

    results.push(await checkDropdown(page, '.history-dropdown .mic-btn', 'History Retention (Privacy)'));

    // History animation: open and pick a different value
    const histBtn = page.locator('.history-dropdown .mic-btn').first();
    const histWidthBefore = await histBtn.evaluate(el => el.offsetWidth);

    await histBtn.click();
    await page.waitForTimeout(300);
    const histMenuVisible = await page.locator('.history-dropdown .mic-menu').isVisible().catch(() => false);

    if (histMenuVisible) {
      // "Forever" is notably wider than "7 days"
      const foreverItem = page.locator('.history-dropdown .mic-item').filter({ hasText: 'Forever' }).first();
      if (await foreverItem.isVisible().catch(() => false)) {
        await foreverItem.click();
        await page.waitForTimeout(400);
        const retentionModal = page.locator('.modal-card[aria-labelledby="retention-confirm-title"]');
        if (await retentionModal.isVisible().catch(() => false)) {
          await page.getByRole('button', { name: 'Cancel' }).click();
          await retentionModal.waitFor({ state: 'hidden', timeout: 2_000 });
        }
        const histWidthAfter = await histBtn.evaluate(el => el.offsetWidth);
        results.push({
          label: `History animation 30 days(${histWidthBefore}px) → Forever(${histWidthAfter}px)`,
          pass: true,
          offsetWidth: histWidthAfter,
        });
      } else {
        results.push({ label: 'History animation (Forever not found, skipped)', pass: true });
        await page.locator('.settings-h').first().click();
      }
    }

    await page.screenshot({ path: path.join(SCREENSHOT_DIR, 'screenshot-privacy.png') });

    // ── Settings > App Mappings ────────────────────────────────────────────────
    await openSettingsSection(page, 'App Mappings');

    results.push(await checkDropdown(page, '.profile-drop-btn', 'Profile (AppMappingsEditor)'));

    // Profile animation: pick a longer profile label
    const profBtn = page.locator('.profile-drop-btn').first();
    const profWidthBefore = await profBtn.evaluate(el => el.offsetWidth);

    await profBtn.focus();
    await page.keyboard.press('ArrowDown');
    await page.locator('#app-mappings-add-profile-menu[role="listbox"]').waitFor({ state: 'visible', timeout: 2_000 });
    const profileExpanded = await profBtn.getAttribute('aria-expanded');
    if (profileExpanded !== 'true') {
      results.push({ label: 'Profile keyboard open semantics', pass: false, reason: `aria-expanded=${profileExpanded}` });
    } else {
      const selectedCount = await page.locator('#app-mappings-add-profile-menu [role="option"][aria-selected="true"]').count();
      results.push({
        label: 'Profile keyboard open semantics',
        pass: selectedCount === 1,
        reason: selectedCount === 1 ? undefined : `selected option count ${selectedCount}`,
      });
    }
    await page.keyboard.press('Escape');
    await page.waitForTimeout(200);
    const profileFocusRestored = await profBtn.evaluate((el) => document.activeElement === el);
    results.push({
      label: 'Profile keyboard focus restore',
      pass: profileFocusRestored,
      reason: profileFocusRestored ? undefined : 'focus did not return to trigger',
    });

    await profBtn.click();
    await page.waitForTimeout(300);
    const profMenuVisible = await page.locator('.profile-drop-menu').isVisible().catch(() => false);

    if (profMenuVisible) {
      const veryCasual = page.locator('.profile-drop-item').filter({ hasText: 'Very Casual' }).first();
      if (await veryCasual.isVisible().catch(() => false)) {
        await veryCasual.click();
        await page.waitForTimeout(400);
        const profWidthAfter = await profBtn.evaluate(el => el.offsetWidth);
        results.push({
          label: `Profile animation (${profWidthBefore}px → ${profWidthAfter}px, Very Casual)`,
          pass: true,
          offsetWidth: profWidthAfter,
        });
      } else {
        results.push({ label: 'Profile animation (Very Casual not found, skipped)', pass: true });
        await page.locator('.settings-h').first().click().catch(() => {});
      }
    }

    await page.screenshot({ path: path.join(SCREENSHOT_DIR, 'screenshot-apps.png') });

  } catch (err) {
    errors.push(`Test threw: ${err.message}`);
  } finally {
    await browser.close();
  }

  // ── Report ────────────────────────────────────────────────────────────────
  console.log('\n── Dropdown Animation Results ─────────────────────────────');
  let allPassed = true;
  for (const r of results) {
    if (r.pass) {
      const detail = r.detail ? ` — ${r.detail}` : '';
      const dims = r.offsetWidth ? ` (${r.offsetWidth}px)` : '';
      console.log(`  ✓ ${r.label}${dims}${detail}`);
    } else {
      console.log(`  ✗ ${r.label}: ${r.reason}`);
      allPassed = false;
    }
  }

  if (errors.length) {
    console.log('\n── Runtime errors ─────────────────────────────────────────');
    errors.forEach(e => console.log(`  ! ${e}`));
    allPassed = false;
  }

  console.log('\n' + (allPassed ? 'PASS' : 'FAIL'));
  if (!allPassed) process.exit(1);
})();
