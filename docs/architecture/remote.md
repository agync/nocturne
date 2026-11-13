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
- **Most private.** The official Tailscale daemon in userspace mode with the person's own
  tailnet, serving the listener over HTTPS. Funnel is never requested.
- **The phone app.** A second, small web app served by the listener. It lives at the address the
  phone was paired on, caches only its own shell, and talks to the current address.

## Pairing

The computer offers one code at a time: single use, five minutes, 120 bits of secret. Scanned, it
is a link whose secret sits in the fragment and never leaves the phone's browser. Typed, it is 24
characters carrying the same secret, so both ways are one protocol. The phone commits to its name and
nonce, the computer replies with its own nonce, and the phone reveals within ten seconds or the
offer is spent. Both sides then derive three words that bind the address, both keys, the name and
both nonces. The person compares them and presses Allow on the computer. Five frames that fail to
open pause pairing for 15 minutes. A refused or abandoned pairing leaves no device behind.

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
open a cited original. A phone cannot delete, forget, export, change settings, pair or remove
phones, or browse memories. Asks leave private memories out, pages use the phone edition, and an
outbound filter drops any payload naming a private item.

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
drop or delay frames. Cloudflare delivers the phone page's code, so an edge that alters it can use
the phone's key while the page runs. A typed address must be exact, and the three words help only if
compared. After a new computer key or Delete everything, phones can only show "can't connect". With
Most private, other devices on the same Tailscale account can reach the listener and try to pair, and
still need a code and the person's Allow. The Tailscale daemon binds UDP on every network
interface while on. See the decision for the full list.
