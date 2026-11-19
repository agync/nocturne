---
type: Guide
title: Online answers
description: Let a service you already use write the last step of an answer, what is sent, what never is, how to see it, and how to turn it off.
status: stable
tags: [guide, online, privacy]
asked:
  Turn it on: [ai i already pay for, already pay for, use chatgpt, online service, answers written online, set up online answers, turn on online answers]
  What is sent: [go online, goes online, what is sent, sent to my provider, what leaves]
  What is never sent: [never sent, never goes online, private things online, health money and id]
  What your provider sees: [provider see, provider keep, does my provider]
  Cost and the limit: [how much does it cost, monthly limit, cost of online answers, spending limit]
  When it does not work: [online answers not working, online answers taking a while, provider is slow, answered on this computer instead]
  See what was sent: [see what was sent, sent online, went online, what went out]
  Turn it off and forget the key: [stop sending, stop it sending, turn off online answers, forget the key, sending my questions]
  Where the key is kept: [where is the key kept, key kept, the keychain]
---

# Online answers

Online answers lets a service you already use write the answer when you ask Nocturne a question. It is
off until you turn it on. Everything else stays on your computer: reading what you keep, the overnight
tidy and your morning page never use it.

## Turn it on

1. Open **Settings**, choose **Online answers**, then press **Set up…**. The setup has five steps and
   says what each one changes.
2. Choose the online service: **Open Advanced details** takes you to where you enter three things
   from your provider's site, the API URL, your API key and the model name. There is nothing to
   sign in to and no account with Nocturne. An online service's address starts with `https://`. One
   that runs on this computer (on `localhost`, for example) or elsewhere on your own network works
   too, and there plain `http://` is fine.
3. Press **Check**. Nocturne sends one small test question ("Reply with the word ready.") that holds none
   of your memories. If the check fails, it says why in plain words, and nothing was sent for an
   address it refuses.
4. Choose which questions may go online (the ones you ask here, and, when Away from home is on,
   **Questions from your phones**), whether details about health, money or ID may go with them (they stay out unless you allow them, and the check can miss some), and a monthly
   limit.
5. Read what will be sent, then press **Turn on online answers**. Until you do, nothing is saved and the
   key is forgotten, and leaving the setup drops what you entered.

You can also reach it from the list for answering questions, under **Choose what runs each job**,
where "Or have answers written online:" is followed by a link that starts with "Online answers".

## What is sent

For each question you ask, only:

- your question, and
- up to 12 remembered sentences that Nocturne needs to answer it.

Your photos, recordings, files, pages and everything else stay on this computer. Nocturne reads your
answer here and still checks that every line it shows leads back to something you kept.

## What is never sent

- Anything marked private.
- Your check-ins, reflections and recaps. They are written and kept on this computer.
- Health, money and ID memories, unless you turn that on. It is off by default.
- Anything while Online answers is off, and nothing at all from reading, the tidy or the page.
- Questions from a paired phone, unless Away from home is on and **Questions from your phones** is
  on. That switch is on by default when Away from home is on during setup, and you can turn it off
  later in **Online answers**.

Journal entries and good things can't be marked private, so while Online answers is on, a line from
one can go with a question it helps answer.

When Nocturne leaves something out, the answer says so, and you can ask again on this computer to
include it.

## What your provider sees

What your provider sees is your question and those sentences, your internet address, and your key.
It also sees one request for its list of services each time Nocturne's private engine starts while
Online answers is on; that request holds none of your memories. Nocturne cannot check what your
provider keeps or for how long, so read its own terms. Choose a service you already trust with
that.

## Cost and the limit

Online answers has no monthly limit unless you set one. You can limit the number of questions a
month, or an amount in US dollars (only when your provider reports what each answer costs). When
the limit is reached, answers come from this computer and Nocturne tells you once a month. The
count starts again on the 1st. Nocturne shows money only if your provider reports the cost of an
answer; otherwise it says the cost is "Not told". The site of your provider is where you see your
real balance and set your own spending cap.

## When it does not work

If you are offline, or your provider cannot be reached, is busy or returns an error, Nocturne
answers on this computer and says so. If your provider has not replied after 6 seconds, a note says
"Still waiting: your provider is taking a while." with a button **Answer on this computer
instead**, and after 20 seconds Nocturne switches by itself. You can also choose **This computer**
for a single question before you ask. It never goes the other way: if answering here fails, it
does not send anything online. If your provider refuses your key or has no credit left, Online
answers pauses and tells you; check the key or add credit, then press **Try again**.

## See what was sent

Every answer is marked with where it was written. **See what was sent**, on an answer, in Online
answers and in **Privacy**, opens "Sent online". It lists each question, the time, your provider
and exactly what left, and it keeps this for 90 days. It also lists the connection checks, and questions
that were stopped just before sending, as "not sent". A line shows again only while the memory is
unchanged; if you changed or deleted the memory since, it says so.

## Turn it off and forget the key

**Turn off** stops all sending at once, and any question being answered carries on here. **Turn off
and forget the key…** (or **Forget the key…** when it is already off) also removes the key from this
computer. You can also remove the key on your provider's site, which is
the only way to stop it working anywhere else. Delete everything removes the log and the key too, and
export includes the log as readable text (`online/sent.md`) but never the key.

## Where the key is kept

In your computer's keychain. If there is none, in a file only you can read in Nocturne's folder, and
Nocturne tells you which. It is never shown, logged or exported. Nocturne hands it to the private engine in its memory each
time the engine starts, and takes it back from the engine when you turn off, forget the key, delete
everything or leave the setup. If the engine does not take it, nothing is sent: questions are answered
on this computer, a note says so, and **Try again** brings Online answers back. If you think the key was
exposed, rotate it on your provider's site.
