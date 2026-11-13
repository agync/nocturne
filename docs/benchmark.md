# Benchmark

The benchmark answers one question: does the sleep cycle make memory better, or just bigger?

## Data

Thirty days of the owner's real captures: text notes, pages, photos, and voice notes. A held-out set
of one hundred questions with ground-truth answers and the source event ids.

The questions are written before the system is tuned. Otherwise the benchmark measures the tuning.

## Baselines

| Baseline | What it does |
|---|---|
| No memory | The model answers from the prompt only |
| Full context | Every event in the prompt, truncated to the model window |
| Naive RAG | Top-k chunks by embedding similarity |
| Flat vector memory | Top-k facts by embedding similarity, no graph |
| Graph | Facts with one-hop links, no sleep |
| Graph plus sleep | Merge, contradict, decay |
| Graph plus sleep plus decay | The full design |

## Metrics

| Metric | Definition | Target |
|---|---|---|
| Recall@5 | The source fact is in the top five | > 0.8 |
| Answer accuracy | The answer matches the ground truth | > 0.75 |
| Citation precision | The cited fact supports the answer | > 0.9 |
| Tokens per answer | Input plus output | < 2k |
| p50 and p95 latency | Wall time per answer | p95 < 3 seconds |
| Consolidation cost | Tokens, seconds, and watt hours per night | < 10 minutes |
| Forgetting precision | Archived facts the user wanted to keep | < 5 percent |
| Outbound bytes | Network traffic during a full cycle | 0 |

## Ablations

- No decay.
- No contradiction handling.
- No NPU salience, GPU only.
- No provenance, to show why provenance is load-bearing.
- No citations, to measure whether the paper is trusted less.

## Reporting

One table, one chart, one paragraph of failure analysis. Do not report a single score.

```text
| Run | Recall@5 | Accuracy | Citation precision | Tokens | p95 | Forgot wrong |
|---|---|---|---|---|---|---|
| No memory | 0.00 | 0.31 | n/a | 0.4k | 0.9s | 0 |
| Full context | 0.62 | 0.58 | n/a | 12.1k | 8.2s | 0 |
| Naive RAG | 0.41 | 0.49 | 0.71 | 2.3k | 1.9s | 0 |
| Graph | 0.67 | 0.66 | 0.84 | 2.1k | 1.8s | 0 |
| Graph plus sleep | 0.79 | 0.74 | 0.91 | 1.9k | 1.7s | 2 |
| Graph plus sleep plus decay | 0.81 | 0.76 | 0.92 | 1.8k | 1.6s | 3 |
```

The numbers above are placeholders. Replace them with captured output before publishing.

## What the benchmark does not cover

- Multi-user memory.
- Cross-device sync.
- Long-term drift beyond 30 days.
- Adversarial memory poisoning beyond the quarantine test.
- Energy use on a laptop battery.
