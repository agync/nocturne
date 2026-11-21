---
type: Log Entry
title: "Built the desktop screens for the phone's calendar and health."
description: "From your phones, the health pages, last night and today's calendar on the morning page, the journal rows and side block, and the calendar entry's own page, run against the mock engine."
tags: [log, update]
kind: Added
status: stable
---

The desktop interface now has every screen of
[Nocturne on Android and iPhone](/design/phone-apps.md) and
[Health from the phone](/design/phone-health.md) that sits on the computer: the From your phones
page with what came in, stopping and deleting; a page per health kind with the shared charts; "In
your calendar today" and "Last night" on the morning page and in print; calendar, workout and
woke rows and the "From Health" block in the journal, with their own groups in Delete this day;
the calendar entry's page; and the new add-a-phone words and "Also delete what it brought" in Away
from home. The charts live in `ui/src/components/health` so the phone's screens can reuse them.

The screens run against the mock engine (`?mockPhones=1` in the browser build) until the engine
fills in the commands. Words the interface uses for the two new kinds are "appointment" and
"health reading".
The state copy those screens use (loading, error, empty and refreshed states, dialog titles, units
and the chart sentences) is now recorded in the two design pages.
