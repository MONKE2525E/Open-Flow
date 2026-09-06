import { describe, expect, it } from 'vitest';
import {
  foregroundForAccent,
  foregroundForDarkSurface,
  isAdaptiveDefaultAccent,
  normalizeAccentColor,
} from './accentTheme';

describe('accent theme', () => {
  it('normalizes valid six-digit colors', () => {
    expect(normalizeAccentColor(' #4f7fd8 ')).toBe('#4F7FD8');
    expect(normalizeAccentColor('#abc')).toBeNull();
    expect(normalizeAccentColor('orange')).toBeNull();
  });

  it('treats only exact black and white as adaptive defaults', () => {
    expect(isAdaptiveDefaultAccent('#000000')).toBe(true);
    expect(isAdaptiveDefaultAccent('#ffffff')).toBe(true);
    expect(isAdaptiveDefaultAccent('#000001')).toBe(false);
    expect(isAdaptiveDefaultAccent('#FEFEFE')).toBe(false);
  });

  it('chooses a readable foreground for light and dark accents', () => {
    expect(foregroundForAccent('#F6D84A')).toBe('#17100C');
    expect(foregroundForAccent('#273A76')).toBe('#FFFFFF');
  });

  it('uses white on the homepage hotkey tile only for black-adjacent accents', () => {
    expect(foregroundForDarkSurface('#000000')).toBe('#FFFFFF');
    expect(foregroundForDarkSurface('#333333')).toBe('#FFFFFF');
    expect(foregroundForDarkSurface('#4A4644')).toBe('#FFFFFF');
    expect(foregroundForDarkSurface('#4F7FD8')).toBe('#4F7FD8');
    expect(foregroundForDarkSurface('#2D1B69')).toBe('#2D1B69');
  });
});
