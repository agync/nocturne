---
type: Design
title: Advanced details
description: The words the advanced details view uses where they have been designed and approved.
status: draft
tags: [design, copy, advanced, npu]
---

# Advanced details

The advanced details (Settings, then "Show advanced details") are the only view that may name
models, the NPU and the GPU ([experience principles](experience-principles.md)). Model names come
from data. This page holds the copy for its rows as each is designed.

## The NPU row

Label "NPU". The value says what the NPU does, then the models it uses:

| When | Value | Status |
|---|---|---|
| No NPU, or one Nocturne cannot see | none found | in use |
| It reads recordings only | reads recordings · {speech model} | in use |
| It reads recordings and items while the GPU mind answers (Split, [decision 0014](/adr/0014-npu-mind.md)) | reads recordings and what you keep · {speech model}, {NPU mind} | approved |
| It reads recordings and items and answers, with no GPU mind loaded (NPU only, decision 0014) | reads, answers and reads recordings · {speech model}, {NPU mind} | approved |
| It is not used | not used · {reasons}, with the engine's words and the steps to fix it in rows below | in use |
