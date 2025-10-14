# LoTaR UI Design Tokens & Styling Notes

_Last updated: 2025-09-29_

This document captures the baseline design tokens and shared styling conventions introduced in the Vue UI refactor. Treat this as living documentation—extend it as new primitives arrive (themes, typography scales, motion, etc.).

## Token overview

All tokens are exposed as CSS custom properties under `:root` in `view/styles.css`. Light / dark overrides and explicit `html[data-theme]` switches keep the same semantic names so components can remain palette-agnostic.

### Typography

| Token | Description | Default |
| --- | --- | --- |
| `--font-sans` | Primary UI font stack | Inter / system sans stack |
| `--text-xs` | Micro / helper text | `0.75rem` |
| `--text-sm` | Body text | `0.875rem` |
| `--text-md` | Section headings / strong labels | `1rem` |
| `--text-lg` | Page titles | `1.25rem` |
| `--line-tight` | Tight line height (chips, labels) | `1.25` |
| `--line-body` | Standard line height | `1.5` |

### Spacing & radii

| Token | Purpose |
| --- | --- |
| `--space-0` … `--space-8` | Baseline spacing scale (4px → 32px increments) |
| `--radius-sm` | Chips / small controls |
| `--radius-md` | Inputs / buttons |
| `--radius-lg` | Cards / popovers |
| `--radius-xl` | Elevated surfaces |

### Shadows & focus

| Token | Usage |
| --- | --- |
| `--shadow-xs`, `--shadow-sm`, `--shadow-md`, `--shadow-lg` | Depth ramp for chrome → surfaces |
| `--focus-ring` | Default accessible focus outline (accent) |
| `--focus-ring-danger` | Focus outline for destructive actions |
| `--blur-backdrop` | Frosted topbar/backdrop blur strength |

### Palette

Semantic colors wrap raw values so light / dark themes can swap without touching components.

| Token | Meaning |
| --- | --- |
| `--color-bg`, `--color-surface`, `--color-surface-contrast` | Page + raised surfaces |
| `--color-border`, `--color-border-strong` | Subtle vs strong borders |
| `--color-fg`, `--color-muted` | Primary and secondary text |
| `--color-accent`, `--color-accent-strong`, `--color-accent-contrast` | Interactive / CTA palette |
| `--color-danger`, `--color-danger-contrast` | Destructive actions |
| `--color-success`, `--color-warning` | Status and feedback |

Legacy aliases (`--bg`, `--fg`, `--accent`, etc.) are maintained for gradual migration; prefer the `--color-*` names for new work.

## Component baselines

- **Buttons (`.btn`)** — Token-driven padding, weight, and transitions. Variants: `primary`, `danger`, and the new `ghost` style. Focus states rely on `--focus-ring` tokens; overrides should do the same to stay accessible.
- **Inputs / selects (`.input`, `.ui-select`)** — Use the same padding and border radius as buttons for vertical rhythm. `UiSelect` applies the focus ring and sets option colors for both light/dark themes.
- **Cards / popovers (`.card`, `.columns-popover`, `.menu-popover`)** — Share border radius + shadow ramp so surfaces feel consistent.
- **Tables** — `TaskTable` now references the spacing tokens for cell padding, color tokens for hover/focus states, and the focus ring for sortable headers.
- **Loaders & empty states (`UiLoader`, `UiEmptyState`)** — Tokenized spinner, accessible status text, and reusable CTA slots keep loading/error/empty flows consistent across views.

## Accessibility checklist (baseline)

1. **Focus visibility** — Every interactive element should rely on `--focus-ring` or `--focus-ring-danger`. Avoid removing outlines unless a better replacement is supplied.
2. **Color contrast** — When introducing new colors, verify AA contrast (4.5:1) against the relevant background. Accent ≈ 7:1 on light mode, 3.2:1 on dark—acceptable for large text/icons but double-check text on dark surfaces.
3. **Hit targets** — Buttons and interactive chips use at least `32px` height after padding. Maintain that standard for new controls.
4. **Motion** — Keep transitions under `150ms`; avoid large motion without `prefers-reduced-motion` fallbacks.
5. **Semantic structure** — Reuse `.row`, `.col`, `.surface`, `.card` utilities; ensure headings remain ordered (`h1` → `h2` etc.).

## Next steps

- Extend tokens with density (compact / cozy / comfortable) switches in preferences.
- Extract icon sizes & stroke widths into tokens once the icon set lands.
- Add Playwright + Axe smoke checks (tracked under milestone M5.1).

