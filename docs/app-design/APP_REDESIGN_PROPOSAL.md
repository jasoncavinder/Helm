# Helm App Redesign Proposal

## Scope and Constraints

This proposal refines visual design only. It does not re-architect layout, flow, or information architecture.

Constraints honored:
- No major layout changes
- Preserve current UX patterns and interaction model
- Align with `docs/brand/HELM_DESIGN_SYSTEM.md`, `docs/brand/UI_COMPONENT_SPEC.md`, and `docs/brand/VISUAL_IDENTITY_GUIDE.md`
- Maintain accessibility and macOS-native feel

Design intent:
- Deterministic
- Calm
- Professional
- Native to macOS

## Current Baseline (Observed)

Current implementation strengths:
- Strong three-region structure and operational hierarchy in Control Center
- Clear action surfaces for refresh, run plan, and diagnostics
- Good use of concise status chips and inspector context

Current visual inconsistencies to resolve:
- Primary CTA style uses orange/red gradient (`apps/macos-ui/Helm/Views/Components/HelmButtonStyles.swift`) instead of Helm Blue hierarchy
- Radius/spacing/elevation values drift across components (8/10/12/14/16 used inconsistently)
- Card surfaces mix `.thinMaterial`, `.ultraThinMaterial`, and ad hoc fills without a strict depth scale
- Pro visual language is not consistently distinct from standard controls
- Dark mode reads as generic dark material rather than a cohesive naval command deck

## Visual Direction

Use the existing structure, but tighten visual hierarchy:
- Borrow Docker-like structural clarity (stable nav + focused workspace)
- Borrow Avast-like CTA decisiveness (single clear primary action)
- Keep Helm identity: restrained blue command surface, sparse gold premium accents, no flashy gradients

## Token Adjustments

## 1. Color Hierarchy

### Semantic Color Tokens (Proposed)

| Token | Light | Dark | Usage |
|---|---|---|---|
| `color.surface.base` | `#F5F7FA` | `#0E1624` | App/window base |
| `color.surface.panel` | `#FFFFFF` | `#141E2F` | Primary cards/panels |
| `color.surface.elevated` | `#FFFFFF` + subtle tint | `#18263A` | Hovered/selected cards, overlays |
| `color.border.subtle` | `#E2E6EC` | `#24324A` | Dividers and card borders |
| `color.text.primary` | `#1C1F26` | `#E6EDF6` | Primary text |
| `color.text.secondary` | `#4B5563` | `#9FB0C7` | Secondary/meta text |
| `color.action.primary.default` | `#2A5DA8` | `#3C7DD9` | Primary buttons |
| `color.action.primary.hover` | `#3C7DD9` | `#6CA6E8` | Hover |
| `color.action.primary.pressed` | `#1B3A66` | `#2A5DA8` | Pressed |
| `color.action.secondary.border` | `#3C7DD9` | `#6CA6E8` | Secondary button border |
| `color.selection.bg` | `rgba(60,125,217,0.12)` | `rgba(108,166,232,0.16)` | Row/card selection |
| `color.pro.accent` | `#C89C3D` | `#C89C3D` | Pro surfaces/badges (<=10% weight) |

### Before/After Reasoning
- Before: primary actions visually compete with warning/action orange tones.
- After: Helm Blue is the single action hierarchy, with gold reserved for Pro and priority emphasis.

## 2. Spacing, Radius, and Elevation

### Spatial and Shape Tokens (Proposed)

| Token | Value |
|---|---|
| `space.1` | 4 |
| `space.2` | 8 |
| `space.3` | 12 |
| `space.4` | 16 |
| `space.5` | 24 |
| `space.6` | 32 |
| `radius.control` | 12 |
| `radius.card` | 16 |
| `radius.chip` | 8 |
| `elevation.1` | subtle shadow + 1px border |
| `elevation.2` | elevated shadow for sheets/overlays only |

### Elevation Rules
- Default cards use `panel + border`, not always material blur.
- Use material only where contextual depth is required (popover/sheets), not as default card fill.
- Keep shadows soft and low-contrast.

### Before/After Reasoning
- Before: mixed radii and material usage cause uneven density perception.
- After: consistent geometry and depth cues improve scan speed and perceived quality.

## 3. Button and Card Standardization

### Button Variants
- Primary: Helm Blue fill, white text, `radius.control`, deterministic hover/pressed states.
- Secondary: transparent/panel fill, blue border/text, soft blue hover tint.
- Tertiary: text-only muted action for inline operations.
- Pro Button: Gold fill only for explicit Pro actions (upgrade, entitlement prompts).

### Card Variants
- Standard Card: panel background + subtle border.
- Highlighted Card: standard card + blue border accent.
- Pro Card: standard card + gold corner/badge marker + slight elevation bump.

### Before/After Reasoning
- Before: button semantics are clear functionally but inconsistent visually.
- After: buttons and cards communicate action importance and state consistently at a glance.

## 4. Command-Bridge Aesthetic (Subtle)

Introduce restrained operational styling without gimmicks:
- Add a thin "status rail" treatment in top bar/section headers (1px tinted divider line).
- Use monospaced numerics and compact metadata labels for counts, versions, and task timing.
- In dark mode only, allow a very subtle directional panel tint shift (not a visible gradient treatment).
- Maintain SF Symbols and avoid decorative iconography.

### Before/After Reasoning
- Before: strong functional surfaces, limited cohesive "bridge" identity.
- After: stronger operational character while staying calm and macOS-native.

## 5. Helm Pro Distinction

Pro should feel premium, not separate:
- Keep global palette unchanged.
- Apply gold accent only to:
  - Pro badge/chip
  - Pro-only CTA surfaces
  - Premium card marker
- Add slight elevation and tighter border contrast on Pro cards.
- Avoid full-panel gold tints and avoid parallel "theme mode."

### Before/After Reasoning
- Before: Pro context is detectable but not visually systematic.
- After: Pro affordances become immediately recognizable without overwhelming standard workflows.

## 6. Dark Mode as Naval Control Deck

Dark mode refinements:
- Anchor base at `#0E1624` with layered panel hierarchy (`#141E2F`, `#18263A`)
- Increase divider clarity (`#24324A`) for deterministic boundaries
- Strengthen focus/selection outlines with Blue 300/500 combinations
- Keep text luminance stable and reduce haze from overused material fills

### Before/After Reasoning
- Before: dark mode uses quality materials but lacks a coherent deck-like hierarchy.
- After: stronger layer separation and instrument-like clarity under low visual noise.

## Component-Level Suggestions (Implementation Map)

## Priority 1: Tokens and Core Components

1. `apps/macos-ui/Helm/Views/Components/HelmButtonStyles.swift`
- Replace orange/red primary gradient with Helm Blue state tokens.
- Add explicit `primary`, `secondary`, `tertiary`, and `pro` style variants.
- Normalize corner radius to `radius.control`.

2. `apps/macos-ui/Helm/Views/ControlCenterSectionViews.swift`
- Convert default card backgrounds from blanket `.thinMaterial` to panel + border tokens.
- Keep material for overlays/ephemeral blocks only.
- Apply unified card radius and spacing scale.

3. `apps/macos-ui/Helm/Views/ControlCenterViews.swift`
- Tighten top bar hierarchy: stable panel base, clearer search field contrast, subtle status rail.
- Keep existing layout and sizing unchanged.

## Priority 2: Lists and Detail Surfaces

4. `apps/macos-ui/Helm/Views/Components/PackageRowView.swift`
- Standardize selected row background/border token usage.
- Align manager chip styling with neutral chip tokens.
- Reserve orange/gold for actual warning/pro contexts only.

5. `apps/macos-ui/Helm/Views/Components/ManagerItemView.swift`
- Move status indicator colors to semantic state tokens.
- Align manager tile radius and fill with card system.

6. `apps/macos-ui/Helm/Views/SettingsPopoverView.swift`
- Apply consistent card elevation/radius and button hierarchy.
- Introduce explicit Pro badge/button treatment for future monetization surfaces.

## Priority 3: Supporting Surfaces

7. `apps/macos-ui/Helm/Views/PopoverHelpers.swift`
- Harmonize popover/pill shadow and radius with elevation tokens.
- Preserve responsive triage behavior; visual-only adjustments.

8. `apps/macos-ui/Helm/Views/DashboardView.swift`
- Align legacy dashboard cards with tokenized panel/border/elevation rules.

## Before/After Summary by Goal

1. Refine color hierarchy
- Before: mixed accent logic (orange primary CTA, accent-color selection, ad hoc warning tones).
- After: deterministic blue action hierarchy with semantic states and constrained gold Pro accents.

2. Improve spacing and elevation consistency
- Before: per-view radius/padding/material drift.
- After: shared spacing/radius/elevation scale and predictable surface depth.

3. Standardize button and card styling
- Before: multiple visual button idioms and card fill types.
- After: four button variants and three card variants mapped to clear semantics.

4. Introduce subtle command-bridge aesthetic
- Before: functional but visually generic control surfaces.
- After: restrained instrumentation cues (status rail, metadata typography, structured dividers).

5. Improve visual distinction for Helm Pro
- Before: Pro emphasis is present but not systematic.
- After: repeatable Pro markers (gold badge/CTA/edge treatment) with strict visual weight limits.

6. Refine dark mode to naval control deck
- Before: quality dark appearance but soft hierarchy.
- After: layered dark surfaces and crisp boundaries for "control deck" confidence.

## Accessibility and Quality Guardrails

- Preserve minimum WCAG contrast targets (4.5:1 body text, 3:1 large text and UI boundaries where applicable).
- Do not rely on color alone for status; keep icon + text pairing.
- Respect Reduce Motion by keeping transitions opacity/position-light and 180-240ms ease-out.
- Maintain keyboard and VoiceOver semantics already in place.

## Implementation Notes

- This is an incremental visual refactor, not a layout redesign.
- Recommended sequence: token layer first, then button/card standardization, then per-section polish.
- Validate on both light and dark appearances in Control Center and popover before rollout.
