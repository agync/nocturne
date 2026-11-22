---
type: Design
title: Remote access
description: "How Away from home works, from the local listener to the limits."
status: draft
tags: [architecture, remote, privacy, phone]
---

# Remote access

Away from home lets a paired phone ask, keep and read today's page while the person is out. It is
off by default. This page describes the design in [decision 0010](/adr/0010-away-from-home.md). It
describes what is designed; real-network behaviour is not claimed until the owner-approved trials
have run.

## Off means off

With the switch off, Nocturne downloads nothing, starts no helper, binds no listener and makes no
connection for this feature, and no helper file exists unless a download was agreed to. The state
that says the person wants it on is stored, and at launch Nocturne starts the helper again only if
that state is on and automatic switch-off is not due. The release check proves the off case.

## Parts

- **The listener.** Local only: an owner-only Unix socket in an owner-only folder on Linux and
  macOS, loopback on Windows with a per-launch key carried in the address or a path prefix, never in
  a command line. It never serves the engine's port and never forwards to the engine. Requests
  without the key get a bare 404.
- **Helpers.** One supervisor runs either helper in a process group (a job object on Windows) with
  a scrubbed environment, its own home under the data folder, restart backoff and proof at stop that
  no process remains. Each helper is downloaded only after consent, checked against a pinned hash
  before every start, and never updates itself or uploads logs. Flags are used only if they appear
  in help text captured from the pinned binary.
- **Simple.** A Cloudflare quick tunnel to the listener. Its address changes each start.
  Cloudflare deletes a quick tunnel once all its connections drop, and the helper then retries the
  dead name forever. When the edge refuses a connection because the tunnel is gone and none is
  left, Nocturne stops the helper and starts a new tunnel, after a wait that grows with each
  renewal in a row, and shows the new address so each paired phone can be re-linked.
- **Most private.** `nocturne-tailnet`, a small helper Nocturne builds on Tailscale's `tsnet`
  library, joins the person's own tailnet and serves the listener over HTTPS on port 443 only. It
  admits only the person's own devices (same user, untagged, not shared in), forwards nothing to
  the computer's loopback, uploads no logs, and has no SSH, Taildrop, web client or Funnel. The
  supervisor talks to it in JSON lines and it reports states such as sign-in needed, approval
  needed and ready. It is built in CI and tested offline against an in-process control server.
- **The phone app.** A second, small web app served by the listener. It lives at the address the
  phone was paired on, caches only its own shell, and talks to the current address. The shell is
  served cache first and is not revalidated, so in Simple one altered response from the relay stays
  on the phone until it is added again (see below).

## Pairing

The computer offers one code at a time: single use, five minutes, 120 bits of secret. Scanned, it
is a link whose secret sits in the fragment and never leaves the phone's browser. Typed, it is 24
characters carrying the same secret, so both ways are one protocol. The phone commits to its name and
nonce, the computer replies with its own nonce, and the phone reveals within ten seconds or the
offer is spent. Both sides then derive three words that bind the address, both keys, the name and
both nonces. The person compares them and presses Allow on the computer. A frame that fails to
open is refused and noted in the activity list, and pairing frames are rate limited, but a wrong
guess never closes the offer. A refused or abandoned pairing leaves no device behind.

## The sealed channel

Each phone and the computer hold long-term P-256 keys; the phone's is not extractable. A session
begins with a proof that the phone holds its key, then the computer adds a fresh key of its own for
forward secrecy. Session keys come from four key agreements through HKDF-SHA256, one per direction,
and frames use AES-256-GCM with the header as authenticated data. Counters start at 1 and must
advance by exactly one; any gap, repeat or bad tag closes the session. Frames are never resent: after
a transport error the phone opens a new session and retries by request id. Session keys and counters
exist only in memory, one session per tab, and sessions last 15 minutes. Text frames are padded to
multiples of 256 bytes. The Rust and browser sides are both tested against the same reference
vectors made by an independent script.

## What a phone can do

Ten calls and two phone verbs, each a typed match: see readiness, keep text, keep a photo, document
or voice note, take back a recent keep, ask and cancel, read today's page and the last seven, and
open a cited original. The journal adds its own calls: read a day and a look back, keep a check-in,
answer "Which {name} is this?", and answer the page's question from the journal, which is kept once
per client id like a keep. Online answers adds one: say that the person has seen what changed
when only what they mark private began to be held back. A phone question that would go online
while that notice waits is not started; the phone shows the notice, and may ask again with the
computer chosen. A keep that waited on the phone says when it was made, and the computer
files it on that journal day, never in the future and never more than a week back. A phone cannot
delete, forget, export, change settings, pair or remove phones, or browse memories. Asks leave
private memories out, pages use the phone edition, and an outbound filter drops any payload naming
a private item.

## Limits

Separate rate buckets apply to requests without the helper key, pairing frames, failed session
opens, and each phone's opens, requests, asks and keeps. Text is capped at 100 KB, a question at
2 KB and an item at 50 MB, with 500 MB a day per phone. Uploads stream to an owner-only file, one
per phone, at most 100 MB of incoming files in all. Nothing polls faster than every 30 seconds while
no phone is connected. Removing a phone ends its open connections within a second.

## What is recorded

An audit trail records events such as pairing, removal and wrong codes, never content. Logs name
phones by an id prefix only. Secrets never appear in command lines, logs, events or addresses other
than the pairing fragment.

## What the channel does not hide

The relay sees a stable device id per phone, session ids, frame counts, sizes and timing, and can
drop or delay frames. In Simple, Cloudflare serves the phone page's code. The phone keeps the page,
so one altered response can stay until the phone is added again, and an altered page can use the
phone's key while it runs. After a computer restart the phone's home origin is a retired quick-tunnel
name that Cloudflare, or a later tenant of the name, answers. Revalidating the page or checking
hashes in the service worker would not help, because the same origin serves the worker and the
hashes, so this is a known limitation and a native phone app is the long-term fix. Most private
makes it much harder, since Tailscale would have to issue a certificate for the computer's name,
which shows up in public certificate logs. A typed address must be exact. The three words stop a
passive relay but not a look-alike site that has the typed code, and a leaked code is stopped only by
comparing them. An unlocked or stolen phone can read everything not marked private until it is
removed, and in Simple any local user on the computer can see the public address. After a new computer key or Delete everything, phones can only show "can't connect". With
Most private, only the person's own devices are admitted, and they still need a code and the
person's Allow to pair. The Tailscale helper binds UDP on every network interface while on. See the decision for the full list.
