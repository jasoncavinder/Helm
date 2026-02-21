# Helm Website Redesign Plan

## Purpose
Define a full website redesign direction for Helm that aligns with:

- `docs/brand/HELM_DESIGN_SYSTEM.md`
- `docs/brand/UI_COMPONENT_SPEC.md`
- `docs/brand/VISUAL_IDENTITY_GUIDE.md`
- `docs/brand/LANDING_COPY_REWRITE.md`
- `docs/brand/TYPOGRAPHY_COLOR_SYSTEM.md`
- `docs/brand/WEBSITE_TYPOGRAPHY_COLOR_SPEC.md`

This plan is design-only. No feature or product-positioning changes are included.

## Brand Alignment Summary
- Tagline remains: "Take the helm."
- Tone remains: commanding, calm, professional, technical.
- Visual priority: Helm Blue is primary.
- Rope Gold is reserved for Pro UI only and kept below 10% visual weight.
- Style direction: high clarity and hierarchy (inspired by Avast/Docker structure), without flashy startup aesthetics.
- Typography authority: headings use Neue Haas Grotesk and body uses Inter per website typography spec.

## 1) Proposed Layout Structure (Section by Section)
## 0. Global Frame
- Max content width: `1200px` (marketing body), with wider hero background treatment.
- Page rhythm: large section spacing (`80-120px` desktop, `56-72px` mobile).
- Consistent section headers with short supporting text.
- Sticky top navigation for orientation and CTA persistence.

## 1. Header / Navigation
- Left: Helm logo + wordmark.
- Center/right: anchor links (`Problem`, `Solution`, `Helm Pro`).
- Right actions:
  - Secondary: `View on GitHub`
  - Primary: `Download Helm`
- Behavior:
  - Minimal chrome, subtle bottom divider.
  - On scroll, add slight panel blur/tint for readability.

## 2. Hero (Primary Value)
- Copy (from rewrite):
  - Eyebrow: `macOS-native toolchain control`
  - H1: `Take the helm.`
  - Supporting line: `Helm centralizes your toolchain and restores operational clarity across package managers.`
- Actions:
  - Primary: `Download Helm`
  - Secondary: `View on GitHub`
- Visual:
  - Product screenshot or UI crop in framed command-deck card.
  - Subtle radial blue glow behind product frame.
  - Optional tiny status chips (deterministic, auditable, centralized) as supporting proof points.

## 3. Problem Section
- Heading: `Modern development environments are fragmented.`
- Body references manager fragmentation (`Homebrew`, `npm`, `pip`, `cargo`, `RubyGems`).
- Layout:
  - Left: concise narrative copy.
  - Right: structured "fragmentation map" card stack (not decorative blobs).
- Goal: show loss of visibility before presenting solution.

## 4. Solution Section
- Heading: `Helm restores control.`
- Content cards for:
  - Unified visibility across managers
  - Centralized upgrade strategy
  - Deterministic system state
  - Clear update history
  - CVE awareness (Helm Pro, visually tagged)
- Layout:
  - Desktop: 2-column card grid (or 3+2 split).
  - Mobile: single-column stack.
- Visual hierarchy:
  - Standard cards use blue-neutral styling.
  - Pro capability card gets subtle gold badge only.

## 5. Command Bridge Section
- Heading: `Think of Helm as your command bridge.`
- Supporting lines:
  - `One interface.`
  - `All managers.`
  - `Total visibility.`
- Layout:
  - Left: concise conceptual copy.
  - Right: structured diagram panel showing manager inputs -> Helm control plane -> clear state.
- Direction:
  - Technical and literal, not metaphor-heavy.

## 6. Helm Pro Section
- Heading: `Helm Pro adds advanced system intelligence.`
- List:
  - CVE detection
  - Priority upgrade recommendations
  - Audit-ready update history
  - Proactive risk insights
- Visual rules:
  - Use gold accents only on Pro badge, border highlights, and Pro CTA.
  - Keep section mostly blue/neutral with premium contrast accents.
- CTA:
  - Secondary Pro-focused action (for future pricing/details route) without changing base product positioning.

## 7. Footer CTA
- Heading: `Take the helm.`
- Supporting text: `Download Helm for macOS.`
- Primary action: `Download Helm`
- Optional secondary: `View on GitHub`

## 8. Footer
- Left: logo + short product line.
- Right: compact links (`Docs`, `GitHub`, `Privacy`, `Support`).
- Subtle legal/meta row.

## 2) Updated Component Hierarchy
```text
WebsitePage
- BackgroundLayer
  - GradientBase
  - GridOverlay (subtle, optional by section)
- SiteHeader
  - BrandLockup
  - NavLinks
  - HeaderActions
    - SecondaryButton (GitHub)
    - PrimaryButton (Download)
- MainContent
  - HeroSection
    - HeroCopy
    - HeroActions
    - ProductFrameCard
  - ProblemSection
    - SectionHeading
    - ProblemNarrative
    - FragmentationCards
  - SolutionSection
    - SectionHeading
    - CapabilityCardGrid
      - CapabilityCard (standard)
      - CapabilityCard (pro-tagged)
  - CommandBridgeSection
    - BridgeCopy
    - BridgeDiagramCard
  - ProSection
    - ProIntro
    - ProFeatureList
    - ProBadge
    - ProAction
  - FooterCTASection
    - CTAHeading
    - CTAActions
- SiteFooter
  - FooterLinks
  - MetaRow
```

Shared primitives:
- `Button` (primary, secondary, tertiary, pro)
- `Card` (standard, highlighted, pro)
- `Badge` (standard, pro)
- `SectionHeader`
- `Divider`
- `ListRow`

## 3) Responsive Strategy
## Breakpoints
- `sm`: 0-639px
- `md`: 640-1023px
- `lg`: 1024-1439px
- `xl`: 1440px+

## Layout Behavior
- `sm`:
  - Single-column flow.
  - Hero screenshot below copy.
  - Collapsible nav menu.
  - Full-width CTAs with clear vertical stacking.
- `md`:
  - Two-column hero where space allows.
  - 2-column capability grid.
  - Slightly denser section spacing than desktop.
- `lg/xl`:
  - Full two-column hero.
  - Problem and Command Bridge use split layouts.
  - Capability cards balance into 3-column or 2+3 composition.

## Type and Spacing Scaling
- H1 uses fluid scale with hard min/max to prevent oversized headline behavior.
- Body and label sizes step by breakpoint, never below readable contrast standards.
- Increase hit areas for touch devices while preserving desktop precision.

## 4) Suggested Visual Enhancements
## Background and Atmosphere
- Base background:
  - Light mode: cool neutral with faint blue lift.
  - Dark mode: deep navy gradient with restrained radial glow.
- Add subtle compass/grid overlay:
  - Very low opacity.
  - Applied to hero and bridge sections only.
  - Must not reduce text contrast.

## Depth and Framing
- Use soft elevation cards for content grouping.
- Frame product visuals like a macOS workspace panel.
- Use thin neutral dividers to keep section boundaries explicit.

## Contrast Discipline
- Keep major CTAs and selected states in Helm Blue.
- Use gold accents only for Pro indicators and Pro CTA.
- Avoid bright multi-hue gradients and decorative color noise.

## 5) Minimal Animation Guidance
- Timing:
  - Standard transitions: `180-240ms`.
  - Ease: `ease-out` for entrances and state changes.
- Allowed motions:
  - Fade + slight upward translate for section reveal.
  - Soft hover elevation on cards/buttons.
  - Nav background transition on scroll.
- Disallowed motions:
  - Bounce, elastic, springy playful effects.
  - Continuous looping ornamentation.
- Accessibility:
  - Respect `prefers-reduced-motion`.
  - In reduced mode, use instant or opacity-only transitions.

## 6) Copy and Hierarchy Guardrails
- Keep rewritten landing copy structure intact.
- Do not reframe Helm as a different product category.
- Prioritize scanability:
  - Clear section headers
  - Short body blocks
  - Strong CTA consistency
- Maintain a professional command-center feel over marketing spectacle.
