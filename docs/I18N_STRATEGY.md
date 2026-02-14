# Helm Internationalization (i18n) & Localization (l10n) Strategy

**Purpose:**  
Define how Helm handles localization of UI text, errors, formatting, and UX across all surfaces (macOS app, service, CLI, website).  

This document is written primarily for **AI coding agents** responsible for implementing and maintaining localization.

---

## 1) Core Principles

1. **English is the source of truth**
   - All strings are authored in `en`
   - All other locales derive from `en`

2. **Preserve meaning, not wording**
   - Use *transcreation* where needed
   - Do not force literal translations

3. **Consistency over completeness**
   - Partial, well-implemented locales are acceptable
   - Broken or inconsistent localization is not

4. **Single source of truth**
   - All translatable strings must exist in the `locales/` directory
   - No hardcoded user-facing text

5. **Fail visibly in development**
   - Missing translations must be obvious during development

---

## 2) Terminology

- **Locale:** Language + optional region (e.g., `en`, `pt-BR`, `zh-Hans`)
- **UI Strings:** Visible text in the interface
- **Service Strings:** Errors/messages from backend services
- **Transcreation:** Adapting meaning instead of literal translation

---

## 3) Supported Locales & Expansion Policy

Helm uses a **phased rollout strategy** for languages.

### 3.1 Tier 1 (Required)
- `en` (English)

### 3.2 Tier 2 (Default Shipping Set)
High ROI, low complexity:

- `es` (Spanish)
- `fr` (French)
- `de` (German)
- `pt-BR` (Portuguese - Brazil)
- `ja` (Japanese)

### 3.3 Tier 3 (Expansion)
Added after initial stability:

- `zh-Hans` (Chinese Simplified)
- `ko` (Korean)
- `it` (Italian)
- `nl` (Dutch)

### 3.4 Tier 4 (Advanced / Future)
Requires additional UX or linguistic complexity:

- `hi` (Hindi)
- `ar` (Arabic) — requires RTL support
- `ru` (Russian)
- `id` (Indonesian)
- `tr` (Turkish)

---

### 3.5 Constraints

- Do not add RTL languages (e.g., `ar`) until RTL UI support exists
- New locales must pass:
  - i18n validation
  - UI layout QA
- All locales must follow BCP 47 tags

---

## 4) Localization Architecture

### 4.1 Directory Structure

```

locales/
en/
common.json
app.json
service.json
website.json

es/
common.json
app.json
service.json
website.json

...

```

### 4.2 File Responsibilities

| File | Purpose |
|------|--------|
| common.json | Shared strings (buttons, labels, brand) |
| app.json | macOS UI |
| service.json | Service/XPC errors |
| website.json | Marketing site |

---

### 4.3 Metadata (Required for Ambiguity)

Since JSON does not support comments, use metadata:

```

locales/_meta/en.json

````

Example:

```json
{
  "website.hero.tagline": {
    "meaning": "Imperative command, nautical metaphor, take control",
    "tone": "confident, concise",
    "constraints": "short phrase, max ~20 chars"
  }
}
````

---

## 5) Key Naming Conventions

### 5.1 Format (Required)

Use dot notation:

* `common.button.ok`
* `app.menu.checkUpdates`
* `service.error.permissionDenied`
* `website.hero.tagline`

---

### 5.2 Rules

* Keys must be stable (never renamed casually)
* Keys must not contain English phrasing
* Do not concatenate strings at runtime
* Use placeholders instead

---

### 5.3 ICU Message Format (Required)

Example:

```json
{
  "common.items": "{count, plural, one {# item} other {# items}}"
}
```

---

## 6) Fallback Strategy (Strict)

### 6.1 Resolution Order

Given requested locale `L`:

1. Exact match (`pt-BR`)
2. Language fallback (`pt`)
3. Default locale (`en`)
4. Hard fallback (`en`)

---

### 6.2 Missing Keys

#### Development

* Display: `⟦missing.key⟧`

#### Production

* Fallback to English
* Log missing key

---

## 7) UX Guidelines

### 7.1 Language Selection

* Default to system language
* Provide manual override
* Include:

  * “System Default”
  * Native names (Deutsch, 日本語, Español)

---

### 7.2 Text Expansion

* Expect 30–40% growth
* Avoid fixed widths
* Use dynamic layout

---

### 7.3 Tone

* Clear
* Direct
* Confident
* Not overly technical

---

### 7.4 RTL Support (Future)

* Do not assume LTR
* Avoid “left/right” in strings
* Use leading/trailing constraints

---

## 8) Branding & Tagline Strategy

### 8.1 Product Name

* **Helm is never translated**

---

### 8.2 Tagline

Canonical:

```
brand.tagline.primary = "Take the helm!"
```

---

### 8.3 Localization Policy

* Do not force literal translation
* Use transcreation when needed

---

### 8.4 Recommended Structure

```
brand.tagline.primary
brand.tagline.secondary
```

Example:

**English**

* Take the helm!
* Take control of your system.

**German**

* Take the helm!
* Übernimm die Kontrolle über dein System.

**Japanese**

* Take the helm!
* システムの主導権を握ろう。

```

---

## 9) Formatting (Dates, Numbers, Currency)

### 9.1 Dates

- Store in UTC
- Display in user locale

---

### 9.2 Numbers

- Use locale-aware formatting
- Never build manually

---

### 9.3 Plurals

- Use ICU rules
- Never hardcode singular/plural logic

---

## 10) Service & CLI Localization

### 10.1 Principle

- User-facing messages → localized
- Debug logs → English

---

### 10.2 Error Structure

Return structured errors:

```

{
"error_code": "PERMISSION_DENIED",
"user_message_key": "service.error.permissionDenied",
"recovery_key": "service.recovery.checkPermissions"
}

```

---

## 11) Validation & CI Requirements

### 11.1 Required Checks

1. Key parity vs `en`
2. Placeholder consistency
3. ICU syntax validation
4. Missing translation report

---

### 11.2 CI Jobs

- `i18n:lint`
- `i18n:coverage`
- `i18n:extract`

---

### 11.3 Missing Translation Policy

- Allowed outside `en`
- Must fallback cleanly
- Core flows must be translated

---

## 12) Platform Implementation

### 12.1 macOS (SwiftUI)

- Prefer a central translation wrapper:

```

L10n.t("common.button.ok")

```

- Use environment locale override

---

### 12.2 Rust Core / Service

- Return keys, not strings
- UI layer handles localization

---

### 12.3 Website

- Use ICU-compatible i18n library
- Use URL-based locales (`/es/`)
- Persist user preference

---

## 13) Agent Playbook

When adding a new locale:

1. Copy `en` locale files
2. Translate high-priority flows:
   - onboarding
   - permissions
   - errors
3. Run validation
4. QA UI for overflow
5. Mark missing keys

---

## 14) Do / Don’t Rules

### Do
- Use ICU format
- Provide metadata
- Use fallback correctly
- Keep keys stable

### Don’t
- Concatenate strings
- Hardcode English
- Assume fixed UI width
- Translate "Helm"
