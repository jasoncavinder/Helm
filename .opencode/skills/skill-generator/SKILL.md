---
name: skill-generator
description: Convert a repeated workflow or conversation process into a reusable agent skill using a required WORKFLOW SPEC confirmation stage. Deprecated: use manual skill creation instead.
license: MIT
compatibility: opencode
metadata:
  audience: maintainers
  workflow: skill-generation
  status: deprecated
---

# skill-generator (Deprecated)

> **Deprecated:** This skill is retained for reference only. New skills should be created manually under `.opencode/skills/` with the standard SKILL.md format. The spec-driven generation workflow is no longer actively maintained.

## Purpose

Convert repeated Helm workflows into reusable skills under `.opencode/skills/` through a spec-first process: workflow -> WORKFLOW SPEC -> generated skill.

## When this Skill should trigger

Trigger when user intent indicates workflow reuse, including phrases like:

- "Convert this workflow into a skill"
- "Create a skill for this"
- "Automate this workflow"
- "Make this workflow reusable"

Also trigger when the agent detects repeated commands, long checklists, fragile sequences, CI-like loops, or packaging/release process repetition.

## Inputs required

- workflow conversation or procedure to convert
- extracted ordered steps
- desired skill name (kebab-case)
- purpose/outputs/safety constraints
- whether optional `scripts/` or `resources/` are needed

## Outputs generated

- a confirmed WORKFLOW SPEC
- `.opencode/skills/<skill-name>/SKILL.md`
- `.opencode/skills/<skill-name>/scripts/` (if needed)
- `.opencode/skills/<skill-name>/resources/` (if needed)

## Safety rules

- refuse skills that include secrets/credentials handling
- refuse skills that publish releases or appcasts automatically
- refuse destructive operations without explicit confirmation safeguards
- use kebab-case skill names and confirm final name before generation
- do not overwrite existing skills without explicit confirmation

## Skill generation process

1. Extract workflow steps from the conversation/process.
2. Produce a WORKFLOW SPEC in the required format.
3. Show the spec to the user and confirm skill name + scope.
4. Generate `.opencode/skills/<skill-name>/SKILL.md` from the spec.
5. Create optional `scripts/` and `resources/` directories when requested.

## WORKFLOW SPEC format (required)

```text
WORKFLOW SPEC

Name:
<kebab-case skill name>

Purpose:
One or two sentence description.

Inputs:
List required inputs.

Outputs:
List outputs.

Steps:
Ordered list of workflow steps.

Safety Constraints:
Important safety rules.

Optional Scripts:
Whether scripts are needed.
```

Optional extension:

- `Optional Resources:` may be included when `resources/` should be scaffolded.

## Execution helper

Use the generator script with a spec file:

- `.opencode/skills/skill-generator/scripts/create_skill.sh --spec <workflow-spec.md> --confirm-name`

Useful options:

- `--init-spec <path>` to create a spec scaffold
- `--dry-run` to render without writing files
- `--force` to overwrite existing `SKILL.md` only with explicit intent

If a generated skill exceeds ~25 lines of steps, the skill-generator should recommend splitting it into multiple smaller skills.
