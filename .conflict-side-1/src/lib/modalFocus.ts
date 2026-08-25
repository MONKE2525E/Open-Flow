type ModalFocusOptions = {
  active: boolean;
  initialFocus?: () => HTMLElement | null;
  restoreFocus?: () => HTMLElement | null;
};

const FOCUSABLE_SELECTOR = [
  'button:not([disabled])',
  'input:not([disabled])',
  'select:not([disabled])',
  'textarea:not([disabled])',
  'a[href]',
  '[tabindex]:not([tabindex="-1"])',
].join(',');

function getFocusableElements(node: HTMLElement): HTMLElement[] {
  return Array.from(node.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR)).filter(
    (el) => !el.hasAttribute('inert') && el.offsetParent !== null,
  );
}

export function modalFocusTrap(node: HTMLElement, options: ModalFocusOptions) {
  let current = options;
  let previousFocus: HTMLElement | null = null;

  function tryFocusInside(): boolean {
    const preferred = current.initialFocus?.();
    const [first] = getFocusableElements(node);
    const target = preferred ?? first ?? node;
    if (!target?.isConnected) {
      return false;
    }
    target.focus();
    return node.contains(document.activeElement);
  }

  function restoreFocus() {
    const target = current.restoreFocus?.() ?? previousFocus;
    previousFocus = null;
    if (target?.isConnected) {
      requestAnimationFrame(() => target.focus());
    }
  }

  function focusInside() {
    if (
      !previousFocus &&
      document.activeElement instanceof HTMLElement &&
      !node.contains(document.activeElement)
    ) {
      previousFocus = document.activeElement;
    }

    if (tryFocusInside()) {
      return;
    }

    queueMicrotask(() => {
      if (!current.active || tryFocusInside()) {
        return;
      }
      requestAnimationFrame(() => {
        if (!current.active || tryFocusInside()) {
          return;
        }
        requestAnimationFrame(() => {
          if (current.active) {
            tryFocusInside();
          }
        });
      });
    });
  }

  function handleKeydown(event: KeyboardEvent) {
    if (!current.active || event.key !== 'Tab') return;

    const focusable = getFocusableElements(node);
    if (focusable.length === 0) {
      event.preventDefault();
      node.focus();
      return;
    }

    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    const active = document.activeElement;

    if (event.shiftKey && (active === first || active === node)) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && active === last) {
      event.preventDefault();
      first.focus();
    }
  }

  node.addEventListener('keydown', handleKeydown);
  if (current.active) {
    focusInside();
  }

  return {
    update(next: ModalFocusOptions) {
      const wasActive = current.active;
      current = next;
      if (!wasActive && current.active) {
        focusInside();
      } else if (wasActive && !current.active) {
        restoreFocus();
      }
    },
    destroy() {
      node.removeEventListener('keydown', handleKeydown);
      if (current.active) {
        restoreFocus();
      }
    },
  };
}
