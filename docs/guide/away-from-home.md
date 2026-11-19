---
type: Guide
title: Away from home
description: Ask, keep and read your page from a phone when you are out, what that lets other companies see, and how to remove a phone or turn it off.
status: stable
tags: [guide, phone, privacy, remote]
asked:
  Turn it on: [get nocturne on my phone, use nocturne on my phone, set up away from home, turn on away from home, nocturne on my phone]
  Most private sign-in: [sign in to tailscale, tailscale account, most private sign in]
  Add a phone: [connect my phone, pair a phone, pair my phone, add a phone, add my phone, get nocturne on my phone, compare the three words]
  What the phone can do: [phone can do, when i'm out, while i'm out, my phone see, see on my phone, read on my phone, from my phone, phone app]
  What the companies can see: [cloudflare see, tailscale see, companies see, who can see my questions]
  When the address changes: [can't connect, cannot connect, won't connect, phone needs the new address, scan the new code]
  Switching connection: [switch connection, change connection, change to most private, change to simple]
  Remove a lost phone: [lost my phone, phone was stolen, stolen phone, remove a phone, stop a phone]
  Turn it off: [turn off away from home, turn away from home off, stop away from home]
---

# Away from home

Away from home lets your phone ask Nocturne a question, keep something and read today's page while
you are out. It is off until you turn it on. Everything stays on your computer; your phone talks to
it through a connection you choose, and what travels is sealed so that only your phone and your
computer can read it.

## Turn it on

1. Open **Settings** and choose **Away from home**. Press **Set up…**, or **Turn on** if you have
   set it up before.
2. Choose a connection. Simple is the default and needs no account. Most private uses your own
   Tailscale account and is harder for anyone else to observe.
3. Nocturne asks before it downloads the small helper program the connection needs. Nothing is
   downloaded until you say yes.
4. Your computer must stay on and awake for the phone to reach it. Nocturne can keep it awake, but
   only while it is plugged in.

## Most private sign-in

Most private uses your own Tailscale account, and Nocturne walks you through what that account asks
for. Each step opens the right Tailscale page in your browser, and Nocturne carries on by itself
when you are done.

1. Sign in to Tailscale in your browser, in the page that opens.
2. If your Tailscale account asks for new computers to be approved, Nocturne says so and opens the
   page that lists this computer. Approve it there. It shows under a made-up name.
3. If your account does not allow secure links yet, Nocturne opens that page. Choose
   Enable HTTPS once.
4. Get Tailscale on your phone, if it is not there already.

Tailscale can take a little while to make the secure address. Nocturne keeps trying. If it is
still not ready after 30 seconds, Away from home shows a problem and keeps trying, then returns to
normal when it works.

If your Tailscale account asks computers to keep a log of their activity for Tailscale, Most
private refuses to join and says so in plain words, because it never sends logs. Use another
account, or turn Away from home off.

## Add a phone

1. Choose **Add a phone**. Nocturne shows a code that works once and lasts five minutes.
2. On the phone, scan the code with its camera. If you cannot scan it, open the address shown and
   type the 24 characters of the typed code instead. Type the address exactly: a look-alike site
   that has your code can pair itself and show matching words, and only the exact address protects
   you from that.
3. Name the phone.
4. Compare the three words. Both screens show three words. If they are the same, press **Allow** on
   the computer. If they differ, or you did not start this, press **Don't allow**. If the computer asks to
   allow a phone and your phone is not showing three words, someone else is trying to pair, so
   refuse.

A pairing code can leak through a photo, a screen share or a QR scanner that works online. Comparing the words
is the only thing that stops someone who has it, so do not press **Allow** without comparing.

If an iPhone is involved, add Nocturne to the home screen first and open it from there, so the phone
keeps its key.

## What the phone can do

Ask a question, keep text, a photo, a document or a voice note, take back what it just kept, read
today's page and the last seven, read your journal, keep a note with a feeling or as a good thing,
answer "Which {name} is this?", and open an original that an answer or the page cited. It never
sees anything you marked private. It cannot forget, delete, export, change settings or add other
phones.

If the phone has no signal, what it keeps waits on the phone and is sent when it connects again.
That waiting list sits in the phone's own storage and is not hidden from someone who has the phone.

An unlocked or stolen phone can read everything not marked private. That includes your journal. It can read it until you remove it from the
computer, so remove a lost phone first.

## What the companies can see

Content is sealed, so neither company can read your questions, your page or your files. They can see
other things.

- **Simple (Cloudflare).** Your phone's internet address and rough place, a stable id for each
  phone, when it connects, and the size and timing of every message. Cloudflare also serves the
  code of the page your phone opens. The phone keeps that page so it opens without a connection, so
  a change Cloudflare makes, or someone who breaks into it, can stay on the phone until you add the
  phone again, and a changed page could misuse your phone's key while it is open. After your
  computer restarts, the phone looks for it at an old name that Cloudflare, or someone who later
  gets that name, answers. Nothing in the phone app can check this, so it is a known limit. A
  native phone app would later fix it. Most private makes it much harder: Tailscale would have to
  vouch for your computer's name in public, where anyone can see it. In Simple, anyone using your
  computer can also see the public address, though it does not let them in.
- **Most private (Tailscale).** Your account, the computers on it and when they connect. When your
  phone and computer cannot connect directly, Tailscale carries the sealed traffic. Only your own
  devices on your Tailscale account can reach your computer. Other people's devices, tagged
  devices and devices shared with you are refused, and your own still need a code and your Allow to
  pair.
- **Both.** The helper also talks to its company's computers to stay connected while Away from home
  is on. Your computer may ask whether Nocturne's helper may accept connections. That is expected.
  It is why this feature is not described as staying on your own computer: while it is on, a
  helper uses your computer's network interfaces, not only its internal address.

Nocturne does not measure this live. **Privacy** shows what was sent to your phones, under **Sent to
your phones**.

## When the address changes

With Simple, the address changes each time Nocturne starts it. Your phones then need the new
address. Nocturne shows a code for it: on the phone, open Nocturne from the home screen, choose
**Scan the new code** and point it at your computer, or type the four words shown. Most private does not need
this.

## Switching connection

Changing between Simple and Most private removes every phone, and the screen tells you before it
does. Add them again afterwards.

## Remove a lost phone

Open **Away from home**, choose **See phones**, then **Remove** beside the phone. It stops at once and any answer in progress
ends. Do this first if a phone is lost or stolen. If you set up a new computer key, or choose Delete
everything, phones can no longer connect and only show "can't connect". That is expected.

## Turn it off

Open **Away from home** and turn it off. The helper stops, and no helper program is left running or listening.
Phones stay paired unless you choose to remove them. You can also set it to turn itself off after a
day, a week or 30 days unused, under **Turn off by itself**.
