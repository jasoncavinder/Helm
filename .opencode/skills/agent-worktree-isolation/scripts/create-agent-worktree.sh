#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage:
  create-agent-worktree.sh <task-id> <task-branch> <base-ref> [--primary <path>] [--dry-run]

Creates <primary>/.worktrees/<task-id> with a new task branch. Run this from
the primary coordination checkout, not from an existing linked worktree.
USAGE
}

fail() {
  printf 'error: %s\n' "$1" >&2
  exit 1
}

if [ "$#" -lt 3 ]; then
  usage >&2
  exit 64
fi

TASK_ID="$1"
TASK_BRANCH="$2"
BASE_REF="$3"
PRIMARY_PATH=""
DRY_RUN=0
shift 3

while [ "$#" -gt 0 ]; do
  case "$1" in
    --primary)
      [ "$#" -ge 2 ] || fail "--primary requires a path"
      PRIMARY_PATH="$2"
      shift 2
      ;;
    --dry-run)
      DRY_RUN=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      printf 'error: unknown argument: %s\n' "$1" >&2
      usage >&2
      exit 64
      ;;
  esac
done

if [[ ! "$TASK_ID" =~ ^[a-z0-9]+(-[a-z0-9]+)*$ ]]; then
  fail "task ID must be kebab-case"
fi

git check-ref-format --branch "$TASK_BRANCH" >/dev/null 2>&1 || fail "invalid task branch: $TASK_BRANCH"

case "$TASK_BRANCH" in
  main|dev)
    fail "long-lived branch '$TASK_BRANCH' cannot be used as a task branch"
    ;;
  docs|web)
    fail "reserved bare name '$TASK_BRANCH' must use scoped form such as docs/<topic> or web/<topic>"
    ;;
esac

DISCOVERED_PRIMARY=""
while IFS= read -r line; do
  case "$line" in
    "worktree "*)
      DISCOVERED_PRIMARY="${line#worktree }"
      break
      ;;
  esac
done < <(git worktree list --porcelain)

[ -n "$DISCOVERED_PRIMARY" ] || fail "could not discover the primary checkout"
DISCOVERED_PRIMARY="$(cd "$DISCOVERED_PRIMARY" && pwd -P)"

if [ -z "$PRIMARY_PATH" ]; then
  PRIMARY_PATH="$DISCOVERED_PRIMARY"
fi

[ -d "$PRIMARY_PATH" ] || fail "primary checkout does not exist: $PRIMARY_PATH"

PRIMARY_PATH="$(cd "$PRIMARY_PATH" && pwd -P)"
[ "$PRIMARY_PATH" = "$DISCOVERED_PRIMARY" ] || fail "--primary must identify Git's primary checkout: $DISCOVERED_PRIMARY"
PRIMARY_TOPLEVEL="$(git -C "$PRIMARY_PATH" rev-parse --show-toplevel 2>/dev/null)" || fail "primary path is not a Git worktree"
[ "$PRIMARY_TOPLEVEL" = "$PRIMARY_PATH" ] || fail "primary path must be the worktree root: $PRIMARY_PATH"

CURRENT_TOPLEVEL="$(git rev-parse --show-toplevel 2>/dev/null)" || fail "run from a Git worktree"
CURRENT_TOPLEVEL="$(cd "$CURRENT_TOPLEVEL" && pwd -P)"
[ "$CURRENT_TOPLEVEL" = "$PRIMARY_PATH" ] || fail "run from the primary checkout; current worktree is $CURRENT_TOPLEVEL"

WORKTREES_ROOT="$PRIMARY_PATH/.worktrees"
DESTINATION="$WORKTREES_ROOT/$TASK_ID"

[ ! -L "$WORKTREES_ROOT" ] || fail ".worktrees must not be a symbolic link"
[ ! -e "$WORKTREES_ROOT" ] || [ -d "$WORKTREES_ROOT" ] || fail ".worktrees exists but is not a directory"

if [ -e "$DESTINATION" ] || [ -L "$DESTINATION" ]; then
  fail "worktree destination already exists: $DESTINATION"
fi

if git -C "$PRIMARY_PATH" show-ref --verify --quiet "refs/heads/$TASK_BRANCH"; then
  fail "task branch already exists: $TASK_BRANCH"
fi

git -C "$PRIMARY_PATH" rev-parse --verify --quiet --end-of-options "$BASE_REF^{commit}" >/dev/null || fail "base ref does not resolve to a commit: $BASE_REF"

IGNORE_PROBE=".worktrees/.helm-agent-ignore-probe"
git -C "$PRIMARY_PATH" check-ignore --quiet "$IGNORE_PROBE" || fail ".worktrees/ is not ignored in the primary checkout"

if [ "$DRY_RUN" -eq 1 ]; then
  printf '[agent-worktree] validated destination: %s\n' "$DESTINATION"
  printf '[agent-worktree] validated branch: %s (base: %s)\n' "$TASK_BRANCH" "$BASE_REF"
  printf '[agent-worktree] dry run; no worktree created\n'
  exit 0
fi

if [ ! -d "$WORKTREES_ROOT" ]; then
  mkdir "$WORKTREES_ROOT"
fi

git -C "$PRIMARY_PATH" worktree add -b "$TASK_BRANCH" -- "$DESTINATION" "$BASE_REF"
printf '%s\n' "$TASK_ID" > "$DESTINATION/.WORKTREE_ID"

ACTUAL_TOPLEVEL="$(git -C "$DESTINATION" rev-parse --show-toplevel)"
ACTUAL_BRANCH="$(git -C "$DESTINATION" branch --show-current)"
[ "$ACTUAL_TOPLEVEL" = "$DESTINATION" ] || fail "created worktree path verification failed"
[ "$ACTUAL_BRANCH" = "$TASK_BRANCH" ] || fail "created worktree branch verification failed"

printf '[agent-worktree] created: %s\n' "$ACTUAL_TOPLEVEL"
printf '[agent-worktree] branch: %s\n' "$ACTUAL_BRANCH"
printf '[agent-worktree] next: run all task work from this worktree\n'
