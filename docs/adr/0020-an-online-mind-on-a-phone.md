---
type: Decision
title: "0020 An online mind on a phone"
description: "Proposed: a phone that is its own home can choose an online service with the person's own key, or Apple's servers on iPhone after a trial, to read the words of what it keeps, write its page and answer; called by the phone's core, opt-in, never photos, recordings, check-ins or anything marked private."
status: draft
tags: [adr, phone, online, privacy, mind]
---

# 0020 An online mind on a phone

Status: proposed. Date: 2026-11-22.

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
- **The phone's core makes the call.** There is no `lemond` on a phone. The phone's Rust core calls
  the service, with the same typed steps the computer's mind uses (read a thing's words, answer
  from the few lines found, write the page), never a general chat.
- **What goes.** The words of what is kept (typed, heard by the phone's own speech recogniser, or the
  text of a shared page), journal entries and good things, each question with at most 12 remembered
  sentences, and the lines a page is written from. Never photos or recordings themselves, check-ins,
  reflections or recaps (by [0015](0015-journal-privacy-class.md)), or anything marked private.
  Nothing else is held back by what it seems to be about.
- **Bounded and recorded.** A monthly limit only if the person sets one; a Sent online list on the
  phone for 90 days, as 0011; every page and answer says where it was written.
- **0011 amended.** A phone alone's route is called by the phone, also reads and writes the page,
  and may name one company's route (Apple's) against "one generic form" and "no provider-specific
  code". On the computer too, the check for health, money or ID details stops holding sentences
  back: only things marked private are held back.
- **Invariant 2** gains the online mind clause proposed in
  [0018](0018-a-phone-as-its-own-home.md#proposed-contract-changes).

## Consequences

- A phone alone gets a page and answers on the person's account with a service, at their cost.
- The phone needs an HTTPS client to one service while this is on; the app's network rule names it.
- Removing the health, money and ID hold-back changes the Online answers setup copy on the
  computer, which goes through design with the build.
- The shadow question of [0016](0016-decision-models-shadow-first.md) about health, money and ID
  details may never add a mark that holds anything back.
- Using the person's assistant subscription stays out. Nocturne inside an assistant is parked for a
  separate design.
