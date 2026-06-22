import { tick } from 'svelte';

export async function focusListboxOption(menuId: string, preferLast = false) {
  await tick();
  const menu = document.getElementById(menuId);
  if (!menu) return;

  const options = Array.from(menu.querySelectorAll<HTMLElement>('[role="option"]'));
  if (options.length === 0) return;

  const selected = options.find((option) => option.getAttribute('aria-selected') === 'true');
  const fallback = preferLast ? options[options.length - 1] : options[0];
  (selected ?? fallback).focus();
}

export function moveListboxFocus(event: KeyboardEvent, menuId: string) {
  const menu = document.getElementById(menuId);
  if (!menu) return;

  const options = Array.from(menu.querySelectorAll<HTMLElement>('[role="option"]'));
  if (options.length === 0) return;

  const activeIndex = Math.max(0, options.indexOf(document.activeElement as HTMLElement));
  if (event.key === 'Home') {
    event.preventDefault();
    options[0].focus();
    return;
  }
  if (event.key === 'End') {
    event.preventDefault();
    options[options.length - 1].focus();
    return;
  }
  if (event.key === 'ArrowDown') {
    event.preventDefault();
    options[(activeIndex + 1) % options.length].focus();
    return;
  }
  if (event.key === 'ArrowUp') {
    event.preventDefault();
    options[(activeIndex - 1 + options.length) % options.length].focus();
  }
}

export function handleListboxOptionKeydown(
  event: KeyboardEvent,
  menuId: string,
  restoreFocus?: () => void,
) {
  if (event.key === 'Escape') {
    event.preventDefault();
    event.stopPropagation();
    restoreFocus?.();
    return;
  }

  moveListboxFocus(event, menuId);
}
