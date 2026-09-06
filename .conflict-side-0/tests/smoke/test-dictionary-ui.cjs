// Dictionary UI test — verifies layout, "often" display, inspector, and no flicker.
// Connects to the Vite dev server (port 1420, see vite.config). Start with: npm run dev
const { chromium } = require('playwright');
const { tauriMock, APP_VERSION } = require('./_tauri-mock.cjs');
const path = require('path');
const fs   = require('fs');

const TARGET_URL = process.env.TEST_URL || 'http://localhost:1420';
const TIMEOUT    = 10_000;
const SS_DIR     = path.join(__dirname, '../../tmp-screenshots');

fs.mkdirSync(SS_DIR, { recursive: true });

const DICT_DATA = [
  { id: 1, term: 'Kubernetes',  mistake: 'koobernetes', auto_learned: false, correction_count: 3, created_at: '2025-01-01T00:00:00' },
  { id: 2, term: 'Björk',       mistake: null,          auto_learned: false, correction_count: 0, created_at: '2025-01-02T00:00:00' },
  { id: 3, term: 'ChatGPT',     mistake: 'chat GPT',    auto_learned: true,  correction_count: 5, created_at: '2025-01-03T00:00:00' },
  { id: 4, term: 'Tauri',       mistake: 'Tari',        auto_learned: false, correction_count: 1, created_at: '2025-01-04T00:00:00' },
];

(async () => {
  console.log('=== Dictionary UI Tests ===');
  console.log(`Target: ${TARGET_URL}`);
  const browser = await chromium.launch({ headless: true });
  const page    = await browser.newPage();
  const errors  = [];
  let   passed  = 0;
  let   failed  = 0;

  // This legacy-page test opts into the legacy navigation explicitly.
  await page.addInitScript(() => {
    localStorage.setItem('__open_flow_tauri_mock_settings', JSON.stringify({ legacy_features_enabled: true }));
  });
  // Inject Tauri mock + dictionary data override
  await page.addInitScript(tauriMock, { appVersion: APP_VERSION });
  await page.addInitScript((data) => {
    const orig = window.__TAURI_INTERNALS__.invoke;
    window.__TAURI_INTERNALS__.invoke = async (cmd, args) => {
      if (cmd === 'get_dictionary') return data;
      return orig(cmd, args);
    };
  }, DICT_DATA);

  page.on('pageerror', err => errors.push(`Page exception: ${err.message}`));
  page.on('console',   msg => {
    if (msg.type() === 'error') errors.push(`Console error: ${msg.text()}`);
  });

  function pass(msg) { console.log(`  ✓ ${msg}`); passed++; }
  function fail(msg) { console.error(`  ✗ ${msg}`); failed++; errors.push(msg); }

  async function ss(name) {
    await page.screenshot({ path: path.join(SS_DIR, `dict-${name}.png`), fullPage: false });
  }

  try {
    await page.goto(TARGET_URL, { waitUntil: 'networkidle', timeout: TIMEOUT });

    // ── Navigate to Dictionary ────────────────────────────────────────────────
    const dictNav = page.locator('.nav-item:has-text("Dictionary")');
    await dictNav.waitFor({ state: 'visible', timeout: TIMEOUT });
    await dictNav.click();
    await page.locator('h1.page-h:has-text("Dictionary")').waitFor({ state: 'visible', timeout: 4_000 });
    pass('Dictionary view loaded');
    await ss('01-loaded');

    // ── Toolbar: search + sort pills + add button ─────────────────────────────
    await page.locator('.search').waitFor({ state: 'visible', timeout: 3_000 });
    pass('Search bar present');

    const pills = page.locator('.sort-pill');
    const pillCount = await pills.count();
    if (pillCount === 4) pass(`Sort pills: ${pillCount} found`);
    else fail(`Sort pills: expected 4, got ${pillCount}`);

    const pillTexts = await pills.allInnerTexts();
    const expectedPills = ['Newest', 'Oldest', 'A → Z', 'Most corrected'];
    const pillsOk = expectedPills.every(t => pillTexts.some(p => p.includes(t)));
    if (pillsOk) pass('Sort pill labels correct');
    else fail(`Sort pill labels wrong: ${pillTexts}`);

    await page.locator('button:has-text("Add term")').waitFor({ state: 'visible', timeout: 3_000 });
    pass('Add term button present');

    // ── List populated ────────────────────────────────────────────────────────
    await page.waitForTimeout(300); // let data render
    const rows = page.locator('.dict-row');
    const rowCount = await rows.count();
    if (rowCount === 4) pass(`List rows: ${rowCount} found`);
    else fail(`List rows: expected 4, got ${rowCount}`);

    await ss('02-list');

    // ── "often:" label present in rows with mistake ───────────────────────────
    // Kubernetes and Tauri have mistakes; Björk does not
    const oftenLabels = page.locator('.dict-often-label');
    const oftenCount = await oftenLabels.count();
    // DICT_DATA has 3 entries with mistake (Kubernetes, ChatGPT, Tauri)
    if (oftenCount === 3) pass(`"often:" labels: ${oftenCount} found in list`);
    else fail(`"often:" labels: expected 3, got ${oftenCount}`);

    // ── No arrow SVG in list rows ─────────────────────────────────────────────
    const arrows = page.locator('.dict-arrow');
    const arrowCount = await arrows.count();
    if (arrowCount === 0) pass('No arrow elements in list rows');
    else fail(`Arrow elements still present: ${arrowCount}`);

    // ── auto-star present for auto_learned entries ────────────────────────────
    const stars = page.locator('.dict-auto-star');
    const starCount = await stars.count();
    if (starCount === 1) pass('Auto-star present for auto-learned entry (ChatGPT)');
    else fail(`Auto-stars: expected 1, got ${starCount}`);

    // ── Click a row → inspector opens ────────────────────────────────────────
    await rows.first().click();
    await page.waitForTimeout(250);
    const inspector = page.locator('.inspector');
    const inspCount = await inspector.count();
    if (inspCount >= 1) pass('Inspector opened after row click');
    else fail('Inspector not found after row click');

    await ss('03-inspector');

    // ── Inspector shows "often:" inline (not arrow) ───────────────────────────
    const inspOften = page.locator('.insp-often');
    const inspOftenCount = await inspOften.count();
    // First row is Kubernetes (newest sort → id 4 Tauri, id 3 ChatGPT, id 2 Björk, id 1 Kubernetes)
    // Actually newest sort: created_at descending → Tauri(4) > ChatGPT(3) > Björk(2) > Kubernetes(1)
    // So first row = Tauri, which has a mistake → insp-often should show
    if (inspOftenCount === 1) pass('Inspector shows insp-often section');
    else fail(`Inspector insp-often: expected 1, got ${inspOftenCount}`);

    const inspArrow = page.locator('.insp-arrow');
    const inspArrowCount = await inspArrow.count();
    if (inspArrowCount === 0) pass('No insp-arrow in inspector');
    else fail(`insp-arrow still present: ${inspArrowCount}`);

    // ── Auto-learned badge in inspector for ChatGPT ───────────────────────────
    // Click ChatGPT row (id 3, 2nd newest)
    await rows.nth(1).click();
    await page.waitForTimeout(250);
    const badge = page.locator('.insp-auto-badge');
    const badgeCount = await badge.count();
    if (badgeCount === 1) pass('Auto-learned badge shown in inspector');
    else fail(`Auto-learned badge: expected 1, got ${badgeCount}`);

    await ss('04-auto-learned');

    // ── Björk has no "often:" in inspector ───────────────────────────────────
    // Click Björk row (id 2, 3rd newest)
    await rows.nth(2).click();
    await page.waitForTimeout(250);
    const inspOftenBjork = await page.locator('.insp-often').count();
    if (inspOftenBjork === 0) pass('Inspector shows no insp-often for entry without mistake');
    else fail(`insp-often shown for Björk (no mistake): count ${inspOftenBjork}`);

    await ss('05-no-mistake');

    // ── Quick-click flicker test: max 1 .inspector in DOM at any time ─────────
    // Click rows rapidly and check we never have 2+ inspectors
    let maxInspectors = 0;
    for (let i = 0; i < 4; i++) {
      await rows.nth(i % 4).click({ delay: 0 });
      const count = await page.locator('.inspector').count();
      if (count > maxInspectors) maxInspectors = count;
    }
    await page.waitForTimeout(400); // let any transition finish
    if (maxInspectors <= 1) pass(`Flicker test: max inspectors in DOM = ${maxInspectors} (no double-stack)`);
    else fail(`Flicker test: ${maxInspectors} inspectors in DOM simultaneously`);

    await ss('06-after-rapid-click');

    // ── Sort indicator slides: click a different pill ─────────────────────────
    const alphaPill = page.locator('.sort-pill:has-text("A → Z")');
    await alphaPill.click();
    await page.waitForTimeout(350);
    const activeSort = page.locator('.sort-pill.active');
    const activeSortText = await activeSort.innerText();
    if (activeSortText.includes('A → Z')) pass('Sort pill active state switches correctly');
    else fail(`Active sort pill: expected "A → Z", got "${activeSortText}"`);

    // After alpha sort, first row should be "Björk" (B < C < K < T)
    const firstRowText = await rows.first().innerText();
    if (firstRowText.includes('Björk')) pass('Alpha sort: Björk is first');
    else fail(`Alpha sort: first row = "${firstRowText.slice(0, 40)}"`);

    await ss('07-sorted-alpha');

    // ── Two-column layout ─────────────────────────────────────────────────────
    const listCol = page.locator('.dict-list-col');
    const inspCol = page.locator('.inspector-col');
    const listBox = await listCol.boundingBox();
    const inspBox = await inspCol.boundingBox();
    if (listBox && inspBox && Math.abs(listBox.x - inspBox.x) > 50) {
      pass('Two-column layout: list and inspector are side by side');
    } else {
      fail('Two-column layout: columns not side by side');
    }

    // ── Add term modal ────────────────────────────────────────────────────────
    const addTermButton = page.locator('button:has-text("Add term")');
    await addTermButton.click();
    await page.waitForTimeout(200);
    const modalCard = page.locator('.modal-card');
    if (await modalCard.count() === 1) pass('Add term modal opens');
    else fail('Add term modal did not open');
    const modalFocusInside = await modalCard.evaluate((el) => el.contains(document.activeElement));
    if (modalFocusInside) pass('Dictionary modal moves focus inside the dialog');
    else fail('Dictionary modal did not move focus inside the dialog');

    // Escape closes modal
    await page.keyboard.press('Escape');
    await page.waitForTimeout(200);
    if (await page.locator('.modal-card').count() === 0) pass('Escape closes modal');
    else fail('Modal still open after Escape');
    const addFocusRestored = await addTermButton.evaluate((el) => document.activeElement === el);
    if (addFocusRestored) pass('Dictionary modal restores focus to the trigger');
    else fail('Dictionary modal did not restore focus to the trigger');

    await ss('08-final');

  } catch (err) {
    fail(`Unexpected error: ${err.message}`);
    await ss('error');
  } finally {
    await browser.close();
  }

  console.log(`\n=== Results: ${passed} passed, ${failed} failed ===`);
  if (errors.length) {
    console.error('\nFailures:');
    errors.forEach(e => console.error(`  - ${e}`));
  }
  console.log(`Screenshots saved to: ${SS_DIR}`);
  process.exit(failed > 0 ? 1 : 0);
})();
