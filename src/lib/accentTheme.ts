export const ACCENT_CHANGE_EVENT = 'verenu:accent-color-changed';

const HEX_COLOR = /^#[0-9a-f]{6}$/i;
const ACCENT_PROPERTIES = [
  '--jap-50',
  '--jap-100',
  '--jap-200',
  '--jap-300',
  '--jap-400',
  '--jap-500',
  '--jap-600',
  '--jap-700',
  '--accent',
  '--accent-ink',
  '--accent-soft',
  '--on-accent',
] as const;

export function normalizeAccentColor(value: unknown): string | null {
  if (typeof value !== 'string') return null;
  const trimmed = value.trim();
  if (!HEX_COLOR.test(trimmed)) return null;
  return trimmed.toUpperCase();
}

function relativeLuminance(hex: string): number {
  const channels = [1, 3, 5].map((offset) => {
    const value = Number.parseInt(hex.slice(offset, offset + 2), 16) / 255;
    return value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
  });
  return channels[0] * 0.2126 + channels[1] * 0.7152 + channels[2] * 0.0722;
}

export function foregroundForAccent(hex: string): '#17100C' | '#FFFFFF' {
  const normalized = normalizeAccentColor(hex);
  if (!normalized) return '#FFFFFF';
  const luminance = relativeLuminance(normalized);
  const whiteContrast = 1.05 / (luminance + 0.05);
  const darkLuminance = relativeLuminance('#17100C');
  const darkContrast = (luminance + 0.05) / (darkLuminance + 0.05);
  return darkContrast >= whiteContrast ? '#17100C' : '#FFFFFF';
}

export function applyAccentTheme(
  root: HTMLElement,
  value: string | null,
  _options: { animate?: boolean } = {},
): void {
  const accent = normalizeAccentColor(value);

  if (!accent) {
    for (const property of ACCENT_PROPERTIES) root.style.removeProperty(property);
    return;
  }

  // The chosen color stays exact at --accent. The rest of the scale mixes
  // against the current theme tokens, so one custom color remains readable in
  // both light and dark mode without maintaining two saved palettes.
  const soft = (amount: number) => `color-mix(in srgb, ${accent} ${amount}%, var(--paper))`;
  const ink = (amount: number) => `color-mix(in srgb, ${accent} ${amount}%, var(--ink))`;
  const palette: Record<(typeof ACCENT_PROPERTIES)[number], string> = {
    '--jap-50': soft(6),
    '--jap-100': soft(14),
    '--jap-200': soft(30),
    '--jap-300': `color-mix(in srgb, ${accent} 68%, var(--bg-elev))`,
    '--jap-400': accent,
    '--jap-500': ink(88),
    '--jap-600': ink(78),
    '--jap-700': ink(68),
    '--accent': accent,
    '--accent-ink': ink(68),
    '--accent-soft': soft(14),
    '--on-accent': foregroundForAccent(accent),
  };

  for (const property of ACCENT_PROPERTIES) root.style.setProperty(property, palette[property]);
}

type ViewTransitionDocument = Document & {
  startViewTransition?: (update: () => void | Promise<void>) => { finished: Promise<void> };
};

export async function animateAccentChange(update: () => void | Promise<void>): Promise<void> {
  const viewTransitionDocument = document as ViewTransitionDocument;
  if (
    !viewTransitionDocument.startViewTransition
    || window.matchMedia?.('(prefers-reduced-motion: reduce)').matches
  ) {
    await update();
    return;
  }

  try {
    const transition = viewTransitionDocument.startViewTransition(update);
    await transition.finished.catch(() => {});
  } catch {
    await update();
  }
}
