---
type: Guide
title: Pin bumps
description: What Dependabot keeps current, every pin it cannot see, where each one lives and how to bump it, and which ones a later bump workflow could automate.
status: draft
tags: [contribution, dependencies, ci, supply-chain]
---

# Pin bumps

Nocturne pins what it downloads and builds with, so a release is the same bytes twice. A pin
does not move on its own, so someone moves it, on purpose, with a pull request.

## What Dependabot does

`.github/dependabot.yml` opens pull requests weekly, staggered over four days:

| Ecosystem | Directory | Day | Grouped |
|---|---|---|---|
| `github-actions` | `/` | Monday | minor and patch together |
| `cargo` | `/` | Tuesday | minor and patch together |
| `npm` | `/ui` | Wednesday | minor and patch together |
| `gomod` | `/helpers/tailnet` | Thursday | minor and patch together |

A new major version gets a pull request of its own. Every title starts with `chore(deps)`, or
`ci(deps)` for the actions, so release-please does not count a bump as a release. A version is
proposed only after it has been out for seven days. Each ecosystem has at most five open pull
requests, three for Go. CI runs on them like on any other pull request, and each is merged by a
person.

Two things to check on a Go pull request. A new `tailscale.com` changes the helper the release
builds, so its pin in `helpers.lock` moves with the next release. A change to the `go` line of
`helpers/tailnet/go.mod` must be matched by `go_version` in `helpers.lock` (see the Go toolchain
row below).

## What Dependabot cannot see

| Pin | Where | How to bump it | A later workflow could |
|---|---|---|---|
| Embedded engine (Lemonade) | `lemonade.lock`: version, base URL and a sha256 per platform. The Flatpak manifest repeats the Linux archives, and `scripts/check_flatpak.py` fails `make check` when they differ | Take the new release's archives and their sha256, edit both files, then run the engine's runtime tests (`make e2e-real`) | Yes: find the release, fill both files. The runtime tests stay a person's gate |
| Connection helper (cloudflared) | `helpers.lock`, with captured help text under `crates/helpers/fixtures/help` | Change the version, then run `scripts/helpers-pin.sh --write`, which fills the checksums and recaptures the help text; review the fixture diff | Yes, as a pull request that shows the fixture diff |
| Most private helper (tailscale.com) | `version` in `helpers.lock`, with the module in `helpers/tailnet/go.mod` | Merge the Dependabot pull request, then pin the checksums of the two reproducible builds from `tailnet.yml` after a release | No: the checksums exist only after a release |
| Go toolchain | the `go` line of `helpers/tailnet/go.mod` and `go_version` in `helpers.lock`; `tailnet.yml` reads the second, `ci.yml` and `scripts/test-tailnet.sh` the first | Move both to the same version in one commit | Yes, from the Go release list, writing both |
| `go_licenses_version` | `helpers.lock`, a placeholder until the first release | Set it to the licence tool version the licence report is built with | No |
| AetherPak container | the `image:` digest in the `build` job of `.github/workflows/flatpak.yml`. Dependabot does not read `container:` in a workflow | Resolve the builder tag to its digest, replace it and the date in the comment, then run the workflow by hand with `publish` off | Yes: resolve the tag, rewrite the digest and the date |
| AetherPak actions | `aetherpak/actions/*` in the same workflow, by commit and version comment | Dependabot bumps these with the other actions | Already covered |
| GNOME runtime and SDK extensions | `runtime-version` and `sdk-extensions` in `flatpak/is.nocturne.Nocturne.yml`, repeated in [The Flatpak](flatpak.md) | A new runtime must carry a glibc new enough for the pinned engine. Move it, rebuild locally and run the Flatpak checks | No: it is a decision |
| Flatpak generators and shared modules | `tools_commit` and `cargo_generator_sha256` in `scripts/flatpak-sources.sh`; `flatpak/shared-modules/` | Take the new commit, update the checksum, rebuild the sources | Not worth it |
| Docs renderer (okf-wiki) | `VERSION` in `site/toolchain.sh`, the whole `site/okf-wiki.sha256`, and the image digest `OKF_WIKI_IMAGE` in `site/Makefile`, which carries the same release | Change the version, replace `okf-wiki.sha256` with the release's `checksums.txt`, update the image digest, run `make site` | Yes: all three come from one release |
| `okf` in CI | `OKF_VERSION` and `OKF_SHA256` in the `hygiene` job of `.github/workflows/ci.yml` | Take the release's `checksums.txt` line for `linux_amd64` | Yes |
| cargo-nextest in CI | `NEXTEST_TOOL` in `.github/workflows/ci.yml`; `nextest-version` in `.config/nextest.toml` is the oldest it accepts | Pick a release at least a week old that the pinned `taiki-e/install-action` lists in its `manifests/cargo-nextest.json` (that list holds the sha256 it checks), then let CI run | Yes: read the action's manifest |
| Pre-commit hooks | `rev` in `.pre-commit-config.yaml`, `pre-commit==` in `ci.yml` | `pre-commit autoupdate`, then `make check` | Yes, with `pre-commit autoupdate` |
| Node and Rust | `NODE_VERSION` in `ci.yml`, `node-version` in `release.yml` and the Node stage of `Containerfile`; `channel` in `rust-toolchain.toml` and the Rust stage of `Containerfile`; `rust-version` in `Cargo.toml` (the Rust the Flatpak SDK's `rust-stable` extension provides, never above it) | Move them together, then fix any new clippy findings. `@types/node` in `ui/package.json` moves with the Node pin (major), which is why Dependabot ignores its major updates, and the Flatpak's `node26` extension is moved with it | No |

The npm and Cargo pull requests move the Playwright package, and `npx playwright install` in CI
fetches the browser that version needs, so the browsers need no pin of their own.

The rows marked yes share a shape: find the newest release, rewrite a few lines, open a pull
request. A pin-bump workflow can do that one tool per job, as a bot that opens ordinary pull
requests so CI runs on them. The engine, the docs renderer, `okf` and the AetherPak digest are the
first to move there. Nothing of the kind exists yet.
