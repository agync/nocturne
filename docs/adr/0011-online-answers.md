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
  off by default. Phones use the route only while Away from home is on and the person allowed it.
- **Bounded.** One request per question, no retry, a per-month limit (200 questions by default) and a
  fallback to answering on this computer. Every answer says where it was written. Nothing ever goes
  from the computer to online automatically, and an answer that came back "not sure" is not sent
  again.
- **An exact record.** A Sent online log keeps what was sent for 90 days. It is part of export and
  is removed by Delete everything together with the key.

## How the route is built

| Part | Choice |
|---|---|
| Provider entry | One entry in the engine's `config.json` and one fixed model row in its user models, written at launch from one function. With Online answers off, `cloud_providers` is an empty list, the row is removed and no key is in the environment. A launch for a download the person agreed to leaves the provider out, and online questions are answered on this computer until the download window closes |
| The key | In `lemond`'s spawn environment only. Never argv, never a file the engine reads, never an API call. An environment key outranks any key given at run time, so a process holding the engine's own key cannot replace it. Turning off relaunches the engine without it. The key is kept in the system keychain, or in an owner-only file when there is none |
| `offline` | Stays on whether or not Online answers is on, so the engine never looks up the provider's list of AIs. It reaches the provider only when Nocturne sends a question or the one check question |
| Requests | One non-streaming chat request per question with a short reply limit, no tools and nothing else. The check sends one fixed word-reply question and no memories |
| Addresses | Release builds accept only `https://` addresses with a host, and refuse user names, query strings, loopback, link-local (including metadata addresses), unspecified, private network, unique-local and `.local` addresses. Redirects are not followed |
| Logs | The engine's output goes through Nocturne's own pipe, which withholds provider error bodies (a multi-line body whole) and replaces any occurrence of the key with `[key]`. Replies are never logged |
| Core dumps | The whole app sets `RLIMIT_CORE` to 0 at start, and on Linux is also made undumpable, so no core file of the app, the engine or its children holds the key. The limit is set on the whole app, not on the engine alone, because the workspace forbids the `unsafe` code a per-spawn hook needs |

## Residual risks

These are accepted and stated, not hidden.

- Anything holding the engine's per-launch key can add a provider, change an address or allow plain
  HTTP, because the engine has no switch against it. The key is treated as an admin credential, held
  only by the app and the engine, and the route exists only while Online answers is on.
- The engine's model servers (llama.cpp and the others) inherit its environment, key included. Code
  running as the same person in one of them could already read the engine's own environment, so
  this adds little. On Linux the engine is dumpable again after it starts, which is the same
  exposure.
- A public name that resolves to a private address passes the address check, because Nocturne does
  not resolve names. A test pins that behaviour.
- The engine's trace streams on loopback need no key. Telemetry is off, so they carry nothing.
- The engine does not use system proxy settings (the environment is scrubbed) and trusts the TLS
  roots bundled with its HTTP library. Nocturne cannot check what the provider keeps or for how long,
  and says so on the consent screen.
- Windows is untested for the core-dump measure; Windows writes no core files by default.

## Consequences

- Invariant 2 gains a second exception, described in the architecture notes.
- The release check gains an `online` network mode that allows a non-loopback connect only from the
  engine, only to the entered address, only during questions and never after turning off.
- A change to Lemonade's `cloud_providers` behaviour is a release blocker until the route is probed again.
- Whether providers behave as expected over the real internet is not claimed until an owner-approved trial.
