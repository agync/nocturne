---
type: Log Entry
title: "Ran the Away from home interop tests in CI."
description: "The rust job runs ui/interop, the Rust and TypeScript phone sides against each other."
tags: [log, update]
kind: Update
status: stable
---

The `rust` job now runs `scripts/away-integration.sh interop`, which puts the Rust test phone and
the TypeScript phone client against each other's real side, whenever the Rust side or `ui/`
changes. The script asks cargo where it put the test programs, since Rust 1.100 keeps them outside
`target/debug/deps`, where the tests used to look.
