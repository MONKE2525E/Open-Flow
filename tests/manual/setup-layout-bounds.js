import { chromium } from 'playwright';

const TARGET_URL = 'http://localhost:1420';
const VIEWPORTS = [
  { width: 1100, height: 720, label: '1100x720' },
  { width: 900, height: 600, label: '900x600' },
];

async function readLayoutState(page) {
  return await page.evaluate(() => {
    const html = document.documentElement;
    const body = document.body;
    const overlay = document.querySelector('.setup-overlay');
    const wrap = document.querySelector('.step-wrap');
    const step = document.querySelector('.step');
    const footer = document.querySelector('.step-footer');
    const heading = document.querySelector('.step-header h2, .done-title, .brand-name');

    const rect = (el) => el ? el.getBoundingClientRect() : null;
    const stepRect = rect(step);
    const wrapRect = rect(wrap);
    const footerRect = rect(footer);

    const pageScroll =
      html.scrollHeight !== html.clientHeight ||
      body.scrollHeight !== body.clientHeight;

    const inBounds = !!(
      stepRect &&
      wrapRect &&
      stepRect.top >= wrapRect.top &&
      stepRect.bottom <= wrapRect.bottom
    );

    const scrollableVisibleContainers = Array.from(
      overlay?.querySelectorAll('*') ?? []
    )
      .filter((el) => {
        const style = getComputedStyle(el);
        const hasScrollableOverflow = style.overflowY === 'auto' || style.overflowY === 'scroll';
        const visible = el.getClientRects().length > 0;
        return hasScrollableOverflow && visible && el.scrollHeight > el.clientHeight;
      })
      .map((el) => ({
        className: el.className,
        overflowY: getComputedStyle(el).overflowY,
        clientHeight: el.clientHeight,
        scrollHeight: el.scrollHeight,
      }));

    return {
      title: heading?.textContent?.trim() ?? 'unknown',
      pageScroll,
      inBounds,
      footerBottom: footerRect?.bottom ?? null,
      wrapBottom: wrapRect?.bottom ?? null,
      scrollableVisibleContainers,
    };
  });
}

async function verifyViewport(browser, viewport) {
  const page = await browser.newPage({ viewport: { width: viewport.width, height: viewport.height } });
  const failures = [];

  await page.goto(TARGET_URL, { waitUntil: 'networkidle', timeout: 15000 });
  await page.waitForTimeout(500);

  for (let stepIndex = 0; stepIndex <= 8; stepIndex++) {
    const state = await readLayoutState(page);
    console.log(`[${viewport.label}] Step ${stepIndex}: ${state.title}`);

    if (state.pageScroll) {
      failures.push(`[${viewport.label}] "${state.title}" has page scroll (html/body mismatch).`);
    }
    if (!state.inBounds) {
      failures.push(`[${viewport.label}] "${state.title}" content exceeds step container bounds.`);
    }
    if (state.scrollableVisibleContainers.length > 0) {
      failures.push(
        `[${viewport.label}] "${state.title}" has internal scrollable container(s): ${JSON.stringify(state.scrollableVisibleContainers)}`
      );
    }
    if (state.footerBottom !== null && state.wrapBottom !== null && state.footerBottom > state.wrapBottom) {
      failures.push(`[${viewport.label}] "${state.title}" footer extends below visible area.`);
    }

    const finalButton = page.locator('.step-wrap.visible .btn-primary:has-text("Start dictating")');
    if (await finalButton.count()) break;

    const nextButton = page.locator('.step-wrap.visible .btn-primary').first();
    if (!(await nextButton.count())) break;
    await nextButton.click();
    await page.waitForTimeout(420);
  }

  await page.close();
  return failures;
}

(async () => {
  const browser = await chromium.launch({ headless: true });
  const allFailures = [];

  try {
    for (const viewport of VIEWPORTS) {
      const failures = await verifyViewport(browser, viewport);
      allFailures.push(...failures);
    }
  } finally {
    await browser.close();
  }

  if (allFailures.length > 0) {
    console.error('FAILED: setup onboarding layout checks');
    allFailures.forEach((f) => console.error(`  - ${f}`));
    process.exit(1);
  }

  console.log('PASS: setup onboarding has no scroll and stays in bounds at 1100x720 and 900x600');
  process.exit(0);
})();
