# Helm Website Design Tokens (Proposed)

## Purpose
Define CSS variable tokens for the redesigned Helm website, aligned to the official brand system. This is a proposal for design and implementation planning.

## Token Principles
- Helm Blue is the primary interactive and brand color family.
- Rope Gold is Pro-only accent color.
- Semantic tokens should be consumed by components; raw palette tokens stay foundational.
- Gold usage should remain below 10% of total visual weight on any page.

## 1) Foundation Tokens
```css
:root {
  /* Brand - Helm Blue */
  --color-blue-900: #1b3a66;
  --color-blue-700: #2a5da8;
  --color-blue-500: #3c7dd9;
  --color-blue-300: #6ca6e8;

  /* Accent - Rope Gold (Pro only) */
  --color-gold-700: #a97e2a;
  --color-gold-500: #c89c3d;

  /* Neutrals - light */
  --color-bg-light: #f5f7fa;
  --color-panel-light: #ffffff;
  --color-divider-light: #e2e6ec;
  --color-text-primary-light: #1c1f26;
  --color-text-secondary-light: #4b5563;

  /* Neutrals - dark */
  --color-bg-dark: #0e1624;
  --color-panel-dark: #141e2f;
  --color-divider-dark: #24324a;
  --color-text-primary-dark: #e6edf6;
  --color-text-secondary-dark: #9fb0c7;

  /* Status */
  --color-critical-500: #d64545;

  /* Typography */
  --font-sans: "SF Pro Text", "SF Pro Display", "Inter", "IBM Plex Sans",
    -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
  --font-mono: "SF Mono", "IBM Plex Mono", "SFMono-Regular", Menlo, monospace;

  /* Radius */
  --radius-sm: 12px;
  --radius-md: 14px;
  --radius-lg: 16px;

  /* Elevation */
  --shadow-soft: 0 6px 24px rgba(16, 28, 48, 0.08);
  --shadow-soft-dark: 0 10px 30px rgba(0, 0, 0, 0.28);

  /* Motion */
  --motion-duration-fast: 180ms;
  --motion-duration-base: 220ms;
  --motion-duration-slow: 240ms;
  --motion-ease-standard: ease-out;

  /* Layout */
  --container-max: 1200px;
  --space-1: 4px;
  --space-2: 8px;
  --space-3: 12px;
  --space-4: 16px;
  --space-5: 24px;
  --space-6: 32px;
  --space-7: 48px;
  --space-8: 64px;
  --space-9: 80px;
}
```

## 2) Semantic Tokens
```css
:root,
[data-theme="light"] {
  --color-bg: var(--color-bg-light);
  --color-surface: var(--color-panel-light);
  --color-surface-elevated: #ffffff;
  --color-divider: var(--color-divider-light);
  --color-text-primary: var(--color-text-primary-light);
  --color-text-secondary: var(--color-text-secondary-light);

  --color-brand: var(--color-blue-700);
  --color-brand-hover: var(--color-blue-500);
  --color-brand-active: var(--color-blue-900);
  --color-focus-ring: var(--color-blue-500);

  --color-info-border: var(--color-blue-700);
  --color-info-bg: rgba(42, 93, 168, 0.08);

  --color-warning-border: var(--color-gold-500);
  --color-warning-bg: rgba(200, 156, 61, 0.1);

  --color-critical-border: var(--color-critical-500);
  --color-critical-bg: rgba(214, 69, 69, 0.1);
}

[data-theme="dark"] {
  --color-bg: var(--color-bg-dark);
  --color-surface: var(--color-panel-dark);
  --color-surface-elevated: #19263a;
  --color-divider: var(--color-divider-dark);
  --color-text-primary: var(--color-text-primary-dark);
  --color-text-secondary: var(--color-text-secondary-dark);

  --color-brand: var(--color-blue-500);
  --color-brand-hover: var(--color-blue-300);
  --color-brand-active: var(--color-blue-700);
  --color-focus-ring: var(--color-blue-300);

  --color-info-border: var(--color-blue-500);
  --color-info-bg: rgba(60, 125, 217, 0.16);

  --color-warning-border: var(--color-gold-500);
  --color-warning-bg: rgba(200, 156, 61, 0.16);

  --color-critical-border: var(--color-critical-500);
  --color-critical-bg: rgba(214, 69, 69, 0.16);
}
```

## 3) Component Tokens
```css
:root {
  /* Buttons */
  --btn-primary-bg: var(--color-brand);
  --btn-primary-bg-hover: var(--color-brand-hover);
  --btn-primary-bg-active: var(--color-brand-active);
  --btn-primary-text: #ffffff;
  --btn-primary-radius: var(--radius-md);

  --btn-secondary-bg: transparent;
  --btn-secondary-border: var(--color-blue-500);
  --btn-secondary-text: var(--color-blue-700);
  --btn-secondary-bg-hover: rgba(60, 125, 217, 0.08);

  --btn-tertiary-bg: transparent;
  --btn-tertiary-text: var(--color-text-secondary);

  --btn-pro-bg: var(--color-gold-500);
  --btn-pro-bg-hover: var(--color-gold-700);
  --btn-pro-text: #1c1f26;

  /* Cards */
  --card-bg: var(--color-surface);
  --card-border: var(--color-divider);
  --card-radius: var(--radius-lg);
  --card-shadow: var(--shadow-soft);
  --card-highlight-border: var(--color-brand);
  --card-pro-badge-bg: var(--color-gold-500);

  /* Lists */
  --list-row-padding-y: 14px;
  --list-row-divider: var(--color-divider);
  --list-row-hover-bg: rgba(60, 125, 217, 0.05);
  --list-row-selected-bar: var(--color-brand);

  /* Alerts */
  --alert-info-border: var(--color-info-border);
  --alert-info-bg: var(--color-info-bg);
  --alert-warning-border: var(--color-warning-border);
  --alert-warning-bg: var(--color-warning-bg);
  --alert-critical-border: var(--color-critical-border);
  --alert-critical-bg: var(--color-critical-bg);

  /* Navigation */
  --nav-active-text: var(--color-brand);
  --nav-active-bg: rgba(60, 125, 217, 0.1);

  /* Badges */
  --badge-standard-bg: rgba(75, 85, 99, 0.14);
  --badge-standard-text: var(--color-text-secondary);
  --badge-pro-bg: var(--color-gold-500);
  --badge-pro-text: #1c1f26;
  --badge-ai-bg: var(--color-blue-700);
  --badge-ai-text: #ffffff;
}
```

## 4) Typography Scale Tokens
```css
:root {
  --text-display: clamp(2.25rem, 4vw, 3.5rem);
  --text-h1: clamp(2rem, 3.2vw, 3rem);
  --text-h2: clamp(1.5rem, 2.2vw, 2.25rem);
  --text-h3: clamp(1.25rem, 1.6vw, 1.5rem);
  --text-body-lg: 1.125rem;
  --text-body: 1rem;
  --text-body-sm: 0.9375rem;
  --text-label: 0.8125rem;

  --line-height-tight: 1.2;
  --line-height-body: 1.55;
  --line-height-relaxed: 1.7;
}
```

## 5) Responsive Tokens
```css
:root {
  --breakpoint-sm: 640px;
  --breakpoint-md: 1024px;
  --breakpoint-lg: 1440px;

  --section-space-mobile: 56px;
  --section-space-tablet: 72px;
  --section-space-desktop: 96px;
  --section-space-wide: 120px;
}
```

## 6) Visual Enhancement Tokens
```css
:root {
  /* Subtle atmospheric backgrounds */
  --bg-gradient-light:
    radial-gradient(1200px 600px at 75% -10%, rgba(60, 125, 217, 0.14), transparent 60%),
    linear-gradient(180deg, #f7f9fc 0%, #f5f7fa 100%);

  --bg-gradient-dark:
    radial-gradient(900px 520px at 70% -5%, rgba(60, 125, 217, 0.22), transparent 60%),
    linear-gradient(180deg, #0f1828 0%, #0e1624 100%);

  /* Grid overlay */
  --grid-line-color-light: rgba(42, 93, 168, 0.08);
  --grid-line-color-dark: rgba(159, 176, 199, 0.08);
  --grid-size: 32px;
}
```

## 7) Motion and Accessibility Tokens
```css
:root {
  --anim-reveal-distance: 8px;
  --anim-hover-lift: -2px;
}

@media (prefers-reduced-motion: reduce) {
  :root {
    --motion-duration-fast: 0ms;
    --motion-duration-base: 0ms;
    --motion-duration-slow: 0ms;
    --anim-reveal-distance: 0px;
    --anim-hover-lift: 0px;
  }
}
```

## 8) Usage Rules
- Use semantic/component tokens in UI code, not raw palette values.
- Reserve all `--color-gold-*` tokens for Pro-specific badges, borders, and actions.
- Keep focus states in Helm Blue for clear consistency and accessibility.
- Keep contrast compliant in both themes before visual refinements are approved.
