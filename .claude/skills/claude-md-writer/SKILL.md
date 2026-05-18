---
name: claude-md-writer
description: Write, audit, or restructure CLAUDE.md files that give Claude Code the right context for a codebase. Use whenever the user wants to create a new CLAUDE.md, rewrite or improve an existing one, add per-app CLAUDE.md files to a monorepo, or split a bloated root CLAUDE.md into smaller files per sub-project. Triggers on phrases like "write a CLAUDE.md", "improve my CLAUDE.md", "split the CLAUDE.md", "CLAUDE.md for the monorepo", "set up CLAUDE.md for each app", or "add context files for sub-projects". Also use proactively when starting work in a repo with no CLAUDE.md or an obviously stale/generic one — a good CLAUDE.md pays for itself within a session.
---

# claude-md-writer

A skill for producing CLAUDE.md files that are genuinely useful to Claude Code — concise, specific, action-oriented, and grounded in what's actually in the codebase. Also covers splitting a monorepo's context into a root file plus per-app files.

## Why this matters

CLAUDE.md is loaded into context for every action Claude Code takes in that directory tree. Every line costs tokens, displaces other context, and gets re-read constantly. A bad CLAUDE.md is worse than no CLAUDE.md — it wastes tokens, fills the model with irrelevant noise, or worse, encodes wrong information that the model then trusts.

The bar is: every line should change how Claude Code behaves on a real task. If a line wouldn't change anything, cut it.

## Core principles

1. **Specific beats generic.** "Use Prettier" is in the model's training. "We use 2-space indent and single quotes, run `npm run fmt` before committing" is specific to this repo. Keep the latter, drop the former.

2. **Verify, don't hallucinate.** Every command, path, and convention must come from actually inspecting the code — `package.json`, `Makefile`, `README`, the file tree, etc. Don't write "the API lives in `src/api`" unless you've confirmed it.

3. **Action over description.** Commands, paths, and rules over prose. "Run tests: `pnpm test:unit`" beats "This project uses Vitest for unit testing, which can be run via the package script."

4. **Front-load the highest-value lines.** Stack and run commands first — those are referenced most. Lore and history last (if at all).

5. **Length budget.** Aim for under ~150 lines for a project CLAUDE.md and under ~250 for a monorepo root. Going over usually means duplicating the README or including generic advice.

6. **Don't restate the README.** README is for humans; CLAUDE.md is for the agent. Link to the README for prose; in CLAUDE.md, only the lines the agent needs to act correctly.

## Standard structure

Default scaffold. Reorder or drop sections — keep only what applies.

```
# <Project name>

<One-line description: what this is, in one sentence.>

## Stack
- <Language + framework, version pins if they matter>
- <Other key dependencies the agent will encounter>

## Run / build / test
- Install: `<cmd>`
- Dev: `<cmd>`
- Build: `<cmd>`
- Test: `<cmd>`
- Lint / format: `<cmd>`

## Layout
<Only the non-obvious parts of the file tree. Skip standard dirs.>

## Conventions
- <Patterns specific to this codebase: imports, state, error handling, naming.>

## Gotchas
- <Surprising things that have bitten people before. Be specific.>

## Don't
- <Hard rules: never commit X, never edit Y by hand, etc.>

## More
- README: ./README.md
- Architecture notes: <path if exists>
```

## Workflow: writing one from scratch

1. **Inspect the repo.** `ls -la`, then look at the root files. Find:
   - `package.json` / `pyproject.toml` / `Cargo.toml` / `go.mod` / `Gemfile` — stack and scripts
   - `Makefile` / `justfile` / `Taskfile.yml` — task runners
   - `README.md` — what humans were told
   - `.env.example` — required env vars
   - `tsconfig.json` / `eslint.config.*` / `.prettierrc` — tooling
   - `docker-compose.yml` / `Dockerfile` — runtime
   - For monorepos: `pnpm-workspace.yaml`, `lerna.json`, `turbo.json`, `nx.json`, `apps/`, `packages/`

2. **Pull actual commands.** Read scripts from `package.json`, `Makefile` targets, `pyproject.toml` `[tool.poetry.scripts]`, etc. Don't guess command names.

3. **Identify the stack precisely.** Not "JavaScript" — "Node 20 + Vite + React 18 + TypeScript". Version pin only if the version is load-bearing.

4. **Walk the source briefly.** Look at a couple of source files to spot conventions: how imports work, where config lives, how state is managed. Don't read everything — just enough to surface the 2-3 patterns the agent should know.

5. **Draft, then cut.** Write the file, then strike every line that's generic or duplicated from the README. Ruthlessly brief.

6. **Sanity check.** Re-read as if you were Claude Code being dropped into this repo cold. Does this tell you what you'd want to know? Are the commands runnable?

7. **Save** to `/CLAUDE.md` at the repo root (or the relevant subdirectory).

## Workflow: improving an existing CLAUDE.md

1. **Read it end to end.** Note anything that smells generic, stale, or vague.

2. **Verify each claim against the code.** Commands still work? Paths still exist? Conventions still followed? Cross-reference with `package.json`, the file tree, and a couple of source files. Anything that doesn't check out gets fixed or cut.

3. **Cut redundancy.** Lines that repeat the README. Lines that describe what the framework does (the model already knows). Marketing prose.

4. **Cut vagueness.** "Follow best practices" → either replace with a specific rule from the code, or delete.

5. **Add what's missing.** Walk the workflow mentally: install, run, test, debug, deploy. Are all the commands here? Are the gotchas you've encountered documented?

6. **Recheck length.** If still bloated, consider splitting (see below).

## Splitting for monorepos / multi-app projects

### How Claude Code loads CLAUDE.md files

Claude Code reads CLAUDE.md from the current working directory and walks up the tree, loading each one along the way. Working inside `apps/web/`, both `apps/web/CLAUDE.md` and `/CLAUDE.md` get loaded. This means:
- **Monorepo-wide** info goes at the root.
- **App-specific** info goes in the sub-CLAUDE.md.
- **Don't duplicate** — each line lives in exactly one file.

### When to split

Split into root + per-app files when:
- Multiple apps have **different stacks** (e.g., Next.js web + Python ML service + Go CLI)
- Apps have **different run/test commands** that don't share much
- Apps have **diverging conventions** (different style, different patterns)
- The root CLAUDE.md is creeping past ~200 lines

### When NOT to split

Keep one root file when:
- All packages share the same stack and conventions (e.g., a TS monorepo where every package looks the same)
- The whole thing fits comfortably under ~150 lines at the root
- Sub-packages are small shared libs without distinct workflows

A single well-written root file beats a sprawl of half-empty sub-files.

### Splitting workflow

1. **Inventory sub-projects.** Look at `apps/*`, `packages/*`, `services/*`, or whatever convention the repo uses. Note each one's stack and purpose.

2. **Sort each line of the existing root CLAUDE.md into three buckets:**
   - **Monorepo-wide** → keep at root (workspace layout, top-level commands like `pnpm i` and `turbo run build`, cross-package rules, monorepo tooling)
   - **App-specific** → move into that app's CLAUDE.md
   - **Dead** → cut entirely

3. **For each sub-project**, write a focused CLAUDE.md using the standard structure. Skip sections that don't apply — a tiny shared lib might just need stack + a couple of conventions.

4. **Link from root to sub-files.** At the bottom of the root CLAUDE.md, add a short index:
   ```
   ## Per-app context
   - apps/web — see apps/web/CLAUDE.md
   - apps/api — see apps/api/CLAUDE.md
   - packages/ui — see packages/ui/CLAUDE.md
   ```
   No need to repeat this in each sub-file; the root is loaded automatically.

5. **Sanity check the root is now lean.** A good monorepo root is ~50–150 lines: workspace layout, shared tooling commands, cross-package rules, the index. Still long? More of it probably belongs in sub-files.

### Where each sub-CLAUDE.md lives

By convention:
- `/CLAUDE.md` — monorepo root
- `apps/<name>/CLAUDE.md` — per application
- `packages/<name>/CLAUDE.md` — per library (only if it has distinct workflows)
- `services/<name>/CLAUDE.md` — per backend service

If the repo uses a different layout, follow it.

## Anti-patterns

- **Restating the README.** If it's in README.md, link instead.
- **Listing every file.** The model can `ls`. Only mention paths that matter or are non-obvious.
- **Generic style advice.** "Use meaningful variable names" — cut. "Use `_` prefix for private methods in this codebase" — keep.
- **Framework primers.** Don't explain what React or pytest is. Only document where this codebase diverges from the defaults.
- **Marketing intro.** "This innovative platform leverages cutting-edge…" — delete and move on.
- **Stale commands.** A command that no longer works is worse than no command. Verify before writing.
- **Per-file changelogs.** CLAUDE.md is not a CHANGELOG.
- **Aspirational rules.** "We should be writing more tests" — either it's a rule (document and enforce) or it isn't (don't mention).

## Examples

### Small vanilla JS web extension

```
# Steam Spending Tracker

Chrome/Edge extension that scrapes the Steam purchase history page and renders a spending dashboard.

## Stack
- Vanilla JS, HTML, Tailwind (CDN build)
- Manifest V3 extension

## Run / build
- Load unpacked: chrome://extensions → "Load unpacked" → select repo root
- No build step. Source is what ships.

## Layout
- popup/      — extension popup UI
- content/    — content script that scrapes account/history
- background/ — service worker

## Conventions
- No frameworks, no bundler. If you reach for React or webpack, stop.
- Tailwind via CDN — no PostCSS pipeline.
- CSV parsing lives in content/parse.js. Steam uses inconsistent date formats; tests in content/parse.test.js.

## Gotchas
- Steam's purchase history loads lazily — content script must wait for the table to appear (see waitForTable in content/main.js).
- DOM selectors break roughly every 6 months when Steam redesigns. Update SELECTORS in content/selectors.js if scraping returns zero rows.

## Don't
- Don't add a framework.
- Don't ship a bundler.
```

### Monorepo root

```
# Acme Platform

Turborepo with a Next.js web app, a FastAPI service, and shared TS packages.

## Workspace
- apps/web — Next.js 14, TypeScript (see apps/web/CLAUDE.md)
- apps/api — FastAPI, Python 3.11 (see apps/api/CLAUDE.md)
- packages/ui — shared React components (see packages/ui/CLAUDE.md)
- packages/types — shared TS types

## Top-level commands
- Install JS deps: `pnpm i`
- Install Py deps: `cd apps/api && uv sync`
- Run everything: `pnpm dev` (web + api + types-watch via Turbo)
- Build all: `pnpm build`
- Test all: `pnpm test`

## Cross-package rules
- Shared types go in packages/types; never duplicate across apps.
- API contract types are generated — run `pnpm gen:types` after touching apps/api/schemas.

## Don't
- Don't add packages outside the pnpm workspace.
- Don't import from another app's internals — go through a package.
```

## Final pass before saving

Run this checklist before writing the file:

- [ ] Every command came from a real config file, not a guess
- [ ] Every path mentioned actually exists
- [ ] Nothing here is also in the README
- [ ] No generic best-practice advice
- [ ] Under ~150 lines (project) / ~250 lines (monorepo root)
- [ ] An agent dropped into this repo cold would know how to install, run, and test

If yes to all, save it.
