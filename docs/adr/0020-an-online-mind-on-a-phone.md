---
type: Decision
title: "0020 An online mind on a phone"
description: "Accepted: a phone that is its own home can choose an online service with the person's own key to read what it keeps, write its page, reflections and recaps, and answer, Android first; called by the phone's core, opt-in, sending what each task needs, private things, check-ins and originals included; Apple's servers on iPhone come later, after a trial."
status: stable
tags: [adr, phone, online, privacy, mind]
---

# 0020 An online mind on a phone

Status: accepted by the owner on 2026-11-23 ("ADR 0020 accepted now"). Proposed 2026-11-22.
Amended 2026-11-23 (owner): an online mind gets what a good answer needs, check-ins, originals and
things marked private included; private decides only what comes up on its own.
Amended 2026-11-24 (owner): a phone's question sends today's date, and its mind may look further
in the phone's own memory, read only, up to 4 requests per question, each counted
([0022 Ask looks things up](0022-ask-looks-things-up.md)).

## Context

A phone alone has no page and no answers ([0018](0018-a-phone-as-its-own-home.md)).
[0011](0011-online-answers.md) lets a provider the person chose write the last step of an answer on
the computer, called by `lemond`, and nothing else. The owner hoped the phone's own assistant could be
the mind. Research found it cannot: one company forbids other apps from using a person's
subscription, another offers no way for an app to get an answer back, and Android's
assistant-to-app calls run the other way. Apple Intelligence on Apple's servers can answer for an app
with no key on iOS 27. The design is [Choosing a mind](/design/phone-first.md#choosing-a-mind).

## Decision

- **Opt-in, one service.** A phone that is its own home may turn on one online mind, after the
  consent steps in the design: a service with the person's own address, key and model name, on both
  phones; and, on iPhone, Apple Intelligence on Apple's servers, after a trial on iOS 27 confirms its
  limits and terms. Android's default becomes the phone's own mind once that ships.
- **Android first, the key route only.** The first release is Android, with Online answers' one
  form (one provider, its key, the model), its address rules, an optional monthly limit and the Sent
  online list on the phone. iPhone follows with the same route once its build is tried. Apple
  Intelligence is later: it needs its trial and its own clause in invariant 2, and nothing about it
  ships with the first release.
- **The phone's core makes the call.** There is no `lemond` on a phone. The phone's Rust core calls
  the service, with the same typed steps the computer's mind uses (read a thing's words, answer
  from the few lines found, write the page), never a general chat.
- **What goes.** What each task needs to be done well: each thing kept, to be read, with its photo,
  recording or file when it has one; journal entries, good things and check-ins, so it can write
  reflections and recaps; each question with the remembered lines that answer it; and the lines a
  page is written from. Things marked private go like anything else: private decides only what comes
  up on its own, in notifications, reminders and updates. Only what the person let go or is
  forgetting never goes. This replaces the earlier list of what never goes (owner, 2026-11-23).
- **The safeguard is the explanation.** When the person chooses a provider, a short, calm screen
  says plainly what is sent and what private does and doesn't do
  ([Choosing a mind](/design/phone-first.md#choosing-a-mind)).
- **Bounded and recorded.** A monthly limit only if the person sets one; a Sent online list on the
  phone for 90 days, as 0011; every page and answer says where it was written.
- **0011 amended.** A phone alone's route is called by the phone, also reads and writes the page,
  and may name one company's route (Apple's) against "one generic form" and "no provider-specific
  code". On the computer too, the check for health, money or ID details stops holding sentences
  back: only things marked private are held back.
- **Invariant 2** gains the online mind clause proposed in
  [0018](0018-a-phone-as-its-own-home.md#proposed-contract-changes), for the key route. It went into
  `AGENTS.md` on 2026-11-23. Apple's servers are not in it; they join with their own words when the
  trial passes and the owner accepts that route.

## Checks before it ships

The phone's network check, the online mind's counterpart of `scripts/net-check.sh online`, runs on
an Android emulator with every packet the emulator sends captured, against a stand-in provider at an
address no one else uses. The design is [Checks before the online mind
ships](/design/phone-first.md#checks-before-the-online-mind-ships).

- **Off means silent.** With the mind off, or never turned on, the phone reaches nothing but
  consented app downloads, as a phone alone does today.
- **On means one host.** With it on, every connection the app makes goes to the one host the person
  entered: the check, one look at its list of models, and the reading, page and answer requests. No
  redirect is followed. After turning off, nothing more goes, and the key is gone from the phone's
  safe place.
- **Nothing else.** No other host, no other process and no traffic while idle with nothing to read.
  The Sent online list matches what the stand-in received, request for request.
- **The network rule.** Neither phone lets an app name a host the person types in its built-in
  network rules: Android's network security config and iPhone's App Transport Security are fixed when
  the app is built, and the phone's Rust core opens its own connections outside both. So the rule
  lives in the core's one client, built for the one host with 0011's address rules, and the check
  above proves it. The platform adds what it can: Android needs only the internet permission the app
  already holds, and iPhone asks for local network access only when the host is on the person's own
  network.

## Consequences

- A phone alone gets a page and answers on the person's account with a service, at their cost.
- The phone needs an HTTPS client to one service while this is on; that client holds the one-host
  rule (see the checks above).
- Photos and recordings make reading heavier on data and on the person's bill with the service; the
  limit and the Sent online list show it, and the provider must accept images and sound for those
  to be read (one that doesn't leaves them waiting, as with no mind).
- With check-ins sent, a phone with an online mind writes reflections and recaps.
  [0015](0015-journal-privacy-class.md)'s rule that check-ins never go online is amended to match.
- Removing the health, money and ID hold-back changes the Online answers setup copy on the
  computer, which goes through design with the build.
- The shadow question of [0016](0016-decision-models-shadow-first.md) about health, money and ID
  details may never add a mark that holds anything back.
- Using the person's assistant subscription stays out. Nocturne inside an assistant is parked for a
  separate design.
