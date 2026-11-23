---
type: Log Entry
title: "Moved the mind's words and judgement into their own crate."
description: "The prompts, the reading of a reply into memories, the guards, the receipts and the question asked online now live in crates/mind, which builds for the phone, with no change in behaviour."
tags: [log, update]
kind: Changed
status: stable
---

The words the mind is given, the reading of a reply into memories, the guards a reply must pass,
the receipts an answer cites, and the question asked online with its receipt template moved from
`crates/lemonade` into `crates/mind` (`nocturne-mind`). It starts no process and reaches no
network, so the phone can answer with the same words and checks
([decision 0020](/adr/0020-an-online-mind-on-a-phone.md)). The private engine's mind uses it as
before: nothing changes in what the computer sends or accepts, and the prompt snapshots moved
unchanged. The [repository layout](/architecture/layout.md) lists the new crate, and the
`android (engine)` check in [building](/contribution/building.md) now checks it for the phone.
