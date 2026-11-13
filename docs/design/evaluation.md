---
type: Design
title: Evaluation
description: How memory quality, trust and privacy are measured.
status: draft
tags: [evaluation, benchmark, metrics, privacy]
---

# Evaluation

The question is whether the overnight tidy makes memory better or just bigger, and whether people
trust and use the page.

## Memory quality

Measured on thirty days of real captures with one hundred questions written before tuning.

| Metric | Definition | Target |
|---|---|---|
| Recall@5 | The source fact is in the top five | > 0.8 |
| Answer accuracy | The answer matches the ground truth | > 0.75 |
| Citation precision | The cited original supports the answer | > 0.9 |
| Tokens per answer | Input plus output | < 2k |
| p95 answer latency | Wall time to first useful sentence | < 3 seconds |
| Overnight tidy | Wall time for one day of captures | < 10 minutes |
| Forgetting precision | Let-go items the person wanted to keep | < 5 percent |

Ablations: no memory, full context, naive retrieval, flat facts, graph, graph with the overnight
tidy, and graph with the tidy and decay. Report one table, one chart and a failure analysis, never a
single score.

## Experience

| Metric | Target |
|---|---|
| Install finished to first item kept | < 60 seconds |
| First question answered correctly with a receipt, in an unmoderated test | 4 of 5 participants |
| Morning page opened | More than half of days in a two-week trial |
| Tasks completed without help (capture, ask, restore, delete) | 5 of 5 participants |

If the page is not opened on half the days, the artifact is wrong: test an audio or comic edition
before building more.

## Privacy

| Check | Target |
|---|---|
| Outbound bytes during capture, tidy and answers | 0 |
| Outbound traffic at all without consent | None; model and app downloads are explicit |
| The embedded server is reachable by other apps | No: per-launch key, loopback only |
