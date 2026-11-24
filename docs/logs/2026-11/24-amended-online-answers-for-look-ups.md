---
type: Log Entry
title: "Amended Online answers for look-ups."
description: "Decision 0022 amends 0011: today's date goes with every question, and look-ups wait on the computer until the engine passes them on."
tags: [log, update]
kind: Changed
status: stable
---

[Decision 0022](/adr/0022-ask-looks-things-up.md) records Ask with tools and amends
[0011](/adr/0011-online-answers.md). [Online answers](/architecture/online.md) now sends today's
date and the date step's lines, up to 24 in all, and waits for the pinned engine to pass look-ups on
before a question may take more than one request.
