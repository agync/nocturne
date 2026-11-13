# Nocturne

A private desktop app that remembers your day, forgets what does not matter, and hands you one page
each morning.

Drop in a photo of a letter, a voice note, a screenshot or a line of text. Overnight, Nocturne reads
it, links it to what it already knows, notices what changed, and lets go of what you no longer use.
In the morning you get one page: what changed, what needs you, what it let go of, and one question.
Ask it anything and it answers with the original photo or recording as proof.

Everything runs on your computer through an embedded [Lemonade](https://github.com/lemonade-sdk/lemonade)
server. No account, no cloud.

## Status

Pre-implementation. The product and interface design is in progress; see `docs/` for the concept,
architecture and decisions.

## Development

```bash
.agents/bootstrap.sh
make check
```

## License

Apache-2.0. See `LICENSE`.
