# Nocturne

A local AI that remembers the day, forgets what does not matter, and prints a one-page paper in the
morning.

Nocturne is a design for the Lemonade Challenge. This repository is the runnable scaffold, not the
finished app. The design dossier lives in the owner's investigations wiki at
`research/investigations/moonshots/nocturne/`.

## The bet

Most memory systems try to remember everything. Nocturne treats memory as a metabolism: facts stay
strong when they are used or confirmed, fade when they are not, and archive after 90 days. The paper
is the proof. It shows what changed, what needs you, what it forgot, and one question.

The AI sleeps at night. The NPU classifies what is salient, the GPU extracts facts, and the morning
paper is ready when you wake up. Everything runs on the machine.

## Status

Scaffold. What works today:

- A SQLite event tape and a fact graph with provenance, confidence, valid time, and status.
- Full-text search over facts.
- A decay and forget policy with tests.
- A Markdown morning paper.
- A Lemonade client for chat, speech, transcription, and images.
- A heuristic extractor that runs offline, plus a Lemonade extractor.
- A sleep cycle that turns events into facts and a brief.
- A minimal MCP stdio server.

Not built yet: the Omni ingest path for images and audio, the chat proxy, the web UI, the printer,
and the NPU worker.

## Quickstart

```bash
uv run pytest -q
uv run nocturne init
uv run nocturne remember "I prefer oat milk."
uv run nocturne sleep
uv run nocturne paper
```

The default database is under `~/.nocturne/`. Override it with `NOCTURNE_HOME` or `NOCTURNE_DB`.

Lemonade runs at `http://localhost:13305/api/v1` by default. Override it with `LEMONADE_BASE_URL` and
`LEMONADE_API_KEY`.

## Architecture

```text
capture -> Omni (see, hear, read) -> event tape -> extract -> fact graph -> retrieve -> any app
                                                              |
                                                              +-> sleep (merge, contradict, decay)
                                                              +-> morning paper (print, audio, web)
```

| Layer | Choice |
|---|---|
| Store | SQLite with FTS5 and a vector column reserved for later |
| Facts | Bitemporal subject-predicate-object with provenance and confidence |
| Cortex | A large model on the GPU through Lemonade |
| Hippocampus | A small model on the NPU, only when the machine is idle |
| Senses | Lemonade Omni for transcription, vision, speech, and images |
| Interface | Markdown, PDF, audio, a small web UI, and an optional thermal printer |

## The forgetting policy

| Signal | Action |
|---|---|
| Reinforced or repeated | Keep, raise confidence |
| Contradicted | Keep both with valid time, ask |
| Unreferenced for 30 days | Lower confidence, mark fading |
| Unreferenced for 90 days | Archive, do not delete |
| You say forget | Delete with cascade |
| Untrusted source | Quarantine, no promotion |

## Layout

```text
src/nocturne/
  store.py      event tape, fact graph, search, decay, forget
  models.py     dataclasses
  extract.py    heuristic and Lemonade extractors
  sleep.py      the nightly consolidation cycle
  paper.py      the Markdown paper and audio
  lemonade.py   OpenAI-compatible client
  cli.py        command line
  mcp.py        MCP stdio server
tests/          store, decay, paper, and sleep tests
docs/           architecture, data model, demo, benchmark, decisions
```

## Roadmap

1. Text-only paper by day 9.
2. Omni vision and speech, with citations.
3. The chat proxy and the MCP tools.
4. The NPU worker, measured against the GPU fallback.
5. The printer, the web UI, and the 30-day benchmark.

## License

Apache-2.0. See `LICENSE`.
