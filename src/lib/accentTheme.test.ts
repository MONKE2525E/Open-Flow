import { describe, expect, it } from 'vitest';
import { foregroundForAccent, normalizeAccentColor } from './accentTheme';

describe('accent theme', () => {
  it('normalizes valid six-digit colors', () => {
    expect(normalizeAccentColor(' #4f7fd8 ')).toBe('#4F7FD8');
    expect(normalizeAccentColor('#abc')).toBeNull();
    expect(normalizeAccentColor('orange')).toBeNull();
  });

  it('chooses a readable foreground for light and dark accents', () => {
    expect(foregroundForAccent('#F6D84A')).toBe('#17100C');
    expect(foregroundForAccent('#273A76')).toBe('#FFFFFF');
  });
});
