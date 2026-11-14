# Decisions

Architecture decision records. One decision per page; a superseded decision is marked deprecated,
not deleted.

- [0001 Rewrite as a desktop app](0001-rewrite-as-desktop-app.md) - Retire the Python command line scaffold and build a Tauri desktop app.
- [0002 Embed a private Lemonade](0002-embedded-lemonade.md) - Ship and supervise a private lemond instead of depending on a system service.
- [0003 One SQLite file](0003-sqlite-single-file.md) - The tape and the memory live in one local SQLite file.
- [0004 An open engine](0004-open-engine.md) - The memory engine is open source and self-contained.
- [0005 The page first](0005-page-first.md) - The morning page is the first milestone; senses and print follow.
- [0006 The NPU is optional](0006-npu-optional.md) - The NPU works only when the GPU is idle, with a GPU fallback.
- [0007 Forgetting by default](0007-forgetting-by-default.md) - Facts fade at 30 days and archive at 90; deletion is explicit.
- [0008 A small schema](0008-small-schema.md) - A short list of predicates; new ones are quarantined until confirmed.
- [0009 Package locally and prove it stays local](0009-packaging.md) - Linux packages with the private engine inside, and a release-time proof that nothing leaves the computer.
- [0010 Away from home, opt-in and sealed](0010-away-from-home.md) - Ask and keep from a paired phone through Cloudflare or Tailscale, sealed end to end, off by default.
- [0011 Online answers, opt-in and through the engine](0011-online-answers.md) - A provider the person chose may write the last step of an answer, called by the embedded engine, bounded and logged.
