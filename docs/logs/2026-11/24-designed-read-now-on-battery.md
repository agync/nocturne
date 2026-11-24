---
type: Log Entry
title: "Designed honest reading on battery, with Read now."
description: "A phone on battery says it is waiting to charge instead of reading 0 of 89, says why, and offers Read now, which carries on in the background as far as the phone allows and stops plainly, with Read anyway down to 5 per cent."
tags: [log, update]
kind: Added
status: stable
---

[Setting up Nocturne's mind online](/design/mind-online-setup.md#problem-3-reading-on-battery) gains
a third part. The phone reads only while it charges, yet This phone said "Reading · 0 of 89", which
looked stuck. On battery it now says "Waiting to charge · 89 to read", with why ("so it doesn't run
the battery down") and "Read now", which is only ever the person's act.

As the owner decided, Read now carries on after the person leaves Nocturne as far as each phone
allows: on Android a `dataSync` foreground service with a silent notification showing progress and
Stop; on iPhone 26 and later iPhone's own progress, and on earlier iPhones a short while, then a
pause with one quiet notification, carrying on when Nocturne opens. Nothing keeps the screen on, and
no notification names a kept thing. Read now stops at 20 per cent or with the phone's battery saving
on, and then offers "Read anyway", which runs until done or Stop, never below 5 per cent. The part
holds the state table, every string including the notifications, accessibility, the code change it
implies in the mind's charging gate and on each platform, mockups and before and after renders.
