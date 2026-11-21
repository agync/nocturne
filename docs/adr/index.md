# Decisions

Architecture decision records. One decision per page; a superseded decision is marked deprecated,
not deleted.

- [0001 Rewrite as a desktop app](0001-rewrite-as-desktop-app.md) - Retire the Python command line scaffold and build a Tauri desktop app.
- [0002 Embed a private Lemonade](0002-embedded-lemonade.md) - Ship and supervise a private lemond instead of depending on a system service.
- [0003 One SQLite file](0003-sqlite-single-file.md) - The tape and the memory live in one local SQLite file.
- [0004 An open engine](0004-open-engine.md) - The memory engine is open source and self-contained.
- [0005 The page first](0005-page-first.md) - The morning page is the first milestone; senses and print follow.
- [0006 The NPU reads recordings by default](0006-npu-optional.md) - A usable NPU reads recordings when its model fits; every other job stays on the GPU, and any NPU failure falls back to the GPU.
- [0007 Forgetting by default](0007-forgetting-by-default.md) - Facts fade at 30 days and archive at 90; deletion is explicit.
- [0008 A small schema](0008-small-schema.md) - A short list of predicates; new ones are quarantined until confirmed.
- [0009 Package locally and prove it stays local](0009-packaging.md) - Linux packages with the private engine inside, and a release-time proof that nothing leaves the computer.
- [0010 Away from home, opt-in and sealed](0010-away-from-home.md) - Ask and keep from a paired phone through Cloudflare or Tailscale, sealed end to end, off by default.
- [0011 Online answers, opt-in and through the engine](0011-online-answers.md) - A provider the person chose may write the last step of an answer, called by the embedded engine, bounded and logged.
- [0012 Run as a server, experimental](0012-server-mode.md) - A nocturne-server program and container image for a browser, signed in with a token kept in the data folder, behind the person's own TLS proxy.
- [0013 Back up and restore with a passphrase](0013-passphrase-backup.md) - One file holds everything needed to restore, sealed with a passphrase the person sets, and a restore never deletes what is already here.
- [0014 The NPU takes the mind's work](0014-npu-mind.md) - On computers with an XDNA 2 NPU, Gemma e4b on the NPU reads beside the GPU mind where both fit, and reads and answers alone where only one fits.
- [0015 A journal privacy class](0015-journal-privacy-class.md) - A third sensitivity keeps the journal out of audio, notifications, the tray and a lock screen, with no private switch.
- [0016 Decision models, shadow first](0016-decision-models-shadow-first.md) - Typed questions are asked beside the rules and logged, never acting: first of the mind already loaded, with decision models in the evaluation only.
- [0017 Native phone apps on a shared Rust channel](0017-native-phone-apps.md) - Tauri 2 apps for Android first and iPhone later, on one Rust crate for the sealed channel shared with the computer, with the browser phone page kept.
- [0018 A phone as its own home](0018-a-phone-as-its-own-home.md) - Proposed: the phone keeps its own memory with the same engine, no mind at first, one home at a time, moved only by pairing; with the proposed contract wording.
- [0019 A locked copy online](0019-a-locked-copy-online.md) - Proposed: numbered copies of a phone's memory, locked with a key only the person holds, in the one place they chose, as its online backup.
- [0020 An online mind on a phone](0020-an-online-mind-on-a-phone.md) - Proposed: a service with the person's own key, or Apple's servers after a trial, reads the words of what a phone keeps, writes its page and answers.
- [0021 A household mind for invited phones](0021-a-household-mind-for-invited-phones.md) - Proposed: a computer lends its mind to any phone holding its one household code, keeping nothing about them.
