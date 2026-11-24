---
type: Log Entry
title: "Added the look-up check."
description: "Evaluation gains the look-up check per size, and the network check allows at most 4 requests per question."
tags: [log, update]
kind: Changed
status: stable
---

[Evaluation](/design/evaluation.md) gains the look-up check, which decides whether a size's own
mind may look things up, and its scripted parts that run now. The network check for Online answers
changes from at most once per question to at most 4 times.
