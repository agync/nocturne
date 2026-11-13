# Architecture

Nocturne has four loops: capture, consolidate, serve, and publish. Each loop has one job and one
failure mode.

```text
capture -> Omni (see, hear, read) -> event tape -> extract -> fact graph -> retrieve -> any app
                                                              |
                                                              +-> sleep (merge, contradict, decay)
                                                              +-> morning paper (print, audio, web)
```

## Capture

Inputs:

- A folder watcher, for pages and screenshots.
- A command line, for text and notes.
- A share sheet, for the phone.
- An MCP tool, for agents.

Each item becomes one event. The event stores the raw content, the source, the capture time, a
content hash, and metadata. The hash makes the tape idempotent: the same item twice is one event.

Images and audio go through Lemonade Omni first:

- `transcribe_audio` for voice notes.
- `analyze_image` for photos and screenshots.
- The text is stored in the event, and the original path is kept in metadata.

## Consolidate

The sleep cycle runs when the machine is idle and on power. It has four steps.

1. **Salience.** A small model on the NPU, or the GPU when the NPU is unavailable, labels each event:
   keep, skip, or ask. The NPU path is optional because the hardware and the FastFlowLM stack are not
   stable enough to depend on.
2. **Extraction.** A larger model on the GPU turns the kept events into facts. The schema is small:
   person, project, decision, preference, event, place.
3. **Merge and contradict.** A fact with the same subject, predicate, and object reinforces the
   existing row. A fact with the same subject and predicate but a different object is a change: keep
   both with valid time, and put the conflict in the paper.
4. **Decay.** Facts that nobody uses lose confidence. At 30 days they are fading. At 90 days they are
   archived. Archiving is reversible. Deleting is explicit.

## Serve

Two surfaces:

- **MCP tools**: `memory_search`, `memory_remember`, `memory_forget`, `memory_brief`.
- **OpenAI-compatible proxy**: a chat request is augmented with a small package of cited facts. The
  proxy is not built yet.

The package has a token budget. It carries the facts, their ids, their confidence, and their valid
time. The model is told to cite the ids. A citation that does not resolve is a failure.

## Publish

The paper has four sections: what changed, what needs you, what it forgot, one question. It is
rendered as Markdown, PDF, and speech. The printer is optional.

The paper is the product. The graph is the engine.

## Device placement

| Work | Device | Why |
|---|---|---|
| Chat and extraction | GPU | The large model needs the bandwidth and the KV cache |
| Salience and classification | NPU, when idle | The NPU is low power and does not contend with the GPU in the one test that held |
| Embeddings | GPU or CPU | FastFlowLM's Linux NPU embeddings are unusable in 1.0.3 and 1.0.4 |
| Store and orchestration | CPU | SQLite and the scheduler are cheap |

The NPU plus GPU path has a reported segfault, so the default is sequential: the NPU works when the
GPU is idle. The concurrent path is an experiment, not a dependency.

## Failure modes

| Failure | Symptom | Response |
|---|---|---|
| The model is offline | Extraction returns nothing | Keep the event, retry at the next sleep |
| The extraction is noisy | Too many low-confidence facts | Raise the threshold, ask the user, shrink the schema |
| The graph is poisoned | A web page injects a false fact | Quarantine untrusted sources, require corroboration |
| The paper is ignored | The user does not open it | Test a comic or an audio edition before building more |
| The archive grows | Slow search, large database | Optical compression, archival, or a smaller retention window |
