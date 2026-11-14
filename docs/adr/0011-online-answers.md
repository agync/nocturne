---
type: Decision
title: "0011 Online answers, opt-in and through the engine"
description: "An opt-in way to let a provider the person chose write the last step of an answer, called by the embedded engine, with an exact log of what was sent."
status: draft
tags: [adr, online, privacy, engine]
---

# 0011 Online answers, opt-in and through the engine

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
  do. What is sent is the question and at most 12 remembered sentences. Private memories are never
  sent. Memories about health, money or ID are left out unless the person turns that on, which is
  off by default. Phones use the route only while Away from home is on and the person has left phones on in its settings.
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
| The key | Handed to the engine at run time through its own cloud authentication endpoint on loopback, after each start while Online answers is on. Never argv, never the spawn environment, never a file the engine reads. Turning off relaunches the engine without it. The key is kept in the system keychain, or in an owner-only file when there is none |
| Reach to the provider | While Online answers is on, the engine makes one model-listing request to the provider each time it starts, a cost the owner accepted for keeping the key out of the environment. Otherwise it reaches the provider only when Nocturne sends a question or the one check question. With Online answers off it never does |
| Requests | One non-streaming chat request per question with a short reply limit, no tools and nothing else. The check sends one fixed word-reply question and no memories |
| Addresses | Release builds accept only `https://` addresses with a host, and refuse user names, query strings, fragments, loopback, link-local (including metadata addresses), unspecified, private network, shared, multicast, reserved, unique-local, site-local, NAT64 and 6to4 addresses, and the names `localhost`, `.local`, `.home.arpa`, `.internal`, `.ts.net` and any one-label name. Redirects are not followed, by the engine or by the app's own client |
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
- A public name that resolves to a private address passes the address check, because Nocturne does
  not resolve names.
- The engine's trace streams on loopback need no key. Telemetry is off, so they carry nothing.
- The engine does not use system proxy settings (the environment is scrubbed) and trusts the TLS
  roots bundled with its HTTP library. Nocturne cannot check what the provider keeps or for how long,
  and says so on the consent screen.
- Windows is untested for the core-dump measure; Windows writes no core files by default.
- The provider sees one model-listing request, with the key and no question or memory, each time the
  engine starts while Online answers is on.
- On Linux, `systemd-coredump` records the environment of a crashing process in the journal even
  when no core file is saved. Putting the key in the engine's spawn environment would have left it
  there after an engine crash, beyond the reach of "Forget the key". The owner chose to hand the key
  over at run time instead (route B), at the cost of the listing request above; the alternative of
  the environment is recorded as the rejected option. Rotating a key at the provider remains good
  practice after any suspected exposure.
- Sensitive details (health, money and ID signals, checked in every version of a memory) are left
  out unless the person allows them. The detection is being reworked; this page describes the
  behaviour, not the patterns, and the filter is a best effort, not a guarantee.

## Consequences

- Invariant 2 gains a second exception, described in the architecture notes.
- The release check is to gain an `online` network mode that allows a non-loopback connect only from
  the engine, only to the entered address, only during questions and never after turning off. It is
  not written yet ([releasing](/contribution/release.md)).
- A change to Lemonade's `cloud_providers` behaviour or its cloud authentication endpoint is a release blocker until the route is probed again.
- Whether providers behave as expected over the real internet is not claimed until an owner-approved trial.
