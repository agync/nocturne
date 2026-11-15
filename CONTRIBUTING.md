# Contributing to Nocturne

Thanks for helping. This page lists what a first change usually trips on. The full procedure is in
the [contributor guide](docs/contribution/guide.md).

## Set up

```bash
make setup    # git hooks and interface dependencies
make check    # the gate every change must pass
```

[Building and packaging](docs/contribution/building.md) lists the toolchain and every target.
[Releasing](docs/contribution/release.md) and [the Flatpak](docs/contribution/flatpak.md) matter only
if you touch those parts.

## Rules

- **Design before build.** Anything a person sees starts as a design in `docs/design/` (screens
  or mockups, copy and behaviour) and is agreed before it is built.
- **One topic per pull request and per commit.** Stage explicit paths.
- **Conventional Commits.** Summary under 50 characters, body lines under 72, for example
  `fix: keep the page after a restart`. Prefer a summary alone.
- **`make check` must pass.** The git hooks run it, so do not bypass them with `--no-verify`.
  Include the real output of the tests you ran in the pull request.
- **Docs move with the change.** A page added, renamed or restructured updates its parent
  `index.md` and adds an entry under `docs/logs/`. `okf validate docs` must pass.
- **No jargon in the interface.** Words like model, token or GPU belong only in advanced views. See
  the [experience principles](docs/design/experience-principles.md).
- **Receipts.** Every answer and page line cites the original it came from.
- **Local only.** No telemetry and no network traffic beyond the consented downloads and the two
  optional features described in [AGENTS.md](AGENTS.md).
- **Forgetting is reversible, deletion is explicit.** Never delete a person's data silently.
- **Public-ready files.** No host-specific paths, private hostnames, tokens or internal task ids in
  anything committed. No em dashes, filler or comments that restate code.

## Pull requests

Branch from the latest `main` as `feat/`, `fix/`, `docs/`, `refactor/` or `chore/` with hyphens.
Describe what changed and why, and link the docs you touched. A maintainer reviews every change.

## AI agents

[AGENTS.md](AGENTS.md) is the contract for AI agents and applies to them in full.

## Security

Do not open a public issue for a vulnerability. See [SECURITY.md](SECURITY.md).

## Licence

By contributing you agree that your work is licensed under Apache-2.0, as in [LICENSE](LICENSE).
