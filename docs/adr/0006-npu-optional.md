---
type: Decision
title: "0006 The NPU reads recordings by default"
description: "A usable NPU reads recordings when its model fits; every other job stays on the GPU, and any NPU failure falls back to the GPU."
status: draft
tags: [adr, npu, lemonade, runtime]
---

# 0006 The NPU reads recordings by default

## Context

The owner decided that the NPU is used by default when the computer has one, and when the GPU is
busy. The pinned engine (Lemonade v2026.46.2) serves the AMD XDNA 2 NPU through FastFlowLM
(`flm`, backend `npu`, v1.0.11). On Linux it installs a portable FastFlowLM build with its own
runtime libraries into its cache, so nothing needs installing with elevation; the computer still
needs the `amdxdna` kernel driver, NPU firmware and a high enough memory lock limit. The earlier
version of this decision cited an NPU plus GPU crash report that has no source.

## Decision

Per job, from what the engine's catalogue offers on the NPU and what was measured:

| Job | Where | Why |
|---|---|---|
| Reading recordings | NPU, `whisper-v3-turbo-FLM` | Same model as the Large tier's and stronger than the Base and Small models of the other tiers; reading a recording beside an answer slows the answer far less (below) |
| Answering and reading items | GPU | FastFlowLM ignores the reading grammar without saying so; an NPU chat model takes the engine's one chat slot and evicts the GPU mind; no NPU model has been scored on Nocturne's answers |
| Finding memories | GPU | The one NPU model (`embed-gemma-300m-FLM`) has no GPU twin, so a fallback would mix two kinds of vector; changing it means reading every memory again |
| Reranking, reading aloud | GPU, CPU | The catalogue has no NPU model for either |

- **When it is used.** The engine reports the NPU available and the `flm:npu` backend installed,
  the person has not chosen a speech model, and the tier's busiest moment with the NPU model in
  place of its own speech model still Fits the memory budget (`fit::fit_for`). Otherwise
  recordings are read exactly as before.
- **GPU busy.** Reading recordings is the only job that can move, and it stays on the NPU whether
  the GPU is busy or not, so it never queues behind the overnight reading or an answer. No job
  moves to the NPU only while the GPU is busy: moving answers would need a second chat slot for
  every kind of model and an answer set scored on an NPU model, which this decision does not take.
- **Fallback.** A failed NPU load or request is read again by the tier's own speech model; when
  that succeeds, the NPU is left alone until the engine next starts. The person sees nothing. The
  engine's reason is kept for the advanced details.
- **No NPU.** Nothing changes, and nothing extra is downloaded.
- **Downloads.** Setup fetches the NPU runtime (41 MB) and the model (0.62 GB) only when they will
  be used and fit. Both are optional: if either cannot be fetched the tier is installed without
  them. An install from before this decision picks them up at its next setup.
- **When it cannot be used.** The advanced details name the NPU and give the engine's reason and
  the steps. Elsewhere, a quiet note that can be dismissed appears in Settings only when the
  person can fix the cause (driver, kernel, firmware, memory lock limit).

## Evidence

Measured on a Ryzen AI 9 HX 470 (XDNA 2, Radeon 890M, kernel 7.3, 128 GB), one model run at a
time, a 47-second spoken note with 27 checkable names, dates, times and sums, and Qwen3.5-4B (the
Small tier's mind) answering on the GPU:

| Recording read by | Facts kept | Read alone | Answer alone | Answer while reading |
|---|---|---|---|---|
| Whisper-Base, GPU (Small, Tiny) | 26 | 1.5 s | 2.4 s | 2.9 s |
| Whisper-Large-v3-Turbo, GPU (Large) | 26 | 4.4 s | 2.3 s | 4.2 s |
| whisper-v3-turbo-FLM, NPU | 26 | 12.5 s | 2.2 s | 2.4 s |

The NPU text matched the GPU turbo model's word for word and punctuated better than Base. No
errors in any run. The NPU model holds about 1.0 GB while loaded (0.3 GB in its process and
0.7 GB of NPU buffers), against 0.37 GB for Whisper-Base.

## Consequences

- On 8 GB computers (Tiny) the NPU model does not fit beside the mind and recall (5.9 GB against
  a 5.6 GB Fits line), so Tiny keeps its own speech model. Small fits from about 15 GiB of
  memory; Standard and Large fit.
- Reading a recording takes longer (about a quarter of its length) because the NPU model is
  larger. This is background work.
- Risks: the Linux NPU stack is young (kernel 7.0 or later, or a DKMS driver, and firmware 1.1.0.0
  or later); many systems set an 8 MB memory lock limit, which the NPU refuses; one more process
  while a recording is read, unloaded after 120 seconds idle; a first-run download of 0.66 GB
  more. Both turbo models, GPU and NPU, dropped repeats from a recording of one sentence said ten
  times; Base did not.
