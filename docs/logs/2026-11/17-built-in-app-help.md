---
type: Log Entry
title: "Built help with Nocturne in the ask box."
description: "The guide ships inside the app, answers how-to questions from Ask, and has a reader in Settings."
tags: [log, update]
kind: Added
status: stable
---

[Help with Nocturne, from the ask box](/design/in-app-help.md) is built. The guide's sections in
scope ship inside the app, and a question is matched against them by the ways people ask, listed
in each page's front matter, and by the same meaning vector and word search Ask already uses. A
confident match shows the guide's own words beside the memory answer until a size of computer
passes the help check, and none has yet; a size that passes writes one answer from the guide and
what was kept, always on this computer. An unsure match asks "Did you mean help with Nocturne?".
The guide reader is the last row of Settings. The controls the guide names are drawn as their
control chips, from the one chip list the interface uses, and "Open {screen}" comes back to the
answer.

The in-scope [guide](/guide/index.md) pages were checked against the app, reworded where they used
words the interface may not, given their `asked` lists and marked stable. "Answers only from what
you kept" now reads "answers only from what it can show you" on the site, in the guide and in the
app. The help check is in [Evaluation](/design/evaluation.md#help-with-nocturne-the-help-check) and
the [release checklist](/contribution/release.md#checklist).
