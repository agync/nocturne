---
type: Decision
title: "0016 Decision models, shadow first"
description: "Ask typed questions beside today's rules and word lists, logged and never acting, first of the mind Nocturne already loads; decision models run in the evaluation only until they beat both."
status: stable
tags: [adr, decisions, models, evaluation, npu]
---

# 0016 Decision models, shadow first

Status: accepted. Date: 2026-11-21. Amended 2026-11-22: the health, money and ID question is
removed with the check it shadowed, because Nocturne never judges a person's data
([Only what you mark private is held back](/design/only-marked-private.md)).

## Context

Nocturne decides many small things with hand-written rules and English word lists: whether a note
moved or cancelled a kept appointment, and whether the person already wrote how a planned thing
went. Until 2026-11-22 it also decided whether a memory was a health, money or ID detail. The word lists break in other languages
and miss plain wordings ("feeling low, seeing a counsellor").

A decision model takes a short text and a typed question (yes or no, one of a list, a score) and
returns a probability for each answer, with no generated text. Lemonade serves them on
`POST /v1/systemone`: on llama.cpp from v2026.46.2, and also on the NPU through FastFlowLM from
v2026.47.1. In a 44-case synthetic trial, the chat model Nocturne already loads did about as well as
small decision models, and the decision model small enough for an 8 GB computer did worst. The gain
is replacing word lists with questions; a second model is optional.

## Decision

- **A decider seam.** `nocturne_engine::decide` holds the typed question, the answer with its
  probabilities and the `Decider` trait. The first decider is the mind Nocturne already loads,
  asked with a fixed list of answers (`Mind::decide`, on the model that reads items). A decision
  model behind `/v1/systemone` (`SystemOneDecider`) answers through the same seam.
- **Shadow only.** The rules still act. What they said is logged beside what the decider said, in
  a local, derived decision log that goes with the items and memories it is about and with Delete
  everything. Two kinds are asked: a moved or cancelled appointment, and "already wrote how it
  went" for the page question. A third, the health, money and ID check, was asked until
  2026-11-22 and is gone with the check: Nocturne does not ask whether a memory is about health,
  money or identity.
- **Overnight only.** The night's tidy asks once its page is saved, while nobody is at the
  keyboard and the computer is on mains, stopping well before the page time, bounded in count
  and time. An error or a timeout is logged and the rule's outcome stands; three failures in a row
  end the night's asking. Reading is never skipped. A decision is never a strike against the NPU's
  mind, never loads another model in its place and never fetches one; it may load the model that
  reads again if the engine had let it go.
- **Thresholds in code**, per question and per backend, since each backend's probabilities have
  their own calibration.
- **Picked by hardware.** `fit::decider_for` names the model that would serve this computer: `d3`
  through FastFlowLM on a usable NPU, Liquid `d1-Omni-2B` on llama.cpp with a capable graphics
  backend, otherwise the mind. Decision models load in Lemonade's `classification` pool, so they do
  not evict the mind, but their memory is counted in the reading moment of the budget.
- **Decision models in the evaluation only.** The decision check (`evaluation.md`) compares the
  rules, the mind and the hardware's decision model on a labelled set: accuracy, errors on the side
  that loses data, and latency. Nothing is downloaded for people.

## Consequences

- Nothing a person sees changes.
- A kind may act only when it beats the rules and the mind on a real, doubly labelled set, with
  no more errors on the side that loses data, and that change goes through design first.
- Offering a decision model to people needs a download with consent and a size line, so it needs
  design first: the only network traffic allowed is a consented model or app download.
- `/v1/systemone` is experimental upstream; Nocturne pins the engine and tests against
  `fake-lemond`.
- Lemonade's internal backend ports are by design; a decision model is one more loopback backend
  behind the engine's key.
