#!/usr/bin/env bash
# Refuses a commit that changes what users see without a line in CHANGELOG.md.
#
# What users see: the crates' src/, locales/ and icons/, and the VS Code extension's src/ and package.json,
# except test files. A commit that changes none of them users can't notice (refactoring, tests, docs) says so
# with a "Changelog: none" line in its message.
#
# Usage:
#   scripts/check-changelog.sh --hook        Claude Code PreToolUse hook (.claude/settings.json): reads the tool
#                                            call as JSON on stdin; exit 2 blocks the `git commit`.
#   scripts/check-changelog.sh MESSAGE_FILE  git commit-msg hook (optional: git config core.hooksPath .githooks).
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

user_facing='^(crates/[^/]+/(src|locales|icons)/|editors/vscode/(src/|package\.json$))'
test_file='(^|/)tests?(/|\.rs$)|_tests?\.rs$|\.test\.ts$'

if [ "${1:-}" = --hook ]; then
    input=$(cat)
    # Only `git commit`; the command is in the JSON as written, so a plain search finds it and the opt-out.
    grep -qE 'git( -C [^ ]+)? commit' <<<"$input" || exit 0
    grep -q 'Changelog: none' <<<"$input" && exit 0
    files=$(git diff --cached --name-only)
    # `git commit -a` (or -am) commits the changed tracked files too.
    if grep -qE 'git commit[^"]*( -[a-zA-Z]*a[a-zA-Z]*| --all)( |\\"|$)' <<<"$input"; then
        files=$(printf '%s\n%s\n' "$files" "$(git diff --name-only)")
    fi
    refuse=2
else
    message=${1:?usage: $0 --hook | MESSAGE_FILE}
    grep -q '^Changelog: none' "$message" && exit 0
    files=$(git diff --cached --name-only)
    refuse=1
fi

grep -qx 'CHANGELOG.md' <<<"$files" && exit 0
changed=$(grep -E "$user_facing" <<<"$files" | grep -vE "$test_file" || true)
[ -z "$changed" ] && exit 0

{
    echo "tortuga: this commit changes what users see but not CHANGELOG.md:"
    sed 's/^/  /' <<<"$changed" | head -10
    echo "Add a line under \"## Unreleased\" in CHANGELOG.md (Added, Changed or Fixed) and stage it."
    echo "If users can't notice the change (refactoring, tests), add a line \"Changelog: none\" to the commit message."
} >&2
exit "$refuse"
