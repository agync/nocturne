---
type: Decision
title: "0012 Run as a server, experimental"
description: "A nocturne-server program and container image that serve the real app to a browser, signed in with a token kept in the data folder, behind the person's own TLS proxy."
status: draft
tags: [adr, server, container, security]
---

# 0012 Run as a server, experimental

## Context

Some people want Nocturne on a home server or another machine that is always on and to use it from a browser.
The development preview (`nocturne-preview`) already serves the real app over HTTP, but it is
built for one developer session: a token made at every launch, loopback only, a debug build. A
server has to survive restarts, run in a container and be reached through a proxy, without
weakening what the preview guarantees.

## Decision

- **A program of its own, labelled experimental.** `nocturne-server`, behind the `server` cargo
  feature, which builds on the preview's HTTP front (`src-tauri/src/preview/`). It ships only as a
  container image (`Containerfile`), never in a desktop package; the package check fails if the
  program or its interface is in one. The documentation calls it experimental until the blockers
  below are closed.
- **How it is reached.** It listens on `NOCTURNE_BIND` (default `127.0.0.1:55066`). The image
  listens on `0.0.0.0:55066` inside its own network, and the guide publishes that port on the
  host's loopback only. Plain HTTP is for this computer. Anything else goes through a TLS proxy the
  person runs (Caddy, Tailscale serve, or similar), named in `NOCTURNE_PUBLIC_ORIGIN`, which must be
  `https://` or a loopback `http://` address; the server refuses to start otherwise. There is no
  built-in TLS. The `Host` and `Origin` checks are the preview's: only `127.0.0.1` on the port and
  the public origin are accepted, so a page on another site or a name pointed here cannot use it.
- **Signing in.** A 256-bit token is kept in `server-token` in the data folder, owner-only. It is
  made at the first start and printed once. `nocturne-server token` prints the sign-in link again,
  and `nocturne-server token --rotate` replaces the token; the running server reads the file at
  every check, so the old link stops working at the next request without a restart. A missing
  or malformed file admits nobody. The link carries the token in the fragment, which never reaches
  the server in a URL; the page sends it as a bearer header and keeps it in this browser's local
  storage (the preview keeps its own per tab). There are no accounts: one server is one person.
- **Health.** `GET /healthz` needs no token but needs an accepted host, carries no data, and answers
  200 while the store is open. `nocturne-server health` calls it for the image's health check.
- **What stays the same.** The private engine binds loopback inside the container with its
  per-launch key. The data folder is a volume (`/data`), marked so that the desktop app's folder is
  never taken over. Models download at first setup with consent, as on the desktop. Event and media
  tickets, path hiding, upload limits, the converter limits and the interface's content policy are
  the preview's. The Online answers key is kept in the data folder's owner-only key file.
- **What does not apply.** The tray, keep-awake, start at login, desktop notifications, the system
  keychain, the global shortcut and quick capture, file and folder dialogs, opening files in the
  system's apps, printing, exporting to a folder, watching an inbox folder, and Away from home. The
  page reports these as unavailable. Updating is pulling a newer image; there is no updater inside.

## Development shortcuts, and what changes

| Shortcut in the preview | For the server |
|---|---|
| A token made at every launch | Replaced by the token file and its two commands |
| Loopback bind only | Replaced by `NOCTURNE_BIND`, with `https://` required for any other origin |
| Built in debug, `make preview-real` | The image builds `--release`: no checkout engine lookup, no `NOCTURNE_LOG`, no scripted mind, no error details in answers. The engine is found through `NOCTURNE_LEMOND` |
| Tauri's mock runtime and its test call path (`tauri::test`) run the app without a window | Kept on purpose: it sends every command through the same window allowlist as the desktop. It is a test API with no stability promise; `Cargo.lock` pins it and the preview tests cover it. **Blocker** for leaving experimental |
| Turning on Online answers is printed instead of confirmed in a system window | Kept. The page's own consent screen still runs; a browser has no system window. Whoever holds the token can read everything already, so this adds nothing |
| The app crate links the desktop toolkit | Kept. The image carries those libraries though the server never opens a window, which costs size. **Blocker**: a headless build of the app crate |
| Logs go to files in the data folder | Kept (`/data/logs`); the console shows start-up lines only |
| `ffmpeg` found on the path at start | Kept; the image ships Debian's `ffmpeg` for browser recordings |

## Residual risks

- Between the proxy and the server, traffic is plain HTTP on the host's loopback or the container
  network. Publishing the port beyond loopback without a proxy exposes the token on the network the
  first time a browser uses it there; the guide never does this, and a plain `http://` public origin
  that is not loopback is refused.
- The token sits in the browser's local storage, readable by any script on that origin; the content
  policy allows scripts from the server only.
- The first start prints the token to the container's log.
- A rotated token does not close an event stream already open or end a media ticket already issued
  (an hour at most). Restart the server after rotating if the old link may have leaked.
- Anyone who can run commands in the container (`podman exec`) can read the token, as on any host.
- GPU access inside a container depends on the host's drivers and device permissions; only what the
  guide names was tried.

## Consequences

- No new outbound traffic: the server answers the person's own browsers, and everything else is the
  desktop app's. Building the image downloads the base images, the interface and Rust
  dependencies and the pinned engine, as a desktop build does.
- `make server/build` and `make server/image` build it; `make check` tests the server's token,
  health and origin rules with the preview's tests.
- The image is published by `.github/workflows/container.yml` on release tags; pull requests only
  build it ([building](/contribution/building.md)).
