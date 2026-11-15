---
type: Decision
title: "0014 The NPU takes the mind's work"
description: "On computers with an XDNA 2 NPU, Gemma e4b on the NPU reads beside the GPU mind where both fit, and reads and answers alone where only one fits."
status: draft
tags: [adr, npu, lemonade, mind]
---

# 0014 The NPU takes the mind's work

## Context

[0006](0006-npu-optional.md) moved reading recordings to the NPU and left the mind (reading items
and answering) on the GPU for three reasons: FastFlowLM ignores the reading grammar, the engine's
one chat slot is shared by GPU and NPU models, and no NPU model had been scored on Nocturne's
answers. The owner wants the NPU used where it is available, to take load off the GPU and leave it
free for answers, and used first where memory is tight. The pinned engine (Lemonade v2026.46.2,
FastFlowLM v1.0.11) offers Gemma, Qwen and Llama chat models on the NPU. This decision scores them
and places the best one, `gemma4-it-e4b-FLM` (9.1 GB download, 9.8 GB loaded), called the NPU's
mind below.

## Decision

`npu::placement` picks one of three placements at each mind start, from the selection, the memory
and the graphics:

| Placement | NPU | GPU | When |
|---|---|---|---|
| Split | reads items: notes, second looks, photos | answers and writes the headline | the busiest moment with both minds loaded Fits; on Large, within the budget (Tight included) |
| NPU only | reads, answers and writes the headline | nothing; the GPU mind loads only when the NPU fails | shared memory, Split does not fit, the NPU's mind in place of the GPU mind Fits, and the tier is not Large |
| GPU | nothing of the mind's | everything, as before | otherwise, when the person chose a mind, and on any computer without a usable NPU |

- **Memory by device.** On shared memory (Strix Point, Strix Halo: the NPU and the integrated
  graphics both use the computer's memory) every model is counted against the one budget
  (`fit::budget`). With a graphics card of its own that holds the GPU mind and the recall model,
  those live in its memory, and only the NPU's models, the voice and a fallback speech model are
  counted against the computer's memory. A card too small for the mind counts as shared memory.
- **Large keeps its mind.** Its GPU mind (Qwen3.6-35B) is several times larger than the NPU's
  mind and was not scored against it, so Large is never NPU only. The NPU reads beside it whenever
  both are within the budget, past the Fits line the other tiers keep to, and otherwise Large
  stays on the GPU.
- **Reading by schema.** FastFlowLM ignores a grammar without saying so but honours
  `response_format` with a JSON schema by constrained decoding, so every reading request to an NPU
  model carries the reading schema from the first request.
- **Two chat models at once (Split).** Nocturne sets `max_loaded_models` to 2, through
  `/internal/set` at once and in `config.json` at each launch, so loading the NPU's mind does not
  evict the GPU mind (see Eviction). If the engine does not take the setting, the placement falls
  back to GPU. NPU only and GPU keep 1, so a fallback load of the GPU mind evicts the NPU's mind
  and the two never sit together where they do not fit.
- **Fallback.** Work the NPU's mind cannot do (a load that fails, a timeout, an error, an
  unusable reading after the existing retries) is done by the GPU mind, loaded on demand in NPU
  only. In Split, an empty reading is read again by the GPU mind and counts as a failure only when
  the GPU mind finds something. Three failures in a row leave everything to the GPU mind until
  Nocturne restarts, and the engine's next launch holds one model of each kind. A failed load
  makes the engine unload every model and try again (`src/cpp/server/router.cpp:1190-1232`), so
  Nocturne loads the GPU mind afresh with its own context. The NPU's mind is never fetched on use.
  The person sees nothing.
- **Downloads.** Setup fetches the NPU's mind as an optional step, with the NPU runtime, only
  where the placement is Split or NPU only, and leaves it out when the disk has room for the tier
  but not for it.

## Eviction

What the pinned engine (v2026.46.2) does, with lines in its source:

- **By count, whatever the memory.** Every local load calls `ensure_residency_capacity`
  (`src/cpp/server/router.cpp:1077`), which evicts the least recently used model of the same pool
  when the pool holds `max_loaded_models` (`router.cpp:351-369`). A pool is a model type (LLM,
  embedding, transcription, ...) across every device
  (`src/cpp/include/lemon/model_residency.h:56-68`, `router.cpp:297-317`); only cloud models are
  left out. The default is 1 (`src/cpp/resources/defaults.json:60`), so with 1 an NPU chat model
  evicts the GPU mind even when both fit.
- **The NPU's own rule.** FastFlowLM models coexist one per type: loading one evicts the NPU model
  of the same type (`router.cpp:1022-1065`).
- **By memory pressure, on graphics memory only.** With `auto_evict` on (Nocturne sets it), a
  monitor polls graphics memory every 2 seconds (`src/cpp/server/global_vram_monitor.cpp:41-58`)
  and, at or above `auto_evict_threshold_pct` (0.9, `src/cpp/resources/defaults.json:12`), evicts
  one idle model, the one idle longest relative to its load time, on any device
  (`src/cpp/server/eviction_engine.cpp:38-48`, `:139-152`).
  Graphics memory is read from `nvidia-smi`, the amdgpu VRAM counters, or on an AMD APU the VRAM
  and GTT together (`src/cpp/server/system_info.cpp:4182-4299`); system memory and the NPU's
  buffers are not watched, and a Mac is not watched at all.
- **By idle time.** A model idle for its `evict_idle_timeout` (300 seconds unless saved) is
  evicted (`src/cpp/server/eviction_engine.cpp:122-129`).

So the engine does not keep both models loaded just because they fit: the count decides first, and
memory pressure is a late safety net on graphics memory alone. Nocturne raises the count only in
Split, and its own budget remains what keeps the computer's memory from running out.

## Coverage

From `npu::placement` with each tier's own models (memory as the system reports it, in GiB):

| Tier | GPU | NPU only | Split |
|---|---|---|---|
| Tiny (from 6) | below 22 | 22 to 28 | 29 and up |
| Small (from 14) | below 22 | 22 to 34 | 35 and up |
| Standard (from 28) | none | 28 to 44 | 45 and up |
| Large (from 60) | 60 to 68 | never | 69 and up |

A tier's own range ends where the next tier begins, so in practice: 8 and 16 GB computers keep
the GPU; a 32 GB laptop (Standard) is NPU only; a 48 GB one is Split; 64 GB (Large) keeps the GPU;
from about 72 GB it is Split. With a graphics card that holds the mind, Standard at 28 GiB with a
12 GB card is Split; Small at 16 GiB with an 8 GB card keeps the GPU (the NPU's mind, speech and
voice need 14.6 GB against a 9.6 GB Fits line).

qwen3.5-4b-FLM (8.1 GB loaded) would not bring a 16 GB computer in: Small needs about 18.4 GiB
with it, and it scored lower.

## Evidence

Measured with the real-model evaluation (`make e2e-real`, one seeded day, 31 items), one run at a
time, each NPU model doing all of the mind's work.

On a Ryzen AI 9 HX 470 (XDNA 2, Radeon 890M):

| Mind | Answers (of 12) | Held out (of 10) | Facts kept (of 42) | Citation precision | Reading 31 items | Answer p50 / p95 | Peak memory |
|---|---|---|---|---|---|---|---|
| Qwen3.5-2B, GPU (Tiny) | 11 to 12 | 6 to 8 | 41 | 1.0 | 276 to 309 s | 1.1 / 2.1 s | 7.7 GB |
| qwen3.5-2b-FLM, NPU | 7 | 5 | 21 | 0.83 | 613 s | 1.5 / 2.2 s | 7.4 GB |
| Qwen3.5-4B, GPU (Small) | 12 | 9 | 42 | 1.0 | 169 s | 1.9 / 2.4 s | 10.1 GB |
| qwen3.5-4b-FLM, NPU | 9 | 7 | 36 | 0.86 | 643 s | 3.4 / 5.2 s | 10.8 GB |

On a Ryzen AI Max (XDNA 2):

| Mind | Answers (of 12) | Held out (of 10) | Facts kept (of 42) | Citation precision | Reading 31 items | Answer p50 / p95 | Peak memory |
|---|---|---|---|---|---|---|---|
| Qwen3.5-4B, GPU (Small) | 11 | 10 | 40 | 1.0 | 96 s | 1.2 / 1.6 s | 10.0 GB |
| gemma4-it-e2b-FLM, NPU | 10 | 7 | 36 | 0.93 | 348 s | 2.2 / 3.3 s | 10.8 GB |
| gemma4-it-e4b-FLM, NPU | 12 | 9 | 42 | 1.0 | 598 s | 3.8 / 5.9 s | 13.4 GB |

- e4b kept every fact, cited precisely, gave the changed appointment's new value with its history,
  and read the photo (3 of 3 checked words). Its process holds about 9.8 GB.
- The Qwen NPU models lost facts, and the 4B one gave an old appointment as current and could not
  read the photo; e2b lost the changed appointment's history.
- Every NPU model reads at about 20 seconds per item: e4b is six times slower than the GPU mind.
  Reading is background work, so the GPU's time goes to answers instead.

## Consequences

- Answers on an NPU-only computer come from the NPU's mind: the same scores as the Small GPU mind
  or better, but slower (3.8 s p50 against 1.2 s) and from a smaller model than Standard's
  Qwen3.5-9B, which was not scored against it.
- Reading takes several times longer on the NPU; it is background work, and the GPU stays free.
  In Split an item with nothing to remember is read twice, once on each mind.
- In Split, two models of every kind may stay loaded: a GPU speech model beside the NPU's after a
  fallback is counted in the budget, and the change preview counts the NPU's mind, that speech
  model and, during a re-read, both recall models.
- Caveat: the schema puts keys in sorted order, while the GPU path keeps the grammar's order with
  the quote first; e4b kept every fact regardless.
