---
type: Log Entry
title: "Let private things answer online."
description: "Online answers on the computer sends a private memory like any other; private keeps a thing out of notifications, reminders and the lock screen only."
tags: [log, update]
kind: Changed
status: stable
---

[0011 Online answers](/adr/0011-online-answers.md) was amended on 2026-11-23: a private mark decides
only what comes up on its own, and never keeps a memory out of an answer the person asks for. The
computer's Online answers still held private memories back. It now sends them like any other, and
holds back only what the person let go or is forgetting, and anything the store does not know. A
line of an original still stays here, and an answer that needed one says so.

The words that said private things are never sent, in the setup steps, Settings, the privacy page,
the notice for people who agreed before, the guide, the site and its five translations, now say
what private does. A paired phone is unchanged: it still keeps private things on the computer until
its own change is built ([What private means](/design/only-marked-private.md)).
