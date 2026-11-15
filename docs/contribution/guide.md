---
type: Guide
title: Contribution guide
description: The working procedure, from scope to the push gate.
status: draft
tags: [contribution, workflow, review]
---

# Contribution guide

## Set up

```bash
.agents/bootstrap.sh   # installs the git hooks and the local scratch area
make check             # the gate every change must pass
```

`make help` lists every target. You need Rust, Node.js and pre-commit; the `okf` tool validates the
docs bundle. [Building and packaging](building.md) has the versions.

## Working procedure

```mermaid
flowchart LR
    S[Scope in one sentence] --> W[Worktree on a fresh branch]
    W --> P[Plan]
    P --> I[Implement with tests and docs]
    I --> C[make check]
    C --> R[Skeptical review]
    R --> H[Human approves the push]
```

| Step | Rule |
|---|---|
| Scope | One sentence: unit of work, target, outcome. Anything else is a separate change |
| Worktree | One worktree per change; branch `feat/`, `fix/`, `docs/`, `refactor/` or `chore/`, hyphens only, rebased on the latest `main` |
| Commits | Conventional Commits, 50/72, logical units, explicit paths, never `git add -A` |
| Docs | Docs move with the change; a page added or restructured updates its index and adds a log entry |
| Verify | Done only when `make check` passes with real output |
| Review | A skeptical-maintainer review before push; the human decides the push |

## Rules a first change trips on

- Anything a person sees starts as a design in `docs/design/` (screens or mockups, copy and
  behaviour) and is approved before it is built. Code follows the agreed design.
- Interface text follows the [experience principles](../design/experience-principles.md): no model,
  token, GPU, NPU, embedding or graph outside an advanced details view.
- Every answer and page line cites its original, and nothing leaves the computer except the
  downloads and optional features that `AGENTS.md` at the repository root names.
- A page added, renamed or restructured updates its parent `index.md`, adds an entry under
  `docs/logs/`, and passes `okf validate docs` (part of `make check`).
- Committed files hold no host-specific paths, private hostnames, tokens or internal task ids.
- The git hooks run the checks; never bypass them.

Vulnerabilities are reported privately, not as issues: see `SECURITY.md` at the repository root.
