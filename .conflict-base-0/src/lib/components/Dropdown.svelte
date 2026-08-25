<script lang="ts">
  import { tick } from 'svelte';

  let {
    open = $bindable(false),
    closeSelector = '',
    restoreFocus = true,
    optionSelector = '.ui-dropdown-option',
    focusOnOpen = '',
    children,
  }: {
    open: boolean;
    closeSelector?: string;
    restoreFocus?: boolean;
    optionSelector?: string;
    /**
     * Selector for an element to focus instead of an option — a searchable
     * menu should land in its search field, not on a row. Unset keeps the
     * default selected-or-first-option behaviour.
     */
    focusOnOpen?: string;
    children?: import('svelte').Snippet;
  } = $props();

  let trigger: HTMLElement | null = null;
  let root: HTMLElement | null = null;
  let wasOpen = false;

  $effect(() => {
    if (open) {
      // The trigger is whatever has focus when the menu opens — the button that
      // was clicked or keyboard-activated. Its .ui-dropdown ancestor scopes all
      // menu interactions so nested menus never cross-talk. This is
      // gesture-dependent: a future consumer that opens the menu programmatically
      // (no user gesture) would capture null and lose restore/arrow-nav.
      trigger = document.activeElement instanceof HTMLElement ? document.activeElement : null;
      root = trigger ? trigger.closest(closeSelector || '.ui-dropdown') : null;
    }

    if (wasOpen && !open) {
      // Close happened. If focus fell out of the page (option unmounted) or is
      // still inside the menu shell, hand it back to the trigger. If another
      // control claimed focus (outside click, a modal that just opened), leave
      // it alone — racing a modal's trap would yank focus out of the dialog.
      requestAnimationFrame(() => {
        if (!restoreFocus || !trigger?.isConnected) return;
        const active = document.activeElement;
        const claimed = active instanceof HTMLElement && active !== document.body && !(root?.contains(active));
        if (!claimed) trigger.focus();
      });
    }
    wasOpen = open;

    if (!open) return;

    let disposed = false;
    tick().then(() => {
      if (disposed) return;
      window.addEventListener('click', handleOutsideClick);
      focusOptionOnOpen();
    });
    window.addEventListener('keydown', handleWindowKeydown);

    return () => {
      disposed = true;
      window.removeEventListener('click', handleOutsideClick);
      window.removeEventListener('keydown', handleWindowKeydown);
    };
  });

  function focusOptionOnOpen() {
    if (!root) return;
    if (focusOnOpen) {
      const target = root.querySelector<HTMLElement>(focusOnOpen);
      if (target) {
        target.focus();
        // Options still need their roving tabindex set up so arrow keys work
        // once focus moves into the list.
        const options = Array.from(root.querySelectorAll<HTMLElement>(optionSelector));
        setOptionTabindexes(options, options[0] ?? null);
        return;
      }
    }
    const options = Array.from(root.querySelectorAll<HTMLElement>(optionSelector));
    if (options.length === 0) return;
    const selected = options.find((option) => option.getAttribute('aria-selected') === 'true');
    const target = selected ?? options[0];
    target.focus();
    setOptionTabindexes(options, target);
  }

  // The menu is an overlay, so its options are not in the page tab order —
  // only the option that currently has focus keeps a tabindex, so a long menu
  // (e.g. the 57-language list) can't turn Tab into a maze. Arrow keys still
  // rove through every option.
  function setOptionTabindexes(options: HTMLElement[], active: HTMLElement | null) {
    for (const option of options) option.tabIndex = option === active ? 0 : -1;
  }

  function handleOutsideClick(e: MouseEvent) {
    // closest() only exists on Element; a click target can be a Text node or
    // document, so resolve to the nearest element first before testing the
    // close selector (e.g. clicking the label text inside the trigger).
    const target = e.target instanceof Element
      ? e.target
      : (e.target as Node)?.parentElement;
    // Do not rely solely on the element that was focused when the menu opened.
    // On some WebViews a pointer click does not focus the trigger, leaving
    // `root` unset. The opening click then reaches this window listener after
    // the menu is mounted and immediately closes it again. Resolve the scope
    // from the click target as a fallback so mouse-opened menus stay usable.
    const targetRoot = target?.closest(closeSelector || '.ui-dropdown');
    if (target && (root?.contains(target) || targetRoot)) return;
    open = false;
  }

  function handleWindowKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      if (!open) return;
      // Close the menu before page-level Escape handlers run. Note that
      // stopPropagation on a window listener does not stop other window
      // listeners (e.g. Settings' close-on-Escape) — those are already
      // protected by their own `[aria-expanded="true"]` guard, which still
      // reads true during this dispatch.
      e.stopPropagation();
      open = false;
      return;
    }

    if (!open || !root) return;
    const target = e.target instanceof Element ? e.target : null;
    const option = target?.closest<HTMLElement>(optionSelector);
    if (!option) return;
    const menuRoot = option.closest(closeSelector || '.ui-dropdown');
    if (menuRoot !== root) return;

    const options = Array.from(root.querySelectorAll<HTMLElement>(optionSelector));
    const index = options.indexOf(option);
    if (index === -1) return;

    switch (e.key) {
      case 'ArrowDown':
        e.preventDefault();
        setOptionTabindexes(options, options[(index + 1) % options.length]);
        options[(index + 1) % options.length].focus();
        break;
      case 'ArrowUp':
        e.preventDefault();
        setOptionTabindexes(options, options[(index - 1 + options.length) % options.length]);
        options[(index - 1 + options.length) % options.length].focus();
        break;
      case 'Home':
        e.preventDefault();
        setOptionTabindexes(options, options[0]);
        options[0].focus();
        break;
      case 'End':
        e.preventDefault();
        setOptionTabindexes(options, options[options.length - 1]);
        options[options.length - 1].focus();
        break;
    }
  }
</script>

{@render children?.()}
