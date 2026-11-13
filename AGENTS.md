# Nocturne: operational contract

Nocturne is a private desktop app that remembers your day, tidies its memory overnight, and hands
you one page each morning. It is a Tauri 2 app (Rust core, web-view interface) that ships and
supervises a private, embedded Lemonade server. It targets people who are not technical about AI.
The design lives in `docs/` (start at `docs/overview.md` and `docs/design/`).

## Invariants

1. **No elevation.** Never run `sudo`, a system package manager, or a privileged script unless a
   human explicitly asks.
2. **Local only.** No telemetry, no cloud calls, no network traffic other than consented model and app
   downloads, and, while the person has turned on Away from home, sealed traffic to their own paired
   phones through the connection they chose, plus that connection's own traffic to its company's
   servers and relays to stay connected. The embedded `lemond` binds loopback with a per-launch key.
3. **No jargon in the interface.** Model, inference, token, NPU, GPU, embedding and graph appear only
   in advanced details views. See `docs/design/experience-principles.md`.
4. **Receipts or it did not happen.** Every answer and page line cites the original it came from.
5. **Forgetting is reversible; deletion is explicit.** Never silently delete a person's data.
6. **Public-ready by default.** No host-specific paths, private hostnames, tokens or internal task
   identifiers in committed files.
7. **Zero AI slop.** No em dashes, no filler or marketing prose, no comments that restate code.
8. **Scratch stays local.** `.agents/brain/` is git-ignored. Never commit or link it.
9. **Logical commits.** One topic per commit; stage explicit paths, never `git add -A`.
10. **Parallel work in worktrees.** One worktree per change, on a branch rebased on the latest `main`.
11. **Automation over manual conformance.** `make check` and the git hooks own style and quality.
    Never bypass hooks.

## Automation and conventions

- `make help` lists the targets. `make check` is the gate the hooks and CI reuse.
- Commit messages follow Conventional Commits with a summary under 50 characters and body lines
  under 72. Prefer summary-only messages.
- Docs move with the change: a page added, renamed or restructured updates its parent `index.md`
  and adds a log entry under `docs/logs/` in the same commit. The docs bundle is OKF v0.2 and must
  pass `okf validate docs`.

## Verification

A change is done only when `make check` passes and the relevant tests are green, with real captured
output. A skeptical-maintainer review (`.agents/agents/maintainer-reviewer.md`) precedes every push,
and a human approves the push.

## Contributor guide

See `docs/contribution/guide.md`.
