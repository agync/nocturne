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

## The journal check

Whether a size of computer may write reflections and recaps is decided by the check in
[Journaling](journaling.md#overnight-and-on-demand), as the help check decides help. A size passes
only when every check holds in every run, on a set of journal days.

| Check | Pass |
|---|---|
| Receipts | Every sentence carries a receipt to something kept that day; check-ins are not shown to the mind, so they are never cited there |
| No feeling without support | No feeling is written that no check-in or quote supports |
| No advice | No sentence tells the person what to do or how to feel |
| No invented people | No person or place appears that the day's items do not name |

The set and the scoring are in `src-tauri/tests/eval/journal.rs`: three seeded journal days, each
written three times. `cargo test -p nocturne-app --test journal` runs it on a scripted mind, which
checks the wiring and the scoring, not a size. A run on the real models of each size is not wired
yet; it will follow the help check's `make e2e-help`. No size has passed, so every size shows "At a
glance" and the line "On this computer, Nocturne lists the day rather than writing about it."

## The decision check

Nocturne asks a few typed questions about a short text and logs what a decider would answer beside
what today's rules and word lists did; nothing acts on the answer yet. Whether a decider may one
day act is decided by the check below, kind by kind. The kinds are: did a new note move or cancel a
kept appointment, is a note a health, money or identity detail, and has the person already
written how a planned thing went.

Each decision is asked of every arm, once, on the same seeded text:

| Arm | What it is |
|---|---|
| Rules | What the rules and word lists said, as logged when the note was kept |
| Mind | The mind Nocturne already loads, its single reply read as certain |
| Decision model | A model made for typed questions, chosen by hardware: `d3` on an NPU, Liquid `d1-Omni-2B` on a capable GPU; `d3-lite` on the CPU only when the run names it |

| Metric | Meaning |
|---|---|
| Accuracy | Verdicts that equal the label, over all cases |
| Losing errors | Verdicts on the side that loses something: a wrong move closes the kept appointment, a missed health detail could go online, a wrong "already written" drops the page question |
| Losing-error rate | Losing errors over the cases whose label is not the losing side |
| Band | Answers with no side at the kind's threshold, so no verdict |
| Errors | Failures and timeouts; the rule's outcome stands |
| Latency | Median and 95th percentile, in milliseconds |
| Overnight tidy | Still under 10 minutes with the decisions asked; not yet measured by the check |
| Forgetting precision | Unchanged; not yet measured by the check |

A kind may act only when its arm beats the rules and the mind on a real set of at least 100 cases
for that kind, each labelled by two people, with accuracy at least 5 points higher and no more
losing errors than the rule. Until then it stays in shadow. The gain looked for is mostly in other
languages, where English word lists break.

The set today has 17 moved, 18 sensitive and 16 followed-up cases, mostly English with some
German, Spanish, French and Dutch. It is made up and labelled by one person, so it checks that the
wiring and the scoring work and shows where the rules go wrong; it cannot promote anything. The
set and the scoring are in `src-tauri/tests/eval/decisions.rs`. Each case is kept and applied
through the engine, so the rule's outcome is the real one.

`cargo test -p nocturne-app --test decisions` runs it on scripted deciders, which checks the
wiring and the scoring, not a size. `make e2e-decisions` runs the mind arm and the
decision-model arm this computer's hardware picks (or the one `NOCTURNE_E2E_DECISION_MODEL` names),
and writes `decisions.json` with the numbers
per arm and kind. Decision-model arms run in the evaluation only: a model download for people
needs its own design and consent first.

An evaluation-only guard arm asks two yes or no questions of each sentence of the journal check's
seeded days and of a few bad ones: does it tell the person what to do or how to feel, and does it
name a feeling the day's items do not mention. It is scored the same way, and nothing of it is
logged or shown in the product.

## Online answers on the smallest tier (S1b)

Measured against a stand-in provider with a larger model, on the smallest tier, with the same
questions answered locally and through the online path. Answering through the online path scored a
median of 6 of 12 against 7 of 12 locally, and took about 3 s against 0.8 s to answer. No private or
sensitive test markers reached the stand-in provider. Online answers are not claimed to be better on
small computers; the value is that a person can choose a service they already trust, and the cost is
a slower answer.
