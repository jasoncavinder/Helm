# Helm Visual System

## Visual Direction

Calm operational clarity:
- Neutral base surfaces.
- Sparse but meaningful color accents tied to system state.
- Strong typographic hierarchy to reduce scanning time.

## Layout System

Structure:
- Menu bar popover width: 360-420 pt
- Control window min size: 980 x 640
- Three-region window layout: sidebar, content, inspector

Spacing scale:
- 4, 8, 12, 16, 24, 32 pt
- Primary card padding: 16 pt
- Section gaps: 24 pt

Density modes:
- Comfortable default
- Compact optional mode for high-package environments

## Typography

Font families:
- SF Pro Text for UI copy
- SF Pro Display for section titles
- Monospaced digits for versions/counts and task timings

Type scale:
- Title: 24 semibold
- Section: 17 semibold
- Body: 13 regular
- Meta: 11 regular

## Color Usage

Semantic tokens (light/dark adaptive):
- `surface.base`
- `surface.elevated`
- `text.primary`
- `text.secondary`
- `state.healthy`
- `state.attention`
- `state.error`
- `state.running`

Rules:
- Color communicates state, not decoration.
- Never rely on color alone; pair with icon/text.
- Warning/error backgrounds use subtle tint and high-contrast text.

## Iconography

- SF Symbols only for consistency and accessibility.
- Manager icons optional in list rows; symbols remain canonical fallback.
- Status symbols are fixed across surfaces:
- healthy: checkmark.circle.fill
- attention: exclamationmark.triangle.fill
- error: xmark.octagon.fill
- running: arrow.triangle.2.circlepath

## Motion and Animation

Principles:
- Motion explains change, not style.
- Keep durations short (120-220 ms).

Patterns:
- Crossfade for data refresh updates.
- Height expansion for detail disclosure.
- Subtle pulse for active tasks.

Accessibility:
- Respect Reduce Motion by replacing transforms with opacity-only transitions.

## Accessibility Baselines

- Keyboard-first navigation across popover and window.
- VoiceOver labels for all state chips and action buttons.
- Minimum 4.5:1 contrast for text on semantic surfaces.
- Dynamic type support with wrapping/truncation priorities.
- Focus rings clearly visible in both light and dark appearances.
