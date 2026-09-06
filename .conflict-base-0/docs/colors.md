# Verenu Color Palette

Verenu uses neutral gray surfaces and text with restrained accent color usage. Light mode uses near-white grays and dark mode uses charcoal grays. The accent never tints the page backgrounds.

## Neutral surfaces

| Token | Light | Dark | Usage |
|---|---:|---:|---|
| `--sidebar-bg` | `#fbfbf9` | `#10100f` | Sidebar and navigation rail |
| `--paper` | `#fcfcfa` | `#161514` | Main page background |
| `--paper-2` | `#f5f4f2` | `#1d1b1a` | Secondary surfaces |
| `--paper-3` | `#e6e4e0` | `#292726` | Tertiary and active surfaces |
| `--bg-elev` | `#fffffe` | `#1c1b1a` | Elevated cards and menus |

The sidebar and page colors intentionally differ by only a few RGB points. The divider supplies structure without turning settings into a collection of cards.

## Text and lines

| Token | Light | Dark | Usage |
|---|---:|---:|---|
| `--ink` | `#111110` | `#f5f4f0` | Primary text |
| `--ink-strong` | `#252424` | `#fbfaf6` | Strong labels |
| `--ink-soft` | `#4a4947` | `#d2cfc8` | Secondary text |
| `--ink-mute` | `#757471` | `#9c9993` | Descriptions and inactive items |
| `--ink-faint` | `#9b9a96` | `#706d69` | Low-priority text |
| `--line` | `#e3e2e0` | `#2b2928` | Standard divider |
| `--line-soft` | `#ededea` | `#242321` | Subtle divider |
| `--line-strong` | `#d3d2cf` | `#3d3a38` | Strong border |

## Accent

Terracotta is the default accent and is used sparingly for primary actions, selection indicators, focus rings, and status details. Users can replace it in **Settings -> General -> Accent color**. A custom color updates the semantic accent tokens and the full Japonica scale in both themes, while the neutral surfaces stay unchanged.

| Token | Light default | Usage |
|---|---:|---|
| `--accent` | `#d97757` | Primary accent |
| `--accent-ink` | `#a3352b` | Accent text |
| `--accent-soft` | `#f8e6dc` | Soft accent background |

Custom accents are stored as a six-digit hex color. The app derives readable soft, ink, and foreground variants against the active light or dark theme. Resetting the picker removes the override and restores the theme-specific Terracotta defaults.

## Typography

- Serif: Fraunces
- Sans: Inter Tight
- Mono: JetBrains Mono

## Design principles

1. Neutral near-white and charcoal surfaces create structure without reading as cream, beige, orange, or brown.
2. Accent color is reserved for important actions and feedback.
3. Text size, weight, and muted color establish hierarchy before borders do.
4. Sidebar and content surfaces remain subtly distinct in both themes.
5. Semantic tokens are the source of truth for component colors.
