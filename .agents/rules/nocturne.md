# Project rules: Nocturne

These extend `AGENTS.md`.

## Product

1. **The person comes first.** When capability and clarity conflict, clarity wins. Hide power behind
   an advanced details view rather than removing it.
2. **Capture never fails.** Capture is accepted and stored even when Lemonade is down, a model is
   still downloading, or the disk is nearly full (warn, do not drop).
3. **Every claim has a receipt.** Answers and page lines carry fact ids that resolve to the original
   event. A citation that does not resolve is a bug.
4. **Untrusted input is quarantined.** Pasted web content and unknown sources never create active
   facts without a second source or a confirmation.

## Runtime

1. **The interface never calls Lemonade.** Only the engine talks to `lemond`.
2. **One private `lemond`.** Loopback, random port and key per launch, discovery off, `models_dir`
   inside app data. Never connect to, reconfigure or stop a system Lemonade.
3. **Clean shutdown.** `POST /internal/shutdown`, then terminate, then confirm no backend children
   remain. Tests assert it.
4. **Give memory back.** Unload models when idle; the person's other apps come first.
5. **Hermetic tests.** Tests start their own `lemond` on a random port under a scrubbed environment,
   or use a fake; they never read the person's home directory or a system Lemonade.
