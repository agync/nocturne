---
type: Log Entry
title: "A phone app and the computer agree on the phone build they speak."
description: "Each side speaks a window of phone builds, a phone app introduces itself first, and both sides say which one must update when they share none."
tags: [log, update, phone, remote]
kind: Added
status: stable
---

The computer's hello gives the oldest and newest phone build it speaks, and a phone app's first
call says which app it is, its version and the builds it speaks
([Remote access](/architecture/remote.md#phone-and-computer-versions)). With a build in common
they speak the newest one, and a call newer than it waits on the phone. Without one, the phone
shows "Update Nocturne on this phone" or "Update Nocturne on your computer" on Today, and keeping
still works and waits
([Nocturne on Android and iPhone](/design/phone-apps.md#please-update), decision 12).

The computer's phones list says "Android app · Nocturne {version}", "{name} · an update is ready
for it" and "{name} needs an update before it can connect", and its activity list says "{name}
needs a newer Nocturne here". `crates/channel/protocol-history.json` lists each build and the day
it shipped, and the oldest build spoken rises only once the next has been out for six months. The
calls of each build are kept as fixtures that the computer still reads and answers.
