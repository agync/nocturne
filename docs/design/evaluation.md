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
| Away from home off: listeners, helper processes and outbound traffic | None; no helper file unless a download was agreed to |
| Away from home on: outbound traffic | Only from the helper process, never from the app or the engine; any-interface UDP only from the Tailscale helper |
| Online answers off: the engine's providers, keys and outbound traffic | None; zero provider traffic in the network check with a key stored |
| Online answers on: outbound traffic | Only from the engine, only to the one entered address, only while a question is asked, at most once per question; never from the app, never while idle, never after turning off (`online` mode of the network check) |
| Private and sensitive markers in captured online traffic | 0 |
| Online answer accuracy against the Tiny tier on the same questions | Measured (S1b), see below; no target is set |
| Plaintext in a relay recording after a scanned pairing | 0 bytes |
| Bytes sent to phones | Counted at the listener and shown on Privacy |
| Private items reaching a phone | 0 |
| Away from home idle memory and wake-ups, no phone connected | Targets: app resident growth under 25 MB, no timer more often than every 30 s; during a 50 MB upload, peak growth under 10 MB. To be measured on the 8 GB Mac; the helper's own memory (target under 80 MB) is measured in the real-network trial |

Measured on Linux with the stand-in helper (`scripts/away-integration.sh real`, a desktop session
under a virtual display): with Away from home on, the app's resident memory grew by 2.8 MB, its
peak grew by 2.5 MB during a 50 MB upload, and after turning it off and after quitting no process
or listener of Nocturne was left. These are the app's own figures, not the real helper's, and Mac
and Windows are not measured.

## Help with Nocturne (the help check)

Whether a size of computer may write help answers from the guide is decided by the check in
[Help with Nocturne, from the ask box](in-app-help.md#blending-on-small-computers-a-gate-for-each-size):
18 help questions, 5 memory questions that share words with the guide, and 6 mixed questions,
against eleven seeded memories, each asked three times on this computer. A size passes only when
every check holds in every run.

| Check | Pass |
|---|---|
| Finds the right section | An expected guide section is cited for at least 16 of the 18 help questions |
| No help where none was asked | No memory question gets a help answer or the guide's words; the unsure prompt for at most 1 |
| No blending | No sentence citing the guide holds a name, number, date, place or thing of a memory, and no sentence citing a memory holds a control of the guide |
| Faithful steps | Every control a guide sentence names is spelled as the control is and is in the cited section, in the section's order |
| Both halves | At least 5 of the 6 mixed questions cite the right section and the right memory |
| Speed | First useful sentence under 3 seconds at the 95th percentile |

The set and the scoring are in `src-tauri/tests/eval/help.rs`. `cargo test -p nocturne-app --test
help` runs it on a scripted mind, which checks the wiring and the scoring, not a size; `make
e2e-help` runs it on the real models of the size `NOCTURNE_E2E_TIER` sets up and writes
`help.json` with the checks per run and every sentence the script could not settle, for a person
to read. No size has passed yet, so every size shows the guide's own words.

## Online answers on the smallest tier (S1b)

Measured against a stand-in provider with a larger model, on the smallest tier, with the same
questions answered locally and through the online path. Answering through the online path scored a
median of 6 of 12 against 7 of 12 locally, and took about 3 s against 0.8 s to answer. No private or
sensitive test markers reached the stand-in provider. Online answers are not claimed to be better on
small computers; the value is that a person can choose a service they already trust, and the cost is
a slower answer.
