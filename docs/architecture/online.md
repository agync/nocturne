---
type: Design
title: Online answers
description: "How Online answers works, from the route through the embedded engine to the choice per question, the filters, the fallback, the log and the key."
status: draft
tags: [architecture, online, privacy, engine]
---

# Online answers

Online answers lets a provider the person entered write the last step of an answer. It is off until
the person turns it on, and it changes nothing about reading, the overnight tidy or the page. The
reasons are in [decision 0011](/adr/0011-online-answers.md).

## The route

```mermaid
flowchart LR
    Ask[Ask] -- question and at most 12 sentences --> LD[lemond]
    LD -- HTTPS, one request --> P[The address the person entered]
    Key[(Key: keychain or owner-only file)] -. handed over at run time .-> LD
```

- The provider entry is one record in the engine's `config.json` (provider `nocturne_online`, the
  entered address, plain HTTP allowed only for an `http://` address on the person's own network)
  and one fixed model row in its user models, `nocturne-online`, whose model name is the one the
  person entered. Both are written at every launch by one function in the supervisor, from the same
  optional route. Nothing else writes either.
- Off is the default and the state at every launch while off: `cloud_providers` is an empty list, the
  row is removed and no key is given to the engine. The spawn environment never holds the provider
  key: it holds only the two per-launch keys beside a few basics (home folder, path, locale, runtime
  folder), and it is cleared first, so nothing of the person's own is passed on.
- The key reaches `lemond` at run time, through the engine's own endpoint on loopback, after each
  start while Online answers is on. Nocturne loads the online entry, hands the key over in memory,
  and confirms the engine holds it. It is not in argv, in the environment, in `config.json`, in a
  file the engine reads or in any request Nocturne makes to a provider, and the engine's model
  servers do not inherit it.
- Turning off, forgetting the key, Delete everything and an abandoned check clear the key from the
  running engine at once, before the relaunch waits for requests in flight. A launch that was reading
  the route when it changed hands over nothing. If the engine refuses the key or loses the route
  (a failed local load can unload the online entry), nothing is sent, questions are answered on this
  computer, a problem is shown, and "Try again" or a passed check relaunches the engine with the
  route.
- The price of that is one model-listing request to the provider each time the engine starts while
  Online answers is on, made by the engine itself. Besides that request, the engine reaches the
  provider only for a question or for the one connection check, and never while Online answers is
  off.
- A launch for a download the person agreed to leaves the provider out: no entry, no row, no key.
  Questions are answered on this computer, with the reason "downloading", until the window closes and
  the relaunch restores the route.
- Turning on, turning off, a new key or address and each check relaunch the engine. The relaunch waits
  for requests in flight: at most 2 minutes when the new setting has no route (turning off, forgetting
  the key, leaving a setup, a failed check) and 30 seconds when it has one. Turning off first closes
  the gate in the app, so no new online request starts and one in flight is dropped and answered on
  this computer.
- There is no HTTPS client to a provider in the app: the app's client talks only to the engine on
  loopback and follows no redirects. The engine posts to the provider without following redirects
  either (from its source at the pinned version, re-checked by the probe at each bump), so a redirect
  never carries the key elsewhere and comes back as a failed request.

## Addresses

An address has a host, and nothing but a scheme, host, port and path: user names, query strings,
fragments and whitespace are refused. IP literals are read the way the URL parser reads them
(decimal, octal, hex, shortened, a trailing dot) and an IPv6 form that carries an IPv4 address is
judged as that IPv4 address. Every address then falls in one of three classes, in every build:

| Class | Networks and names | Schemes |
|---|---|---|
| Never | Unspecified (0.0.0.0/8, `::`), link-local (169.254/16, fe80::/10, where cloud metadata lives), multicast, broadcast and reserved (240/4), the documentation ranges (192.0.2/24, 198.51.100/24, 203.0.113/24, 2001:db8::/32, 3fff::/20), site-local IPv6, NAT64 and 6to4, and the metadata names `metadata`, `metadata.google.internal`, `instance-data` and `instance-data.ec2.internal` | Refused |
| This computer or the person's own network | Loopback (127/8, `::1`), private (10/8, 172.16/12, 192.168/16), shared (100.64/10, where tailnets live), unique-local IPv6 (fc00::/7), and the names `localhost`, `*.localhost`, `*.local`, `*.home.arpa`, `*.internal`, `*.ts.net` and any name of one label | `https://` or `http://`, with no warning |
| Anywhere else | Everything not above | `https://` only |

The lists are `NEVER_NETS`, `NEVER_NAMES`, `OWN_NETS` and `OWN_NAMES` in the engine crate. An
address on this computer is also refused at a port Nocturne itself listens on: the engine's two
ports, the preview's and a listener of Away from home. Each holds its port while it runs, and the
engine never picks the port of a provider on this computer. A name of the second class is also
looked up, at the check and before each engine start hands over the key: if any address it resolves
to is public, the address counts as public and plain `http://` is refused as not allowed; if any is
never a provider, it is refused. For an `http://` address of the second
class, and only then, the provider entry sets the engine's `allow_insecure_http`; the engine then
takes the key for it and reaches it over plain HTTP for both the list of models and the chats
(`cloud_auth.cpp`, `cloud_server.cpp` in lemonade v2026.46.2). Without that flag the engine refuses
the key with `insecure_http_requires_opt_in`, so the route fails closed. Nocturne does not resolve
names, so a public name that resolves to a private address is judged as public and needs
`https://`. Builds with the `online-stand-in` feature only mark a route as the stand-in's in the
log, and `scripts/check-package.sh` fails a package that holds that marker.

## The choice per question

Ask picks the place for each question. A question goes online only when all of these hold:

- Online answers is on, a key is loaded, no connection check is running, and it is not paused or in
  a problem state;
- no download window is open and this month's limit is not reached;
- the person has not chosen "answer on this computer" for this question (the choice resets after
  each question);
- the question came from the desktop, or from a phone while the person has left "Questions from your
  phones" on (a phone asks only while Away from home is on; the switch is offered, and on by default,
  only when Away from home is on during setup);
- at least one memory is left to send after the filters below.

Otherwise the question is answered on this computer, and the answer says why.

## What is sent

Only the question and the memories needed to answer it, at most 12, as plain sentences. The
originals, transcripts, pages and everything else stay here. Before the request is built:

- memories marked private, in any version, memories being forgotten or let go, and any memory the
  store does not know, are dropped;
- memories about health, money or ID are dropped unless the person turned that on (off by default);
- the first 12 left are kept, and that exact set is checked again against the store just before the
  request is built; any memory that fails is dropped.

When something was left out, an answer written online says so, with "Ask on this computer". When
nothing is left to send, the question is answered here and the sent log lists it as not sent. Reading, the overnight tidy and the page have no way
to reach the route: only Ask holds it.

Request: one non-streaming chat request per question with a short reply limit. No tools, no retry.
The reply is checked on this computer like a local one: the same fences around memories, the same
check that every cited memory was sent, and "I am not sure" when the reply does not hold up. An
answer that came back not sure is never sent again automatically.

## Fallback and limits

| What happens | Result |
|---|---|
| No connection, or the provider is unreachable or busy, or it returns a server error | Answered on this computer, with the reason |
| No reply in 6 seconds | A waiting note with "Answer on this computer instead"; at 20 seconds Nocturne stops waiting, drops its request and answers here |
| The provider refuses the key | Online answers pauses, the person is told once, and a passed "Check the key" with the route in use lifts the pause |
| The provider reports no credit | Online answers pauses and the person is told; a passed check lifts it |
| The model name is not found | A problem state ("The name to use wasn't accepted") until a passed check; a 404 while a model download holds the window is not counted as this |
| The monthly limit is reached | Answered here, one notice a month; the pause lifts on the 1st |
| The reply is "not sure" | The reply is not shown; the closest memories are listed, with "Ask on this computer" |
| The reply is "not sure" and something stayed here (a private or sensitive memory, or a line of an original) | This computer answers at once with what stayed here, marked "your provider wasn't sure" (`notSureOnline`), still linked to what was sent |

Local failure never goes online. There is no monthly limit unless the person sets one. A limit can be
a number of questions (the setup suggests 200) or an amount in US dollars, which holds only while the provider reports a cost on its replies (it
falls back to the question count when it stops). A question counts when the engine
returned an HTTP status, or when it was cut short after the request was handed over (slow, or
Online answers turned off in flight); a request that never reached the provider (no internet,
unreachable) does not. The month is the calendar month in the person's time zone, and questions
being asked at once are counted before they are logged, so the desktop and a phone cannot pass the
limit together.

## The connection check

The check relaunches the engine with a provisional route and sends one fixed question ("Reply with
the word ready.", no memories, at most 8 tokens back, 20 seconds). Outcomes: works, unreachable, key
refused, no credit, model not found, no internet, not secure (`http://` to an address outside the
person's own network), address not allowed,
busy, server error, empty reply, cancelled. An address that is malformed or carries a user name,
query or fragment is reported as address not allowed. The address is judged before anything is
sent. The provisional route and the key stay in memory only until the person consents: the key is
written to its home when they turn on. Leaving the setup drops them and relaunches the engine without
them; quitting loses them because nothing was saved. While Online answers is on, a check never
changes what questions use: a new route waits for the person to agree. Turning on needs a passed
check and then consent. Every check is also listed in the sent log.

## The sent log

Every check, and every question that went toward the route, is recorded with the time, the host, the
size sent and received and the result. So is a question stopped just before sending (nothing left to
send, Online answers turned off, a store problem), as "not sent". Questions answered here earlier
(the person chose this computer, the limit, a pause, a problem state, a download window) are not
listed. The text is stored as a template: each memory sentence is replaced by its id plus a
hash of the exact line sent, so the log never holds the memory words twice. The screen shows a line
again only while the memory still hashes the same; otherwise it says the memory was changed or
deleted. The question is stored as typed. A memory that is forgotten leaves its row with no words,
id or hash. Rows are kept 90 days (pruned at launch and daily), are written to the export as a
readable `online/sent.md` without the key, and are cleared by Delete everything.

## The key

The key is kept in the system keychain (service `Nocturne`). Where there is no keychain, or it
refuses, it is kept in a mode 0600 file (`online/key` in the data folder, in a mode 0700 folder;
on Windows the folder lets in the current user, SYSTEM and Administrators, through its access list) and the advanced
view says so ("owner-only file" or "system keychain"). The file is left out of the
export, and "Forget the key" and Delete everything remove it. If a removal fails, the view still
says the key is kept. A key that cannot be read when first needed leaves Online answers in a problem
state and answers on this computer. It never appears in a view, event, notice, log or export.

## Logs and core files

The engine's standard output and standard error go to Nocturne's own pipe, each redacted on its
own, and then to `lemond.log` in the app's `logs/` folder (owner-only, rotated at 8 MB, 3 files of its own: `lemond.log`, `.1` and
`.2`). The pipe withholds text after a provider error body marker (`Backend returned error
response:`, `body:`, `Body:`), and every following line of that output until one that starts with
the engine's own log prefix, so a multi-line body is withheld whole. Standard error never has that
prefix, so a marker there withholds the rest of it until the engine starts again. Any
occurrence of the key or of the launch's two engine keys, or of any eight or more characters in a
row from one, becomes `[key]`, and lines are capped at 1 KB. When the file falls behind, lines are
dropped rather than the engine waiting, and a log that cannot be opened is tried again at most every
30 s, with lines dropped meanwhile. The engine's log level stays at `info`, so reply text is
never produced. The whole app sets `RLIMIT_CORE` to 0 at start and on Linux is made undumpable;
the engine and its children inherit the limit.

## What the route does not protect

- Anything holding the engine's per-launch key can add a provider or change an address, so that key is
  an admin credential held only by the app and the engine.
- Anything holding the engine's per-launch key can also replace the key the engine holds; that holder
  could already change the address, so it is covered by the first point.
- The provider sees one model-listing request (and the key) each time the engine starts while Online
  answers is on, with no question or memory in it.
- A public name that resolves to a private address passes the address check, as a public one that
  must use `https://`.
- A provider on the person's own network over `http://` sees the key and the questions in plain
  text on that network, as any device there could; that is the person's choice of address.
- 100.64.0.0/10 is also carrier-grade NAT space: without a tailnet, a plain `http://` provider
  there could be outside the person's own network.
- The engine looks a name up again when it connects, so a DNS change after Nocturne's look-up is
  not seen.
- The engine's trace streams on loopback need no key; telemetry is off, so they carry nothing.
- System proxy settings are not used, and TLS trust is the engine's bundled default.
- Nocturne cannot check what the provider keeps.

## Verification

Tests use a stand-in provider on loopback (`fake-provider`) and never a real one. The `online` mode
of the network check (`make netcheck-online`) and the package check for the stand-in marker exist;
the route probe at each engine bump is repeated by hand ([releasing](/contribution/release.md)). The privacy targets are in [evaluation](/design/evaluation.md).
