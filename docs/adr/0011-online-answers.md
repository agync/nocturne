---
type: Decision
title: "0011 Online answers, opt-in and through the engine"
description: "An opt-in way to let a provider the person chose write the last step of an answer, called by the embedded engine, with an exact log of what was sent."
status: draft
tags: [adr, online, privacy, engine]
---

# 0011 Online answers, opt-in and through the engine

Amended 2026-11-24 (owner): every answer sends today's date, time and time zone, and lines for the
days a question names; a mind may look further with read-only look-ups, up to 4 requests per
question, each counted towards the monthly limit
([0022 Ask looks things up](0022-ask-looks-things-up.md)).

Amended 2026-11-22: memories about health, money or ID are no longer kept back. The owner's rule is
that Nocturne never judges a person's data, so only what the person marks private is held back
([Only what you mark private is held back](/design/only-marked-private.md)).

Amended 2026-11-23 (owner): an answer online gets what a good answer needs, things marked private
included. Private decides only what comes up on its own (notifications, reminders and updates), and
never keeps a memory out of an answer the person asks for. The safeguard is a short, calm
explanation of what is sent at the moment the person sets up Online answers
([What private means](/design/only-marked-private.md#revised-2026-11-23-private-is-about-what-comes-up-on-its-own)).

## Context

A small computer answers slowly and sometimes weakly. A person who already has an account with a
service that writes answers may prefer it for the last step of Ask. Nocturne promises that nothing
leaves the computer, so this has to be off by default, small, bounded and provable, and it must
not make Nocturne depend on any one company.

## Decision

- **Opt-in and off by default.** Online answers is turned on only after a check of the person's own
  address, key and model name has passed, and then only after explicit consent. With it off, the
  engine has no provider, no route and no key.
- **One generic form.** The person enters an address, a key and a model name. There is no sign-in,
  no account with Nocturne, no list of providers and no provider-specific code (no balance call, no
  provider parameters). The interface names the provider by its host.
- **The engine makes the call.** Nocturne hands the address and key to `lemond`, and `lemond` calls
  the provider through Lemonade's experimental `cloud_providers` feature. The app has no HTTPS
  client to a provider.
- **Only the last step.** Only Ask can reach the route. Reading, the overnight tidy and the page never
  do. What is sent is the question and at most 12 remembered sentences. Journal sentences, read from entries and good things, may be among the 12; Ask does not search check-ins, reflections and recaps, so none is (amended 2026-11-24: a check-in's line may go, see [0022](0022-ask-looks-things-up.md)). Memories being forgotten or let go are never
  sent. Nothing else about a memory keeps it back: Nocturne does not judge whether it is about
  health, money, ID or anything else, and since 2026-11-23 a private mark does not keep it back
  either. Phones use the route only while Away from home is on and the person has left phones on in its settings.
- **Bounded.** One request per question, no retry, a per-month limit that applies only if the person sets one, and a
  fallback to answering on this computer. Every answer says where it was written. Nothing ever goes
  from the computer to online automatically, and an answer that came back "not sure" is not sent
  again.
- **An exact record.** A Sent online log keeps what was sent for 90 days. It is part of export and
  is removed by Delete everything together with the key.

## How the route is built

| Part | Choice |
|---|---|
| Provider entry | One entry in the engine's `config.json` and one fixed model row in its user models, written at launch from one function. With Online answers off, `cloud_providers` is an empty list, the row is removed and no key is in the environment. A launch for a download the person agreed to leaves the provider out, and online questions are answered on this computer until the download window closes |
| The key | After each engine start while Online answers is on, Nocturne loads the online entry, then hands the key to the engine in memory through its own cloud authentication endpoint on loopback, and confirms the engine holds it. Never argv, never the spawn environment, never a file the engine reads. Turning off, forgetting the key, Delete everything and an abandoned check clear it from the running engine at once, before the relaunch. If the engine refuses it or loses the route, nothing is sent, questions are answered on this computer, and "Try again" recovers. The key is kept in the system keychain, or in an owner-only file when there is none |
| Reach to the provider | While Online answers is on, the engine makes one model-listing request to the provider each time it starts, a cost the owner accepted for keeping the key out of the environment. Otherwise it reaches the provider only when Nocturne sends a question or the one check question. With Online answers off it never does |
| Requests | One non-streaming chat request per question with a short reply limit, no tools and nothing else. The check sends one fixed word-reply question and no memories |
| Addresses | A provider may be on this computer or the person's own network (loopback, private, shared 100.64/10 where tailnets live, unique-local IPv6, and the names `localhost`, `.local`, `.home.arpa`, `.internal`, `.ts.net` and any one-label name), over `https://` or plain `http://`, with no confirmation or warning; for plain HTTP there, and only there, the engine is allowed insecure HTTP. Anywhere else takes `https://` only. Never: user names, query strings, fragments, unspecified, link-local (including metadata addresses and names), multicast, broadcast, reserved, documentation ranges, site-local, NAT64 and 6to4, and Nocturne's own ports on this computer. Redirects are not followed, by the engine or by the app's own client |
| Logs | The engine's output goes through Nocturne's own pipe, which withholds provider error bodies (a multi-line body whole) and replaces any occurrence of the key, or of its first or last eight characters, with `[key]`. Replies are never logged |
| Core dumps | The whole app sets `RLIMIT_CORE` to 0 at start, and on Linux is also made undumpable, so no core file of the app, the engine or its children holds the key. The limit is set on the whole app, not on the engine alone, because the workspace forbids the `unsafe` code a per-spawn hook needs |

## Residual risks

These are accepted and stated, not hidden. The statements about the engine's own behaviour come from
its source at the pinned version, not from tests in this repository; the route probe at each engine
bump re-checks them.

- Anything holding the engine's per-launch key can add a provider, change an address or allow plain
  HTTP, because the engine has no switch against it. The key is treated as an admin credential, held
  only by the app and the engine, and the route exists only while Online answers is on.
- The engine's model servers (llama.cpp and the others) inherit its environment. The key is not in
  it, so they do not inherit the key. Code running as the same person in one of them could still
  read the engine's memory on Linux, where the engine is dumpable again after it starts.
- Anything holding the engine's per-launch key can also replace the key the engine holds. That holder
  could already change the address, so this adds nothing beyond the first point.
- A public name that resolves to a private address passes the address check as a public one, which
  must use `https://`, because Nocturne does not resolve names.
- A provider on the person's own network reached over `http://` gets the key and the questions in
  plain text on that network. The owner chose this on 2026-11-14 so a server at home, on a tailnet or
  on this computer works without certificates; such an address is the person's own.
- 100.64.0.0/10 is where tailnets live, and it is also carrier-grade NAT space: on a network without
  a tailnet, a plain `http://` provider there could be outside the person's own network, at their
  internet provider. The owner chose to allow it without a warning.
- A name on the person's own network (one label, `.local`, `.home.arpa`, `.internal`, `.ts.net`) is
  looked up at the check and again before each engine start hands over the key: every address it
  resolves to must be on this computer or the person's own network, or it counts as public and plain
  `http://` is refused; a name that resolves into a range that is never a provider is refused. The
  engine looks the name up again when it connects, so a change in DNS between that look-up and a
  request is not seen. A name that does not resolve at the check keeps its class.
- The engine's trace streams on loopback need no key. Telemetry is off, so they carry nothing.
- The engine does not use system proxy settings (the environment is scrubbed) and trusts the TLS
  roots bundled with its HTTP library. Nocturne cannot check what the provider keeps or for how long,
  and says so on the consent screen.
- Windows is untested for the core-dump measure; Windows writes no core files by default.
- The provider sees one model-listing request, with the key and no question or memory, each time the
  engine starts while Online answers is on.
- The engine's environment, which system crash records can hold, never holds the key; it is in the
  engine's memory only. Rotate the key at the provider if you think it was exposed.
- Anything the person kept may go when it answers a question, whatever it is about and whether or
  not it is marked private (since 2026-11-23). Only memories the person let go or is forgetting,
  and ones the store does not know, are held back.
  Until 2026-11-22 a check left out memories that looked like health, money or ID details unless
  the person allowed them; it was removed because it judged the person's data, and a best effort
  that could miss some promised more than it held.

## Consequences

- Invariant 2 gains a second exception, described in the architecture notes.
- The release check has an `online` network mode (`make netcheck-online`) that allows a non-loopback
  connect only from the engine, only to the entered address, only during questions and the start-up
  listing, and never after turning off ([releasing](/contribution/release.md)).
- A change to Lemonade's `cloud_providers` behaviour or its cloud authentication endpoint is a release blocker until the route is probed again.
- Whether providers behave as expected over the real internet is not claimed until an owner-approved trial.
