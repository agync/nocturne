# Nocturne: operational contract

Nocturne is a private desktop app that remembers your day, tidies its memory overnight, and hands
you one page each morning. It is a Tauri 2 app (Rust core, web-view interface) that ships and
supervises a private, embedded Lemonade server. It targets people who are not technical about AI.
The design lives in `docs/` (start at `docs/overview.md` and `docs/design/`).

## Design and experience lead

Nocturne is design and experience led. Anything a person sees or touches (screens, flows, copy,
errors, notifications, the site, the install path) is agreed as design first: the screens or
mockups, the copy and the behaviour are written down in `docs/design/` and approved before
implementation starts. Code follows the agreed design. A change to what a person sees goes back
through design, not around it.

The design and its implementation are two pull requests, stacked: the design PR on `main`, the
implementation PR on the design branch. They merge together, so `main` never holds a design that
is not built or a build that is not designed.

## Invariants

1. **No elevation.** Never run `sudo`, a system package manager, or a privileged script unless a
   human explicitly asks.
2. **Local only.** No telemetry, no cloud calls, no network traffic other than consented model and app
   downloads; while the person has turned on Away from home, sealed traffic to their own paired
   phones through the connection they chose, plus that connection's own traffic to its company's
   servers and relays to stay connected; while the person has turned on Online answers, the
   embedded engine's requests to the one provider they chose for answers, online or on their own
   network (a question with the few remembered sentences it needs, never anything marked private),
   plus one look at that provider's list of models each time the engine starts; and, while a phone
   that is its own home has an online mind the person chose, that phone's requests to the one
   provider they chose, online or on their own network (the words of what it reads, each question
   with the few remembered sentences it needs, and the lines a page is written from; never the
   originals themselves, such as photos and recordings, never check-ins, and never anything marked
   private), plus one look at that provider's list of models when it is checked. The embedded
   `lemond` binds loopback with a per-launch key.
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
11. **Automation over manual conformance.** `make check-changed`, the git hooks and CI own style
    and quality. Never bypass hooks.
12. **Design before build.** Nothing a person sees is implemented before its screens, copy and
    behaviour are agreed in `docs/design/`.

## Automation and conventions

- `make help` lists the targets. `make check` is the gate the hooks and CI reuse.
- Commit messages follow Conventional Commits with a summary under 50 characters and body lines
  under 72. Prefer summary-only messages.
- Docs move with the change: a page added, renamed or restructured updates its parent `index.md`
  and adds a log entry under `docs/logs/` in the same commit. The docs bundle is OKF v0.2 and must
  pass `okf validate docs`.

## Verification

Before a push, run `make check-changed` (the lint and drift checks of the areas the change
touches, plus the cheap hooks on the changed files), the tests for what you changed (for example
`make check-changed TESTS="cargo test -p nocturne-engine journal::"`), and any manual or visual
validation, with real captured output. The full suite runs in CI, so `make check` stays the full
gate for anyone who wants it locally, and a change is done when CI is green. A skeptical-maintainer
review (`.agents/agents/maintainer-reviewer.md`) precedes every push, and a human approves the
push.

## Contributor guide

See `docs/contribution/guide.md`.
