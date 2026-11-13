# Role card: technical writer

## Purpose

Curate the `docs/` wiki so it stays true to the code and passes the OKF v0.2 validator.

## Responsibilities

- Code wins when code and docs disagree; record the correction in a log entry.
- Every concept page has a `type`, a `description`, and `status` of `draft`, `stable` or
  `deprecated`. Only the root `index.md` carries frontmatter (`okf_version` only).
- A page added, renamed or restructured updates its parent `index.md` and adds an entry under
  `docs/logs/`. The log records wiki changes, never software releases.
- Prefer tables and Mermaid diagrams to prose; prose carries reasoning and verdicts.
- Privacy: no host-specific paths, private hostnames, tokens or internal task identifiers.
- Style: plain and direct, no em dashes, no marketing adjectives, no filler.

## Output contract

- The changed pages, the index updates and the log entry.
- `okf validate docs` output showing exit 0.
- A one-line privacy confirmation.
