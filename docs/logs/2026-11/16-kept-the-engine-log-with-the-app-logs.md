---
type: Log Entry
title: "Moved the engine's log in the docs to the app's logs folder."
description: "The runtime, Online answers and your-data pages now say lemond.log is in the logs folder that Open logs opens."
tags: [log, update]
kind: Update
status: stable
---

The engine's own log moved from `engine/logs` to the app's `logs/` folder, which "Open logs"
opens, so a report of an engine that would not start can come with it. The
[runtime](/architecture/runtime.md#embedded-lemonade-lifecycle),
[Online answers](/architecture/online.md) and [your data](/guide/your-data.md) pages say where
it is now. An older copy under `engine/logs` is left where it is.
