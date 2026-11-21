---
type: Log Entry
title: "The Android app sends the calendar and Health while it is open."
description: "Two read-only plugins read the phone's calendar and Health Connect, and the app sends what changed to the computer when it opens, comes back to the front and reconnects."
tags: [log, update, phone, android]
kind: Added
status: stable
---

`phone/plugins/calendar` reads the phone's calendars and the appointments from two days back to
two weeks ahead, each repeat as its own day, without unanswered or declined invitations, and with
notes only when they are on. `phone/plugins/health` reads Health Connect: each night with its
asleep periods, heart rate by the hour, resting heart rate, steps, blood pressure, workouts with
their distance and weight, the last 30 days at first or up to a year with older readings on, then
only what changed. Health Connect's link to how Nocturne uses readings opens the app on that page.

The app sends only what changed since the last send, in batches under one send id, when it opens,
when it comes back to the front and when the computer answers again
([Nocturne on Android and iPhone](/design/phone-apps.md), decision 2). While the computer cannot
be reached the changes wait on the phone, the latest version of each. An appointment that leaves
the window with time is not sent as removed. Holiday and birthday calendars start off, and weight
starts off ([Health from the phone](/design/phone-health.md), decision H2).
