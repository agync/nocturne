---
type: Log Entry
title: "Backed up a phone each night to its own account."
description: "The locked copy becomes Backup: nightly, incremental and locked, to the person's own iCloud or Google Drive, with a six-word phrase and an optional recovery key."
tags: [log, update]
kind: Changed
status: stable
---

The owner chose on 2026-11-23 a backup that works like a messaging app's; a file in a folder the
person picks was considered and dropped. [0019 A locked copy online](/adr/0019-a-locked-copy-online.md) is
rewritten: each night while the phone charges, what changed goes in locked pieces with a small
index to the person's own iCloud (the app's container) or Google Drive (the scope for files the app
creates), opened with a random six-word phrase or the person's own, and an optional recovery key
offered once. It needs an invariant 2 clause, written in 0019 and
[0018](/adr/0018-a-phone-as-its-own-home.md#proposed-contract-changes). [Nocturne on a phone of its
own](/design/phone-first.md#backup) replaces "A locked copy online" with "Backup", its screens,
states and copy, and redraws the `copy-online` and `this-phone` mockups. [Phone and computer as equal
starting points](/design/phone-parity.md) records decision 38; [Moving your
memory](/design/moving-memory.md) and [A household computer](/design/household-server.md) follow
the new words.
