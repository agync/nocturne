---
type: Decision
title: "0010 Away from home, opt-in and sealed"
description: "An opt-in way to ask and keep from a paired phone through Cloudflare or Tailscale, with traffic sealed between the phone and the computer."
status: draft
tags: [adr, remote, privacy, phone]
---

# 0010 Away from home, opt-in and sealed

## Context

People want to ask and keep from their phone when they are out. Nocturne promises that nothing
leaves the computer, and its audience cannot set up accounts, domains or networks. Any way in from
outside is a risk to that promise, so it has to be off by default, small, and provable.

## Decision

- **Opt-in and off by default.** With Away from home off, nothing is downloaded, started, bound or
  connected, and no helper file exists unless the person agreed to a download.
- **A separate, allowlisted listener.** It is local only: an owner-only Unix socket in an owner-only
  folder on Linux and macOS, loopback on Windows with a per-launch key. It is never the engine's
  port and never a proxy to the engine. It answers ten calls and two phone verbs, each a typed
  match, never a generic forwarder.
- **Two connections, one sealed channel.** Simple is a Cloudflare quick tunnel, labelled a trial by
  its maker, and is the default. Most private is a small helper Nocturne builds on Tailscale's
  embeddable library, on the person's own tailnet only. Never Cloudflare named tunnels or Access, a project-run tunnel,
  Tailscale Funnel, or a listener on any network interface.
- **Nocturne's own tailnet helper, not the official daemon.** Nocturne builds `nocturne-tailnet`
  from `helpers/tailnet`, a small program on Tailscale's `tsnet` library (tailscale.com v1.104.6).
  The first draft of this decision chose the official `tailscaled` in userspace mode. That was
  reversed once it was built, for two reasons: userspace `tailscaled` forwards tailnet traffic that
  nothing handles to 127.0.0.1, which would expose the computer's own loopback services to the
  tailnet, and shields-up, the setting that stops that, would also drop the serve traffic Most
  private depends on. The helper serves only port 443, admits only the person's own devices (the
  same user, untagged, not shared in from elsewhere), never forwards to localhost, uploads no logs,
  and is built without SSH, Taildrop, the web client and Funnel. One supervisor still runs both
  helpers. The helper is built for all four platforms reproducibly in CI (two builds must match)
  and tested offline against an in-process control server. Upstream fixes now arrive by moving the
  library version and rebuilding, which is a cost this choice accepts.
- **Pairing by scanning or typing.** The computer shows a one-time code (a link in a QR code, or 24
  typed characters carrying the same secret). The phone and computer each show three words, and
  the person compares them and presses Allow on the computer.
- **Per-phone keys, sealed traffic.** Each phone has its own key. Traffic is sealed between the
  phone and the computer whatever the connection carries it, with strictly counted frames and
  session keys that exist only in memory. [Remote access](/architecture/remote.md) has the detail.
- **Home origins and re-link.** The phone app lives at the address it was paired on, set by the
  computer. A Simple restart changes the address, so the computer offers a re-link code that a
  paired phone scans from inside the app, or four words it can type.
- **Switching unpairs.** Changing between Simple and Most private removes every phone, because
  their home origins belong to the other connection. The screen says so first.
- **Little from afar.** A phone can ask, keep text, a photo, a document or a voice note, take back
  what it just kept, read today's page and the last seven, and open an original that something it
  was shown cited. It cannot change settings, forget, export, pair or browse.
- **Nothing private reaches a phone.** Asks leave private memories out, the page uses a phone
  edition, originals of private items are refused, and an outbound filter drops any payload naming
  a private item.
- **Invariant 2 changes once.** It now allows sealed traffic to the person's own paired phones
  through the connection they chose, plus that connection's own traffic to its company's servers
  and relays to stay connected, only while on.

## What the channel does not hide

- The relay (Cloudflare's edge, or Tailscale's relays when a direct path is not possible) sees a
  stable device id per phone, session ids, how many frames are sent, their sizes (text is padded to
  multiples of 256 bytes, nothing else is) and their timing. It can drop or delay traffic or block
  pairing.
- Cloudflare delivers the phone page's code. An edge that alters that page can use the phone's key
  while the page runs. A native phone app would close this and is deferred.
- A typed address must be exact. The three words catch a wrong host only if the person compares
  them. Pressing Allow without comparing defeats them if someone at the relay also has the code, for
  example from a photo of the screen.
- After a new computer key or Delete everything, phones can only show "can't connect", because no
  sealed notice can reach them.
- With Most private, only the person's own devices on their tailnet are admitted, and they still
  need a code and the person's Allow to pair. Devices of other users, tagged devices and devices
  shared in are refused before any page is served.
- The Tailscale helper binds UDP on every network interface while on (its connection layer,
  magicsock, does this), and may cause a firewall prompt on macOS and Windows. The feature as a whole is not loopback only.
- The phone's offline queue of waiting keeps is stored on the phone, in its own storage, and is not
  described as protected at rest.

## What the companies see

Cloudflare sees the phone's internet address and rough place, when it connects, the size of
everything sent each way, and that it delivers the phone page. It does not see content. Tailscale
sees the account, the machines on it and when they connect, and carries traffic when no direct path
exists. It does not see content. Each helper also makes its own upkeep traffic to its company while
on. Helpers are pinned by hash, never update themselves and upload no logs. A tailnet that requires
log upload is refused with a plain message, because the helper never uploads logs.

## Consequences

- The Privacy screen names the company in the middle and counts bytes sent to phones.
- Helpers are pinned, checked before every start and refreshed at least every six months.
- The release check gains an off-means-off proof and an on-only-from-helpers check.
- Simple depends on a free service its maker labels for testing, so the feature degrades to a plain
  sentence when it fails.
- Real-network behaviour, including how long a quick tunnel stays up and how the helpers behave on
  each system, is not claimed until the owner-approved trials run.
