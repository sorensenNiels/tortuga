#!/usr/bin/env bash
# Measures what Claude loads while working on Tortuga (see SKILL.md). Prints only what needs a look.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

over() { # file budget_bytes
  local size; size=$(wc -c < "$1")
  if (( size > $2 )); then echo "OVER   $1: $((size / 1024)) KB (budget $(($2 / 1024)) KB)"
  else echo "ok     $1: $((size / 1024)) KB"; fi
}

echo "== Sizes (about 4 bytes per token)"
over CLAUDE.md $((12 * 1024))
for f in .claude/skills/*/SKILL.md; do over "$f" $((8 * 1024)); done
for f in docs/*.md README.md; do over "$f" $((120 * 1024)); done

echo
echo "== References in CLAUDE.md and skills that no longer exist"
files=$(git ls-files; git ls-files --others --exclude-standard)
grep -oh '`[^`]*`' CLAUDE.md .claude/skills/*/SKILL.md | tr -d '`' | sort -u | while read -r ref; do
  case "$ref" in *' '*|*'*'*|*'<'*|*'~'*|*'$'*|*'('*|*'['*|*'#'*|*'"'*|*'|'*|*'{'*) continue ;; esac
  if [[ "$ref" == */* || "$ref" =~ \.(rs|md|toml|ftl|json|sh|yml)$ ]]; then
    [[ -e "$ref" ]] || grep -qF -- "${ref%/}" <<< "$files" || echo "path   $ref"
  elif [[ "$ref" == *::* || "$ref" == *_* || "$ref" =~ ^[A-Z][a-z]+[A-Z] ]]; then
    sym=${ref##*::}
    [[ "$sym" =~ ^[A-Za-z_][A-Za-z0-9_]{3,}$ ]] || continue
    grep -rqw --include='*.rs' --include=Cargo.toml -- "$sym" crates Cargo.toml || echo "symbol $ref"
  fi
done

echo
echo "== Source files over 1000 lines of non-test code (CLAUDE.md: split them)"
for f in $(git ls-files 'crates/*.rs'); do
  awk '/^#\[cfg\(test\)\]/ { exit } { n++ } END { if (n > 1000) print n, FILENAME }' "$f"
done | sort -rn | awk '{ print "       " $2 ": " $1 }'

echo
echo "== Memory"
mem="$HOME/.claude/projects/-home-nielsp-Projects-tortuga/memory"
ls -1 "$mem" 2>/dev/null | grep . || echo "       (none)"
