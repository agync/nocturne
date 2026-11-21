---
type: Log Entry
title: "Added the phone's client and the Android app shell."
description: "The sealed channel crate gained the phone's side of pairing and a networked client, and phone/ holds a Tauri 2 Android app that pairs by scan or by typed address and code."
tags: [log, update, phone, android]
kind: Added
status: stable
---

`crates/channel` gained the phone's pairing steps (`phone`) and, behind the `client` feature, the
phone's networked client: WebSocket with long-poll when the socket will not open, requests by id,
chunked uploads, a retry on a new session after a transport error, and a connection state. Typed
pairing takes the computer's address as four words or a name, then the code
([Nocturne on Android and iPhone](/design/phone-apps.md), decision 13). `phone/` holds the
Android app shell that uses it, with the phone screens built for the app, and the CI `android`
job builds its debug APK ([Building and packaging](/contribution/building.md#the-android-phone-app)).
