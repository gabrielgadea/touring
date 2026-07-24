# Brand Color Tokens

> Visual identity tokens for Touring (Premium Elite product, 2026-06-04).

## Primary palette

| Token | Hex | RGB | Use |
|-------|-----|-----|-----|
| `--touring-blue` | `#0A2540` | rgb(10, 37, 64) | Primary brand color; headings, links, primary buttons |
| `--harness-green` | `#10B981` | rgb(16, 185, 129) | Accent for success states, "touring on" indicators, validations |
| `--touring-navy` | `#061A2D` | rgb(6, 26, 45) | Deep background, dark-mode primary |
| `--harness-amber` | `#F59E0B` | rgb(245, 158, 11) | Warning states, deprecation notices, beta features |

## Neutral palette

| Token | Hex | Use |
|-------|-----|-----|
| `--neutral-50` | `#F9FAFB` | Lightest background |
| `--neutral-100` | `#F3F4F6` | Code-block background |
| `--neutral-300` | `#D1D5DB` | Borders, dividers |
| `--neutral-500` | `#6B7280` | Secondary text, captions |
| `--neutral-700` | `#374151` | Primary body text |
| `--neutral-900` | `#111827` | Headings, strong text |

## Tier palette (commercial)

| Tier | Color | Token |
|------|-------|-------|
| Free | `#6B7280` (neutral-500) | `--tier-free` |
| Standard | `#3B82F6` (blue-500) | `--tier-standard` |
| Premium | `#8B5CF6` (purple-500) | `--tier-premium` |
| Enterprise | `#0A2540` (touring-blue) | `--tier-enterprise` |

## Typography

- **UI / body**: Inter (sans-serif)
- **Code**: JetBrains Mono
- **Long-form docs**: IBM Plex Serif
- **Logo wordmark**: Inter Bold, tracking +50

## Iconography

- 16x16 + 32x32 + 64x64 + 256x256 + 1024x1024 PNG (raster fallback)
- 1 SVG source (for all sizes; raster is generated)
- License: CC-BY-SA 4.0

## Status

| Asset | Status | Path |
|-------|--------|------|
| Wordmark (text) | ✅ done | `assets/brand/banner.txt` (CLI version) |
| Logo SVG | 🔜 planned (W1.3) | `assets/brand/logo.svg` |
| Color tokens | ✅ done | `assets/brand/color-tokens.md` (this file) |
| Landing page | ✅ done | `docs/landing/index.md` |
| Root README | ✅ done | `README.md` |

## Usage rules

1. **Touring-blue is the primary**. Use it for headings, links, primary
   buttons. Reserve harness-green for success / on states.
2. **Tier colors are sacrosanct**. Don't use `--tier-premium` purple for
   a non-premium feature.
3. **Dark mode is default**. Tour is code-native; engineers live in
   dark mode.
4. **Inter is the only UI font**. No system fallbacks (browsers will
   fall back to sans-serif which is fine).

---

_Generated 2026-06-04 by the TACO orchestrator (W1.3 of the upgrade plan)._
