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

1. Open Settings, then Advanced details, choose Online answers and "Set up…". The setup has five
   steps and says what each one changes.
2. Enter three things from your provider's site: the API URL, your API key and the model name. There
   is nothing to sign in to and no account with Nocturne. The address must start with `https://`.
3. Press Check. Nocturne sends one small test question ("Reply with the word ready.") that holds none
   of your memories. If the check fails, it says why in plain words, and nothing was sent for an
   address it refuses.
4. Choose which questions may go online (the ones you ask here, and, when Away from home is on,
   the ones from your phones), whether health, money and ID details may go with them, and a monthly
   limit.
5. Read what will be sent, then press "Turn on online answers". Until you do, nothing is saved and the
   key is forgotten, and leaving the setup drops what you entered.

You can also reach it from the list for answering questions, where "Or have answers written online:"
links to "Online answers".

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
- Questions from a paired phone, unless Away from home is on and "Questions from your phones" is
  on. That switch is on by default when Away from home is on during setup, and you can turn it off
  later in Settings.

When Nocturne leaves something out, the answer says so, and you can ask again on this computer to
include it.

## What the provider sees

The provider sees your question and those sentences, your internet address, and your key. It also
sees one request for its list of services each time Nocturne's private engine starts while Online
answers is on; that request holds none of your memories. Nocturne
cannot check what the provider keeps or for how long, so read its own terms. Choose a provider you
already trust with that.

## Cost and the limit

Online answers has no monthly limit unless you set one. You can limit the number of questions a
month, or an amount in US dollars (only when your provider reports what each answer costs). When the limit is reached, answers come from this computer and
Nocturne tells you once a month. The count starts again on the 1st. Nocturne shows money only if
your provider reports the cost of an answer; otherwise it says the cost is "Not told". Your
provider's site is where you see your real balance and set your own spending cap.

## When it does not work

If you are offline or the provider cannot be reached, is busy or returns an error, Nocturne answers
on this computer and says so. If the provider has not replied after 6 seconds, a note says "Still
waiting: your provider is taking a while." with a button "Answer on this computer instead", and
after 20 seconds Nocturne switches by itself. You can also choose "This computer" for a single
question before you ask. It never goes the other way: if answering here fails, it does not send anything online. If the
provider refuses your key or has no credit left, Online answers pauses and tells you; check the key
or add credit, then check again.

## See what was sent

Every answer is marked with where it was written. "See what was sent", on an answer, in Online
answers and in Privacy, opens "Sent online". It lists each question, the time, the provider and
exactly what left, and it keeps this for 90 days. It also lists the connection checks, and questions
that were stopped just before sending, as "not sent". A line shows again only while the memory is
unchanged; if you changed or deleted the memory since, it says so.

## Turn it off and forget the key

"Turn off" stops all sending at once, and any question being answered carries on here. "Turn off and
forget the key…" (or "Forget the key…" when it is already off) also removes the key from this
computer. You can also remove the key on your provider's site, which is
the only way to stop it working anywhere else. Delete everything removes the log and the key too, and
export includes the log as readable text (`online/sent.md`) but never the key.

## Where the key is kept

In your computer's keychain. If there is none, in a file only you can read in Nocturne's folder, and
Nocturne tells you which. It is never shown, logged or exported, and it is not put in the private engine's start-up
settings, so a crash record of the engine does not hold it. Rotating a key at your provider is still
good practice after any suspected exposure.
