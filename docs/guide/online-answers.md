---
type: Guide
title: Online answers
description: Let a service you already use write the last step of an answer, what is sent, what never is, how to see it, and how to turn it off.
status: draft
tags: [guide, online, privacy]
---

# Online answers

Online answers lets a service you already use write the answer when you ask Nocturne a question. It is
off until you turn it on. Everything else stays on your computer: reading what you keep, the overnight
tidy and your morning page never use it.

> [!NOTE]
> This page describes how the feature is designed to behave. It has not yet been tried with a real
> provider, and Nocturne cannot check what a provider keeps.

## Turn it on

1. Open Settings and choose Online answers.
2. Enter three things from your provider's site: the API URL, your API key and the model name. There
   is nothing to sign in to and no account with Nocturne.
3. Nocturne checks them with one small test question that holds none of your memories. If the check
   fails, it says why in plain words.
4. Read what will be sent, then say yes. Until you do, nothing is saved and the key is forgotten.

You can also reach it from the choice of answering model.

## What is sent

For each question you ask, only:

- your question, and
- up to 12 remembered sentences that Nocturne needs to answer it.

Your photos, recordings, files, pages and everything else stay on this computer. Nocturne reads your
answer here and still checks that every line it shows leads back to something you kept.

## What is never sent

- Anything marked private.
- Health, money and ID memories, unless you turn that on. It is off by default.
- Anything while Online answers is off, and nothing at all from reading, the tidy or the page.
- Questions from a paired phone, unless you turned that on and Away from home is on.

When Nocturne leaves something out, the answer says so, and you can ask again on this computer to
include it.

## What the provider sees

The provider sees your question and those sentences, your internet address, and your key. Nocturne
cannot check what the provider keeps or for how long, so read its own terms. Choose a provider you
already trust with that.

## Cost and the limit

Nocturne has a limit of 200 questions a month, which you can change. When the limit is reached, answers come from this
computer and Nocturne tells you once. The count starts again on the 1st. Nocturne shows money only if
your provider reports the cost of an answer; otherwise it says the cost is not told. Your provider's
site is where you see your real balance and set your own spending cap.

## When it does not work

If you are offline, the provider is busy or it is slow, Nocturne answers on this computer and says
so. It never goes the other way: if answering here fails, it does not send anything online. If the
provider refuses your key or has no credit left, Online answers pauses and tells you; check the key
or add credit, then check again.

## See what was sent

Every answer is marked with where it was written. "Sent online" in Privacy lists each question,
the time, the provider and exactly what left, and it keeps this for 90 days. A line shows again only
while the memory is unchanged; if you changed or deleted the memory since, it says so.

## Turn it off and forget the key

Turn off stops all sending at once, and any question being answered carries on here. "Forget the key"
also removes the key from this computer. You can also remove the key on your provider's site, which is
the only way to stop it working anywhere else. Delete everything removes the log and the key too, and
export includes the log but never the key.

## Where the key is kept

In your computer's keychain. If there is none, in a file only you can read in Nocturne's folder, and
Nocturne tells you which. It is never shown, logged or exported.
