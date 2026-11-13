---
type: Decision
title: "0002 Embed a private Lemonade"
description: "Ship and supervise a private lemond instead of depending on a system service."
status: draft
tags: [adr]
---

# 0002 Embed a private Lemonade

## Context

A system Lemonade service is shared: other apps can load, evict or delete models, and the person has
to install and run it. Lemonade publishes an embeddable artifact (`lemond`, the `lemonade` CLI and
resources) for Windows, Ubuntu and macOS meant for exactly this.

## Decision

Nocturne bundles the embeddable artifact and runs a private `lemond` as a supervised subprocess:
loopback only, a random port and API key per launch, `models_dir` inside app data, discovery off.
Backends and models are installed through the runtime API on first run, chosen from
`/v1/system-info`.

## Consequences

- People never see Lemonade unless they open advanced details.
- Nocturne owns backend and model versions through `backend_versions.json` and `server_models.json`.
- The app must handle first-run downloads of several gigabytes gracefully and keep capture working
  during them.
- Model files are not shared with a system Lemonade; disk use is the price of isolation.
