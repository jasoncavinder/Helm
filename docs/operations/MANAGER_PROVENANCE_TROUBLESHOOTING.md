# Manager Provenance Troubleshooting

This guide covers how to inspect and troubleshoot manager-install provenance decisions (starting with `rustup` in the current rollout).

## 1. Inspect Current Provenance

Use CLI inspection first:

```bash
helm managers list
helm managers show rustup
helm managers instances rustup
helm managers instances rustup --json
```

Key fields:

- `provenance`: selected install origin (`homebrew`, `rustup_init`, `unknown`, etc.)
- `confidence`: score for the selected provenance
- `decision_margin`: score delta between top and competing provenance (when present)
- `competing_provenance` + `competing_confidence`: runner-up classification
- `explanation_primary` / `explanation_secondary`: top evidence reasons used by scoring

## 2. Confidence + Margin Interpretation

Use both values together:

- High confidence + high margin:
  - low ambiguity, safe for automation policies where allowed.
- High confidence + low margin:
  - close race; keep confirmation safeguards for mutating actions.
- Low confidence or `unknown`:
  - ambiguous environment; interactive or read-only posture is expected.

## 3. External Evidence Behavior

External ownership checks (for example `brew --prefix rustup`) are intentionally:

- timeout-bounded,
- lazy (only in ambiguity cases),
- cached per detection run,
- optional (detection fails closed if probes fail).

If `brew` is unhealthy/hung, provenance detection should still complete with conservative outputs (`unknown`/lower confidence) instead of blocking the pipeline.

## 4. Log Signals

Bounded external probe logging emits:

- probe start (`starting bounded external provenance probe`)
- probe success (`external provenance probe completed successfully`)
- probe non-zero exit (`external provenance probe exited non-zero`)
- probe timeout kill (`external provenance probe timed out and was terminated`)
- cache behavior (`using cached external provenance evidence`)

These signals are intended for local diagnostics; they should explain why external evidence was or was not used.

## 5. Misclassification Playbook

1. Run `helm managers instances <manager> --json` and capture:
   - selected provenance + confidence + decision margin
   - competing provenance (if present)
   - explanation factors.
2. Check executable/canonical paths for mixed ownership patterns (Homebrew + user-local bins, symlink chains, custom prefixes).
3. Validate package-manager health independently (for example `brew --prefix rustup`) to confirm probe availability.
4. If confidence remains low/ambiguous, treat the instance as `Unknown` and use interactive workflows.
5. For reproducible false positives/negatives, add fixture-driven tests before changing scoring weights.

## 6. Rollout Safety Reminder

Current rollout intentionally preserves legacy compatibility:

- single-path detection fields remain active;
- non-rustup adapters default to `Unknown` with explicit TODO markers;
- uninstall routing must not switch to provenance-first until rollout gates pass.

## 7. Install-Instance Identity Continuity

Identity precedence for `manager_install_instances` is deterministic:

1. `DevInode` (`dev:ino`) when available (Unix).
2. `CanonicalPath` when inode metadata is unavailable.
3. `FallbackHash` (`path + size + mtime`) as a last resort.

Operational notes:

- Alias-path changes should not create new instances when `DevInode` or canonical-path identity is available.
- `FallbackHash` is intentionally conservative and may change when file metadata changes.
- If continuity resets while using `FallbackHash`, treat that as expected ambiguity and favor interactive workflows.
