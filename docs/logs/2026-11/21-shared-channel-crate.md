---
type: Log Entry
title: "Moved the sealed channel into its own crate."
description: "The sealed channel's pure steps now live in crates/channel, shared by the computer and the coming phone app, with no change in behaviour."
tags: [log, update]
kind: Changed
status: stable
---

The sealed channel's constants, keys, pairing, session and frame steps moved from the app shell
into `crates/channel` (`nocturne-channel`), so the native phone app can use the same Rust code
([decision 0017](/adr/0017-native-phone-apps.md)). The shell re-exports them under the same paths
and keeps the computer's key file. Nothing changes for the person or on the wire. The
[repository layout](/architecture/layout.md) lists the new crate.
