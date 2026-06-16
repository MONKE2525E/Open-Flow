import { cubicOut } from 'svelte/easing';

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
export const SETTINGS_SECTION_ORDER = ['general', 'apps', 'keys', 'models', 'privacy', 'advanced', 'about', 'developer'] as const;
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

export interface MotionTransitionParams {
  duration?: number;
  distance?: number;
  axis?: 'x' | 'y';
  scaleFrom?: number;
}

interface TransitionOptions {
  direction?: 'in' | 'out' | 'both';
}

export function modalBackdrop(_node: Element, params: MotionTransitionParams = {}) {
  const duration = motionMs(params.duration ?? 180);
  return {
    duration,
    easing: cubicOut,
    css: (t: number) => `opacity:${t};`,
  };
}

export function modalCard(_node: Element, params: MotionTransitionParams = {}) {
  const duration = motionMs(params.duration ?? 220);
  const distance = params.distance ?? motionPx(MOTION_PX.panel);
  const scaleFrom = params.scaleFrom ?? 0.97;

  return {
    duration,
    easing: cubicOut,
    css: (t: number) => {
      const u = 1 - t;
      const scale = scaleFrom + (1 - scaleFrom) * t;
      return `opacity:${t}; transform: translate3d(0, ${u * distance}px, 0) scale(${scale});`;
    },
  };
}

export function pageSwap(node: HTMLElement, params: MotionTransitionParams = {}, options: TransitionOptions = {}) {
  const duration = motionMs(params.duration ?? 260);
  const axis = params.axis ?? 'y';
  const distance = params.distance ?? motionPx(MOTION_PX.page);
  const isOutro = options.direction === 'out';

  return {
    duration,
    easing: cubicOut,
    tick: () => {
      if (isOutro) {
        node.inert = true;
        node.setAttribute('aria-hidden', 'true');
        node.style.pointerEvents = 'none';
      } else {
        node.inert = false;
        node.removeAttribute('aria-hidden');
        node.style.pointerEvents = '';
      }
    },
    css: (t: number) => {
      const u = 1 - t;
      const x = axis === 'x' ? u * distance : 0;
      const y = axis === 'y' ? u * distance : 0;
      return `opacity:${0.001 + t * 0.999}; transform: translate3d(${x}px, ${y}px, 0);`;
    },
  };
}

export function listItemCollapse(node: HTMLElement, params: MotionTransitionParams = {}) {
  const duration = motionMs(params.duration ?? 200);
  const style = getComputedStyle(node);
  const height = node.offsetHeight;
  const paddingTop = parseFloat(style.paddingTop) || 0;
  const paddingBottom = parseFloat(style.paddingBottom) || 0;
  const marginTop = parseFloat(style.marginTop) || 0;
  const marginBottom = parseFloat(style.marginBottom) || 0;
  const borderTopWidth = parseFloat(style.borderTopWidth) || 0;
  const borderBottomWidth = parseFloat(style.borderBottomWidth) || 0;

  return {
    duration,
    easing: cubicOut,
    css: (t: number) => `
      overflow: hidden;
      opacity: ${t};
      height: ${Math.max(0, height * t)}px;
      padding-top: ${paddingTop * t}px;
      padding-bottom: ${paddingBottom * t}px;
      margin-top: ${marginTop * t}px;
      margin-bottom: ${marginBottom * t}px;
      border-top-width: ${borderTopWidth * t}px;
      border-bottom-width: ${borderBottomWidth * t}px;
    `,
  };
}

export interface ExpandFromOriginParams {
  origin?: { x: number; y: number };
  duration?: number;
}

export function expandFromOrigin(node: Element, params: ExpandFromOriginParams = {}) {
  const duration = motionMs(params.duration ?? 240);
  const nodeRect = node.getBoundingClientRect();
  const originX = params.origin ? params.origin.x - nodeRect.left : nodeRect.width / 2;
  const originY = params.origin ? params.origin.y - nodeRect.top : nodeRect.height / 2;
  const scaleFrom = 0.18;

  return {
    duration,
    easing: cubicOut,
    css: (t: number) => {
      const scale = scaleFrom + (1 - scaleFrom) * t;
      return `opacity:${t}; transform: scale(${scale}); transform-origin: ${originX}px ${originY}px;`;
    },
  };
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
