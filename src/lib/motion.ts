export const MOTION_MS = {
  fast: 150,
  base: 220,
  panel: 280,
  page: 340,
} as const;

export const MOTION_PX = {
  nudge: 6,
  lift: 8,
  panel: 14,
  page: 18,
} as const;

export const NAV_ORDER = ['home', 'dictionary', 'snippets', 'style'] as const;
export const SETTINGS_SECTION_ORDER = ['general', 'apps', 'keys', 'models', 'privacy', 'advanced', 'about'] as const;
export const STYLE_TAB_ORDER = ['cleanup', 'personal', 'apps'] as const;

export function directionFromOrder(current: string, next: string, order: readonly string[]): 1 | -1 {
  const oldIdx = order.indexOf(current);
  const newIdx = order.indexOf(next);
  if (oldIdx === -1 || newIdx === -1 || oldIdx === newIdx) return 1;
  return newIdx > oldIdx ? 1 : -1;
}

export function reducedMotionEnabled(): boolean {
  return typeof window !== 'undefined' && window.matchMedia?.('(prefers-reduced-motion: reduce)').matches === true;
}

export function motionMs(ms: number): number {
  return reducedMotionEnabled() ? Math.max(80, Math.round(ms * 0.6)) : ms;
}

export function motionPx(px: number): number {
  return reducedMotionEnabled() ? Math.max(2, Math.round(px * 0.5)) : px;
}

export interface AnimateWidthParams {
  text: string;
  min?: number;
  max?: number;
}

export function animateWidth(node: HTMLElement, params: AnimateWidthParams = { text: '' }) {
  const transition = () =>
    `width ${motionMs(MOTION_MS.base)}ms cubic-bezier(0.22, 1, 0.36, 1)`;

  function applyWidth(animate: boolean) {
    const { min = 0, max = Infinity } = params;
    const prevWidth = node.style.width;

    // Let the browser compute the natural width — no manual font measurement
    node.style.transition = 'none';
    node.style.width = 'max-content';
    const natural = node.getBoundingClientRect().width;
    const target = Math.max(min, Math.min(Math.ceil(natural), max));

    if (animate && prevWidth) {
      node.style.width = prevWidth;
      void node.offsetWidth;
      node.style.transition = transition();
      node.style.width = `${target}px`;
    } else {
      node.style.width = `${target}px`;
      void node.offsetWidth;
      node.style.transition = transition();
    }
  }

  let rafId = 0;
  let domListener: (() => void) | null = null;

  if (document.readyState === 'loading') {
    domListener = () => { rafId = requestAnimationFrame(() => applyWidth(false)); };
    document.addEventListener('DOMContentLoaded', domListener, { once: true });
  } else {
    rafId = requestAnimationFrame(() => applyWidth(false));
  }

  return {
    update(newParams: AnimateWidthParams) {
      params = newParams;
      cancelAnimationFrame(rafId);
      rafId = requestAnimationFrame(() => applyWidth(true));
    },
    destroy() {
      cancelAnimationFrame(rafId);
      if (domListener) document.removeEventListener('DOMContentLoaded', domListener);
    },
  };
}
