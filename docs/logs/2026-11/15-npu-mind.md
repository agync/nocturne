---
type: Log Entry
title: "Give the mind's work to the NPU."
description: "Decision 0014 puts Gemma e4b on the NPU to read beside the GPU mind, or to read and answer alone where memory is tight."
tags: [log, decision]
kind: Update
status: stable
---

[Decision 0014](/adr/0014-npu-mind.md) records the evaluation of NPU chat models and where the
best one runs. Gemma e4b on the NPU matched the GPU mind on answers, facts kept and citations, so
it reads items beside the GPU mind where both fit, and reads and answers alone on shared memory
where only one fits.
