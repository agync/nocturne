---
type: Decision
title: "0001 Rewrite as a desktop app"
description: "Retire the Python command line scaffold and build a Tauri desktop app."
status: draft
tags: [adr]
---

# 0001 Rewrite as a desktop app

## Context

The first scaffold was a standard-library Python command line tool: a SQLite tape and fact graph, a
decay policy, a Markdown paper, a Lemonade client and a minimal MCP server, about 1,100 lines with
tests. It assumed a technical user and a system-wide Lemonade service.

The product goal changed to a polished app for people who are not technical about local AI. That
needs a single installer, a graphical interface, a supervised private Lemonade, background work and
operating system integration (tray, global shortcut, notifications, start at login, printing).

## Decision

Rewrite as a desktop app built on Tauri 2: a Rust core and a web-view interface. The scaffold's
concepts, data model, forgetting policy and tests carry over as specifications, not code. The
scaffold stays in history at the root commit.

## Why

| Option | Verdict |
|---|---|
| Extend the Python scaffold behind a GUI | Two runtimes to bundle (a frozen Python sidecar plus a shell), larger installers, weaker process supervision |
| Electron | Larger footprint and memory use beside multi-gigabyte models |
| Tauri 2 | Small installers, native process supervision in Rust, the same stack as the Lemonade desktop app, and an interface designers can prototype in HTML and CSS |

## Consequences

- The interface framework is chosen in the implementation plan after the design lands; React with
  TypeScript is the default because the Lemonade app uses it.
- Linux web views are WebKitGTK, so the interface avoids features WebKitGTK lacks and bundles its
  fonts.
- The domain logic is rewritten in Rust with the scaffold's behaviour as test cases.
