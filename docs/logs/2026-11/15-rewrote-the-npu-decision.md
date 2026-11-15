---
type: Log Entry
title: "Rewrite the NPU decision."
description: "Decision 0006 now puts reading recordings on the NPU by default, from measurements."
tags: [log, update]
kind: Update
status: stable
---

[Decision 0006](/adr/0006-npu-optional.md) is rewritten: a usable NPU reads recordings when its
model fits the memory budget, every other job stays on the GPU with the reasons for each, and an
NPU failure falls back to the GPU. The unsourced crash report is gone and the measurements are in.
