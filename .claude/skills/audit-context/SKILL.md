---
name: audit-context
description: Audit what Claude loads while working on Tortuga (CLAUDE.md, skills, settings, memory, the planning docs) for token cost, stale references and duplication, and fix what is clear. Use when the user runs /audit-context or asks to audit rules, skills or docs for token usage.
---

# Audit context

Goal: every token Claude reads while planning or implementing earns its place. Ask the user before anything that
changes a documented structure (splitting a doc, a new rule in CLAUDE.md, a Makefile target); fix stale references,
typos and duplication directly.

## 1. Measure (one call)

```sh
bash .claude/skills/audit-context/measure.sh
```

It prints sizes against the budgets below, every path or symbol named in CLAUDE.md and the skills that no longer
exists, and source files past the ~1000-line rule. Read only what it flags.

| What | Budget | Why |
|---|---|---|
| `CLAUDE.md` | 12 KB | Loaded on every turn. |
| Each `SKILL.md` | 8 KB | Loaded whenever it triggers. |
| A planning doc in `docs/` | 120 KB | Edited on every requirement; must stay greppable by ID. Over budget: propose an archive split. |
| Output of `/verify` that reaches context | ~100 lines | It runs once or more per task. |

## 2. Check by reading (only the flagged parts)

- **Stale:** each flagged reference: fix the name, or drop the line if the thing is gone.
- **Duplicated:** a rule in both `CLAUDE.md` and a skill costs on every turn. Keep it in the skill unless it applies
  outside that skill's area (e.g. the glyph-panic rule also covers `locales/*.ftl`).
- **Instructions that cause big reads:** any "update X" or "check Y" in `CLAUDE.md` or a skill that names a file over
  ~500 lines must say how to find the place (grep by ID or symbol), not imply reading it.
- **Settings:** `.claude/settings.json` allows the read-only and quality commands used routinely (`make test|lint|verify…`,
  read-only `git`). Never allow `make run`, `release-*`, `package`, `upgrade`, or `git push`.
- **Memory** (`~/.claude/projects/-home-nielsp-Projects-tortuga/memory/`): every entry still true; nothing the repo
  already records (code, docs, git history). Project knowledge belongs in the repo, where the team sees it.
- **New skills/docs** since the last audit: listed in `CLAUDE.md` with one line, and have a precise `description`
  so they trigger only when needed.

## 3. Report

A short table: item, size or finding, budget, action taken or proposed. Separate what you fixed from what needs the
user's decision, and ask about the latter. Don't commit unless asked.
