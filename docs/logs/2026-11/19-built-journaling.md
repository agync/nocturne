---
type: Log Entry
title: "Built journaling."
description: "The journal is built as designed, all but the page question from the journal, and the data model, runtime, guides, evaluation and a new decision record say how."
tags: [log, update]
kind: Added
status: stable
---

The [journal](/design/journaling.md) is built as approved: a daily entry, the day's timeline,
feelings check-ins, good things, people and places on each entry, a reflection on any day, week,
month and year recaps, and a phone that reads it all. Writing reflections and recaps is gated per
size of computer, and no size has passed yet, so every size shows "At a glance". The page's
question from the journal (decision 7) is not built yet: the page asks its usual question.

Pages that changed with it:

- [Data model](/design/data-model.md): the `checkin` kind, the `journal` sensitivity, and new
  sections on mentions, journal grouping and versions, and reflections and recaps.
- [Runtime](/architecture/runtime.md): the evening reminder, the tray's "Check in…", when
  reflections and recaps are written, and what the spoken page skips.
- [Online answers](/guide/online-answers.md) and [Away from home](/guide/away-from-home.md): what
  never leaves, and what the phone can do.
- [Keeping the next step in view](/design/step-flow-actions.md): the Online answers step 1 and
  step 5 words for the journal.
- [Decision 0011](/adr/0011-online-answers.md), and the new [decision
  0015](/adr/0015-journal-privacy-class.md) for the journal privacy class.
- [Evaluation](/design/evaluation.md): the journal check.
- [Experience principles](/design/experience-principles.md): the one optional evening reminder.
