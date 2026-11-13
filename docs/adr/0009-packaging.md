---
type: Decision
title: "0009 Package locally and prove it stays local"
description: "Linux packages with the private engine inside, built and checked on one machine, with the outbound proof done at release time."
status: draft
tags: [adr, packaging, privacy]
---

# 0009 Package locally and prove it stays local

## Context

Nocturne ships a private engine and promises that nothing leaves the computer. A package that
silently carries the wrong engine, a missing licence or a path from the builder's machine breaks
that promise, and so does a build that talks to the network when it starts. A live per-program
byte count needs privileges (eBPF or control groups) that an ordinary app does not have.

## Decision

- **Targets.** Linux x86_64 packages: an AppImage, an rpm and a deb. Windows installers are built
  on a Windows machine only and are not published in v1. aarch64 and macOS come later.
- **Opt-in configuration.** Packaging is a Tauri configuration layered at bundle time
  (`src-tauri/bundle/linux.json`), so a plain build, `make dev` and `make check` never need the
  fetched engine.
- **The engine is a resource folder, not a sidecar.** The embeddable bundle is a folder, so it
  ships as `lemonade/` next to the program. The runtime looks for it in `NOCTURNE_LEMOND`, then in
  its own resources, then (debug builds) in the checkout.
- **A verified fetch.** The package script downloads the pinned archive fresh, checks its sha256
  against `lemonade.lock`, refuses links and extra top-level entries, and records a sha256 of every
  file. The checker compares the packaged engine against that manifest byte for byte. The
  AppImage tool rewrites executables it finds, so the script puts the verified files back before
  packing.
- **One version source.** The workspace `Cargo.toml`.
- **Licences.** Allowed: MIT, Apache-2.0 (and with the LLVM exception), BSD-2-Clause,
  BSD-3-Clause, ISC, Zlib, OFL-1.1, Unicode-3.0, MPL-2.0, CC0-1.0, BSL-1.0, 0BSD, Unlicense and
  CDLA-Permissive-2.0; anything else or unknown fails the notices step. The AppImage also carries
  system libraries: each is listed with its licence and source package, unused GPL modules are
  removed, and any remaining GPL (not LGPL) licence needs a reviewed entry naming the package and
  library and why it does not bind. A source manifest ships beside the artefacts.
- **No host paths.** The binary is built with the builder's folders remapped, and the checker
  looks for the builder's home in every file, binaries included.
- **Proof of locality at release time.** A live byte count is not available, so the app says
  "Not measured" and shows its download ledger. The release checklist runs the app and its engine
  in a private network namespace under `strace` and fails on any non-loopback address, name lookup,
  unexpected local socket or `io_uring`, and unless the app demonstrably worked.
- **Local only.** Build, check, smoke, tag. No push, no upload, no release page, no CI in v1.
- **Windows.** NSIS per user and MSI, with the WebView2 bootstrapper embedded so the runtime is
  fetched only when it is missing and the person is present. Unsigned in v1.

## Consequences

- A release takes about as long as the longest of its steps, and its evidence is on the machine
  that built it.
- The outbound proof is a release-time statement, not a live guarantee. The privacy screen says
  so honestly.
- Bundling system libraries in the AppImage means the project owes sources and notices for them;
  that offer's wording is an owner decision tracked in the release checklist.
- Changing the engine pin means re-running the checklist, because the manifest and the runtime
  tests depend on it.
