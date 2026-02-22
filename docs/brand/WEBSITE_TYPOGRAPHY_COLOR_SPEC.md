# Helm Website Typography & Color Spec

This document defines the typography and color system specifically for `helmapp.dev`.

Use this file and `docs/brand/TYPOGRAPHY_COLOR_SYSTEM.md` as the authoritative source for website typography/color decisions.
If a conflict exists, `docs/brand/TYPOGRAPHY_COLOR_SYSTEM.md` takes precedence.

---

## 1. Typeface Usage

### Headings
- Font: Neue Haas Grotesk
- Weights: 600-700
- Tracking: -0.01em for H1
- No uppercase transformation
- Fallback: system-ui, "Helvetica Neue", Helvetica, Arial, sans-serif

### Body Text
- Font: Inter
- Weights: 400 (default), 500 (emphasis), 600 (strong emphasis)

### Code Elements
- Font: SF Mono, monospace

---

## 2. Font Scale

H1:
- Size: 48-56px
- Line Height: 1.05
- Weight: 700

H2:
- Size: 32-36px
- Line Height: 1.2
- Weight: 600

H3:
- Size: 22-24px
- Weight: 500

Body Large:
- 18px
- Line Height: 1.6

Body:
- 16px
- Line Height: 1.65

Small:
- 14px

---

## 3. Color Tokens

```css
:root {
  --helm-blue-900: #1B3A66;
  --helm-blue-700: #2A5DA8;
  --helm-blue-500: #3C7DD9;
  --helm-blue-300: #6CA6E8;

  --helm-gold-500: #C89C3D;
  --helm-gold-700: #A97E2A;

  --bg-light: #F5F7FA;
  --panel-light: #FFFFFF;
  --divider-light: #E2E6EC;
  --text-primary: #1C1F26;
  --text-secondary: #4B5563;

  --bg-dark: #0E1624;
  --panel-dark: #141E2F;
  --divider-dark: #24324A;
  --text-dark-primary: #E6EDF6;
  --text-dark-secondary: #9FB0C7;
}
```

---

## 4. Heading Colors

Light Mode:
- H1: `var(--helm-blue-900)`
- H2: `var(--helm-blue-700)`

Dark Mode:
- H1: `#FFFFFF`
- H2: `var(--helm-blue-300)`

---

## 5. Gold Usage

Gold must only be used for:
- Pro badges
- Premium highlight words
- Accent lines

Do not:
- Use gold for primary headings
- Use gold for large body sections

Gold should stay below 10% visual weight per page.

---

## 6. Layout Rhythm

Use 8pt spacing grid.

Recommended spacing:
- 40px above major sections
- 24px between blocks
- 16px between related elements

Generous whitespace is required.
No cramped layouts.

---

## 7. Visual Tone

The website must feel:
- Calm
- Structured
- Professional
- Technical
- macOS-native compatible

Avoid:
- Neon gradients
- Playful blobs
- Startup-style hero effects

---

## 8. Token Mapping Expectations

`docs/website/DESIGN_TOKENS.md` should expose:
- `--font-heading` for display/headings
- `--font-body` for body/interface text
- `--font-mono` for code elements

`--font-sans` can alias `--font-body` for compatibility, but headings should consume `--font-heading`.

---

## 9. Procurement Reminder

Before production use of Neue Haas Grotesk on hosted website surfaces:
- Purchase the required commercial webfont license.
- Confirm allowed usage scope (domains, traffic limits, self-hosting/CDN terms).
- Keep fallback stacks active until procurement and deployment are complete.
