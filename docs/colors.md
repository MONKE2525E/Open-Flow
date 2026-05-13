# Open Flow Color Palette

The Open Flow design system uses a carefully curated palette of warm, earthy tones with minimal accent color usage.

## Color System

### Soft-Amber — Paper & Surfaces
Used for background and elevated surfaces to create a warm, paper-like aesthetic.

| Name | Hex | Usage |
|------|-----|-------|
| `--amber-50` | `#f9f7f3` | Primary paper background |
| `--amber-100` | `#f1ebe3` | Secondary surfaces |
| `--amber-200` | `#d8c9b5` | Tertiary backgrounds |

### Japonica — Accent Color
Warm terracotta accent used sparingly for primary actions, highlights, and interactive states.

| Name | Hex | Usage |
|------|-----|-------|
| `--jap-50` | `#fcf4f0` | Softest accent background |
| `--jap-100` | `#f8e6dc` | Soft accent surface |
| `--jap-200` | `#f0cbb8` | Light accent |
| `--jap-300` | `#e6a78b` | Medium-light accent |
| `--jap-400` | `#d97757` | Primary accent color |
| `--jap-600` | `#c44632` | Strong accent |
| `--jap-700` | `#a3352b` | Darkest accent (text on light backgrounds) |

### Armadillo — Text & Lines
Neutral, warm-dark palette for typography and dividers.

| Name | Hex | Usage |
|------|-----|-------|
| `--arm-200` | `#e8e5e3` | Subtle lines |
| `--arm-300` | `#d8d3cf` | Soft lines |
| `--arm-400` | `#ada299` | Faint text |
| `--arm-500` | `#7e7266` | Muted text |
| `--arm-600` | `#5b554a` | Secondary text |
| `--arm-700` | `#4a433a` | Soft text |
| `--arm-800` | `#2b2422` | Strong text |
| `--arm-900` | `#1e1915` | Dark text |
| `--arm-950` | `#0d0a08` | Primary text (near black) |

## Semantic Tokens

### Surfaces
- `--paper`: `var(--amber-50)` — Main background
- `--paper-2`: `var(--amber-100)` — Secondary background
- `--bg-elev`: `#ffffff` — Elevated surfaces (modals, cards)

### Typography
- `--ink`: `var(--arm-950)` — Primary text
- `--ink-strong`: `var(--arm-800)` — Strong text
- `--ink-soft`: `var(--arm-700)` — Soft text
- `--ink-mute`: `var(--arm-500)` — Muted text
- `--ink-faint`: `var(--arm-400)` — Faint text

### Lines & Borders
- `--line`: `var(--arm-200)` — Standard border
- `--line-soft`: `#efeae3` — Soft border
- `--line-strong`: `var(--arm-300)` — Strong border

### Accent
- `--accent`: `var(--jap-400)` — Primary accent (#d97757)
- `--accent-ink`: `var(--jap-700)` — Accent text (#a3352b)
- `--accent-soft`: `var(--jap-100)` — Soft accent background (#f8e6dc)

## Typography

- **Serif**: Fraunces (400, 500, 600 weights)
- **Sans**: Inter Tight (400, 450, 500, 600 weights)
- **Mono**: JetBrains Mono (400, 500 weights)

## Accent Themes

The design supports multiple accent color schemes that can be toggled via settings:

### Terracotta (Default)
- Primary: `oklch(0.62 0.14 40)`
- Soft: `oklch(0.94 0.03 40)`
- Ink: `oklch(0.42 0.12 40)`

### Moss
- Primary: `oklch(0.55 0.1 145)`
- Soft: `oklch(0.94 0.03 145)`
- Ink: `oklch(0.4 0.1 145)`

### Slate
- Primary: `oklch(0.45 0.04 250)`
- Soft: `oklch(0.94 0.015 250)`
- Ink: `oklch(0.35 0.05 250)`

### Ink
- Primary: `oklch(0.18 0.01 60)`
- Soft: `oklch(0.92 0.005 70)`
- Ink: `oklch(0.18 0.01 60)`

## Design Principles

1. **Warm & Welcoming**: The amber-based paper background creates a warm, natural aesthetic
2. **Minimal Accent**: The terracotta accent is used sparingly for primary actions and important UI elements
3. **Typographic Hierarchy**: Fraunces serif for headings, Inter Tight sans for body text
4. **High Contrast**: Text colors are carefully chosen for accessibility while maintaining the warm tone
5. **Flexible Theming**: Multiple accent options allow users to customize the app's appearance
