---
type: Decision
title: "0017 Native phone apps on a shared Rust channel"
description: "Native Nocturne apps for Android first and iPhone later, built with Tauri 2 on one Rust crate for the sealed channel that the computer also uses, with the browser phone page kept."
status: stable
tags: [adr, remote, phone, channel]
---

# 0017 Native phone apps on a shared Rust channel

Status: accepted. Date: 2026-11-21.

## Context

A paired phone reaches Nocturne through a page its browser keeps
([decision 0010](0010-away-from-home.md)). That page cannot read the phone's calendar or health
readings, has to be added to the home screen by hand, and in Simple its code comes through
Cloudflare, which decision 0010 names as a known limitation with a native app as the long-term fix.
The approved designs, [Nocturne on Android and iPhone](/design/phone-apps.md) and
[Health from the phone](/design/phone-health.md), describe that app. A feasibility build showed
that Tauri 2 runs the existing phone screens unchanged on Android.

The computer's side of the sealed channel is Rust in the app shell; the phone page has its own
TypeScript copy. Both are held to the reference vectors in
`src-tauri/tests/fixtures/remote-vectors.json`.

## Decision

- Build native phone apps with Tauri 2, Android first and iPhone after it has proved out. The app
  shows the same screens as the phone page and adds the calendar and health readings, both
  optional and read only.
- Move the pure steps of the sealed channel (constants, keys, pairing, session and frames) into one
  crate, `crates/channel` (`nocturne-channel`), with no I/O, so it builds for the phone. The
  computer's shell re-exports it and keeps its own key file; the phone app's Rust core uses the
  same crate, so only Rust touches the network on the phone.
- Keep the browser phone page alongside the apps, without calendar or health, and look again a
  year after the iPhone app ships.
- Ship Android through Google Play and as a signed file on the project's site.
- The phone sends calendar and health only while the app is open in the first release; background
  sending comes later, behind a switch, off by default.

## Consequences

- One Rust implementation serves the computer and the app; the phone page keeps its TypeScript
  client. Both stay tied to the same vectors and interop tests.
- The app's code comes from the store or the signed file, not from the tunnel, which removes the
  Simple limitation for people who install it.
- Two phone clients exist until the browser page is retired.
- Store review, Health Connect declarations and an Apple developer account become part of a
  release.
