---
type: Design
title: Privacy policy
description: Proposal for Nocturne's public privacy policy at /privacy/ on the site, with where it is linked, the full public text, the evidence in the code for each claim, the owner's open questions and what must change in it when features change.
status: draft
tags: [proposal, privacy, site, phone, copy, stores]
---

# Privacy policy

Status: proposed. Not built. The site page waits for this design to be approved, as `AGENTS.md`
invariant 12 asks.

## Purpose

One page a person, a store reviewer or a regulator can read to learn what Nocturne keeps, what
leaves the device, when, where it goes and who controls it. The app stores require a policy at a
public address: Google Play for any app, and again for Health Connect and the Google Drive scope;
the App Store for any app, and again for HealthKit. The policy restates what the app already says
at each consent step, in one place. It promises nothing the code does not do.

## Where it lives

| Place | What it shows | Change needed |
|---|---|---|
| The site, `/privacy/` | The full text below, in English | A new page, built from this design after approval |
| The site footer, every page | "Privacy policy", beside "Your data" | One footer link, all languages; the link text is translated, the page stays English until the owner decides otherwise ([question 6](#open-questions-for-the-owner)) |
| The install page | One line under the downloads: "Nocturne keeps what you give it on your own devices. Privacy policy" | One line and link |
| The computer app, Privacy | A quiet link "Privacy policy" at the foot, under the promise line | The app has no About screen; Privacy is the nearest place. Opens the site in the browser |
| The phone app, This phone | A quiet link "Privacy policy" at the foot | Opens the site in the browser. The phone already allows links to `https://nocturne.is/*` |
| The phone's Health Connect rationale page | A link "Privacy policy" under the existing rationale text | Health Connect asks for the policy from this page |
| Google Play and App Store listings | The address `https://nocturne.is/privacy/` | Store console fields, at release |

The computer and phone links are interface changes and wait for this design's approval with the
page. They add one link each and change no other words.

## The public text

The text below is what the page says, word for word. `SUPPORT_EMAIL` and `PUBLISHER_NAME` are
placeholders the owner fills in ([questions](#open-questions-for-the-owner)). The section marked
"When it ships" stays off the page until the nightly backup ships.

---

> # Privacy policy
>
> Effective 24 November 2026.
>
> ## In short
>
> - Nocturne keeps what you give it on your own computer or phone.
> - We have no servers that hold your data. There is no account with us, and no analytics, ads,
>   crash reports or tracking.
> - Nothing you kept leaves your device unless you turn on something that needs it. Each of those
>   is off until you turn it on, and each sends only to a place you chose.
>
> ## Who we are
>
> Nocturne is free, open-source software made by PUBLISHER_NAME. It runs on Linux, Mac and Windows
> computers, and on Android phones and iPhones. In this policy, "we" means the people who make
> Nocturne.
>
> Nocturne does not send us what you keep, so we never see it. We cannot read it, sell it or share
> it, because we do not have it.
>
> ## What Nocturne keeps, and where
>
> Nocturne keeps what you give it, and what it works out from it:
>
> - the things you keep: notes you type or paste, photos, screenshots, voice notes, sound
>   recordings and documents;
> - what it remembers from them, your morning pages and its answers to your questions;
> - your journal: entries, good things, check-ins, reflections and recaps;
> - entries from your phone's calendar and readings from Apple Health or Health Connect, if you
>   let your phone share them;
> - your settings, and small log files that hold counts and kinds of error, never what you kept.
>
> On a computer, all of this is in one folder on that computer. On a phone that keeps its own
> memory, it is in Nocturne's own storage on the phone. Nocturne keeps that storage out of the
> phone's own backup to iCloud or Google, and out of copying to a new phone.
>
> Things you have not used fade after a time and are then let go. You choose how long in Settings.
> You can bring them back until you delete them. Nocturne never deletes something without you
> asking.
>
> ## When something leaves your device
>
> Once it is set up, Nocturne works without the internet. These are the only times anything goes
> out.
>
> ### Downloads you agree to
>
> To read, listen and speak on your computer, Nocturne needs some large files. In the advanced
> details of the app these are called models. Setup downloads them from Hugging Face and GitHub,
> where their makers publish them. Nocturne tells you the size first and downloads only after you
> agree. The same goes for anything you choose to add later, and for the connection part that
> Away from home needs. Those sites see your internet address and what you downloaded, as any
> website does. Nothing you kept is sent. Privacy, in the app, lists every download with its day
> and size.
>
> Nocturne does not check for updates by itself. Updates come from wherever you installed it.
>
> ### Online answers, on a computer
>
> Off until you turn it on. You choose one provider of online answers, and give its address and
> your own key. Your agreement with that provider is your own. While it is on:
>
> - Each question you ask goes to your provider with up to 12 short lines from what you kept that
>   help answer it. These lines can come from your journal entries and good things.
> - Questions you ask from a paired phone go the same way, unless you turn that off.
> - Each time Nocturne starts its engine, it asks your provider once for its list of models. That
>   request carries your key and nothing you kept.
> - Your photos, recordings and documents themselves are never sent, though what Nocturne remembers
>   from them can be. Check-ins, reflections, recaps and health readings are never sent. Nor is
>   anything you let go or asked Nocturne to forget.
>
> Marking something private stops it coming up on its own, for example in notifications. Do not
> rely on it to keep something from the provider you chose.
>
> Privacy, in the app, keeps a list of exactly what was sent, for 90 days.
>
> ### An online mind, on a phone of its own
>
> A phone with no computer can use an online provider you choose to read what you keep, write your
> morning page, reflections and recaps, and answer your questions. Off until you turn it on. You
> give the provider's address and your own key. While it is on, the phone sends that provider what
> each job needs to be done well:
>
> - each thing you keep, to be read, with its photo, recording or file;
> - your calendar entries, journal entries, good things and check-ins;
> - each question, with the lines that answer it;
> - the lines your page and reflections are written from.
>
> Things you marked private go too. Marking something private stops it coming up on its own; it
> does not hold it back from the provider you chose. Health readings are never sent. Nor is
> anything you let go or asked Nocturne to forget.
>
> When you check the connection, the phone also asks the provider for its list of models. The
> phone keeps a list of every request it made, for 90 days.
>
> ### Away from home
>
> Off until you turn it on. It lets your own paired phones reach your computer when you are out.
> You choose how they connect:
>
> - **Simple** goes through Cloudflare. Cloudflare can see when your phone connects, its internet
>   address and roughly where it is, a fixed number for each phone, and how much goes each way and
>   when. Cloudflare also delivers the page your phone opens.
> - **Most private** goes through your own Tailscale network. Tailscale can see your account, your
>   devices, when they connect and roughly where, and carries the traffic when they cannot reach
>   each other directly.
>
> Everything between your phone and your computer is locked so that only they can read it. Neither
> company can read what you ask, what you keep, or your answers and pages. The connection part
> also talks to its own company to stay connected.
>
> ### Your phone and your computer
>
> A phone paired with your computer sends it what you keep on the phone, and, if you allow, its
> calendar entries and health readings. The computer sends back your answers and pages. All of it
> is locked so that only your phone and your computer can read it, and it travels the way you
> chose for Away from home. When you move a phone's memory into a computer, it travels the same way.
>
> ### Nightly backup, on a phone of its own (when it ships)
>
> Off until you turn it on. Each night while the phone charges, Nocturne saves what changed to your
> own account:
>
> - on iPhone, to Nocturne's own space in your iCloud;
> - on Android, to a folder called "Nocturne backup" in your Google Drive. You sign in with
>   Google's own screens, and allow Nocturne to reach only the files it creates. It cannot see
>   anything else in your Drive.
>
> The backup is locked with a phrase only you hold, and a recovery key if you make one. Apple,
> Google and we cannot open it. If you lose both, nobody can. Nocturne keeps the last seven
> nights. When you forget or delete something, it is removed from the backup too.
>
> Your health readings are not in the backup. After a restore, Nocturne reads them again from Apple
> Health or Health Connect, if you allow it. Pages and reflections that mention them are in the
> backup.
>
> ## What your phone asks to use
>
> Your phone asks you before Nocturne can use each of these. Saying no turns off only the part that
> needs it.
>
> | Permission | Why |
> |---|---|
> | Camera (iPhone) | To scan the code on your computer's screen when you pair, and to take photos you choose to keep. |
> | Microphone | To record voice notes you choose to keep, only while you record. |
> | Speech recognition (iPhone) | To turn your voice notes into words on the iPhone, so you can find them. |
> | Calendar | To read your calendar, so your page and journal can show what's on. Nocturne never changes your calendar. |
> | Apple Health or Health Connect | To read the kinds of reading you choose, to show them beside your journal and on your page. Nocturne never writes to Health. |
> | Internet (Android) | To reach your computer, and your online provider if you choose one. |
>
> On Android, Nocturne may read and think while the phone charges, in the background, when you
> have an online mind turned on.
>
> Turning voice notes into words happens on your phone. On iPhone it never leaves the phone. On
> Android Nocturne uses the phone's own on-device reader. Where a phone has none, Nocturne uses the
> phone's speech service, only for a language already installed on the phone, and asks it to work
> offline.
>
> ## Health information
>
> If you allow it, Nocturne reads these from Apple Health or Health Connect: sleep, heart rate,
> resting heart rate, blood pressure, steps, workouts with their distance, and weight. It reads
> only the kinds you allow, and never writes, changes or deletes anything in Health.
>
> - On a phone paired with a computer, readings go only to that computer, locked so only it can
>   read them.
> - On a phone of its own, readings stay in Nocturne's memory on the phone.
> - Readings are never sent to an online provider, even when you have one turned on.
> - Health information is never used for advertising, never sold, and never shared with anyone.
>   We never receive it.
>
> ## Google user data
>
> When the nightly backup ships, Nocturne on Android uses Google Drive with access only to the
> files it creates. It uses that access only to save, list, restore and delete your own locked
> backup. Nocturne's use and transfer to any other app of information received from Google APIs
> will adhere to the
> [Google API Services User Data Policy](https://developers.google.com/terms/api-services-user-data-policy),
> including the Limited Use requirements. Nothing from your Drive is used for advertising, sold,
> read by people, or sent to us.
>
> ## Seeing, exporting and deleting your data
>
> Because we hold nothing of yours, you never need to ask us to see or delete it. You do it in
> Nocturne:
>
> - **See it.** Everything Nocturne remembers leads back to what it came from. Privacy on a
>   computer, and This phone on a phone, show what was sent online and what was downloaded.
> - **Take a copy.** Export everything writes readable files to a folder you choose. A backup to a
>   file saves one locked file wherever you choose.
> - **Let go and delete.** You can let go of a thing, bring it back, or delete it for good.
>   Deleting asks first.
> - **Delete everything.** Removes everything Nocturne holds from that device, after you type the
>   words it shows. It also forgets the key for your online provider and the list of what was sent
>   online. On a computer it removes your paired phones. On a phone with a nightly backup, it
>   deletes the backup too.
>
> ## Copies held by others you chose
>
> Nocturne cannot reach copies outside your devices. Their own policies apply:
>
> - **Your online provider** decides what it keeps of what you sent it, and for how long. Read its
>   privacy policy before you choose it.
> - **iCloud and Google Drive** hold your locked backup under Apple's and Google's terms. Turning
>   off the backup lets you delete it, and Delete everything deletes it.
> - **Cloudflare and Tailscale** see what is listed under Away from home, under their own terms.
> - **Anything you exported, printed or saved elsewhere** is out of Nocturne's reach.
>
> ## This website
>
> This website has no cookies, no analytics, and no scripts or fonts from anyone else. Your
> browser remembers your choice of light or dark, and whether you closed the language line, in its
> own storage. That never leaves your browser. The site is hosted by Cloudflare, which handles each
> request and may keep standard records such as your internet address. The download links lead to
> GitHub, under GitHub's own terms.
>
> ## Children
>
> Nocturne is not made for children under 13, or under 16 where the law sets that age. Since we
> receive nothing from Nocturne, we do not know who uses it.
>
> ## Changes to this policy
>
> When Nocturne changes what it keeps or sends, this page changes with it, and the date at the top
> changes. Earlier versions stay in Nocturne's public source history.
>
> ## Contact
>
> Questions about this policy or your privacy: SUPPORT_EMAIL.

---

## Evidence

Every claim on the page, and where the repository shows it. Paths are from the repository root.

| Claim | Evidence |
|---|---|
| No servers of ours hold data; local only | `AGENTS.md:24-34` (invariant 2) |
| No analytics, no crash reports, no account (computer) | `ui/src/copy/en-GB/privacy.json:26`; `docs/guide/your-data.md` "How you can tell"; no analytics or crash code found by grep in `crates`, `src-tauri`, `phone`, `ui/src` |
| No analytics or ads (phone) | `ui/src/copy/en-GB/phone-alone.json:125`; `ui/src/copy/en-GB/phone-health.json:75`; phone web view CSP `phone/src-tauri/tauri.conf.json:13` |
| The engine's own telemetry is off | `crates/lemonade/src/config.rs:108` |
| No update check by the app | `ui/e2e/privacy.spec.ts:75-78`; `check_updates` defaults false `crates/engine/src/settings.rs:29` and nothing reads it to reach the network; engine model update checks off `crates/lemonade/src/config.rs:83-84` |
| Open source, Apache 2.0 | `LICENSE`; `site/i18n/en.json:108` |
| What is kept: notes, photos, screenshots, voice, audio, documents | `docs/design/concept.md:31-32` |
| Journal, check-ins, reflections, recaps, pages | `crates/engine/src/store/schema.sql:44,301` (pages, reflections) |
| Calendar entries and health readings from phones | `crates/engine/src/store/schema.sql:360-372` |
| Logs hold counts and error kinds | `docs/guide/your-data.md` "Where it is" |
| One folder on the computer | `docs/guide/your-data.md` "Where it is" |
| Phone store out of device backup and device transfer | `phone/src-tauri/gen/android/app/src/main/AndroidManifest.xml:5-9`; `phone/src-tauri/gen/android/app/src/main/res/xml/data_extraction_rules.xml:4-25`; `phone/plugins/system/ios/Sources/SystemPlugin.swift:111-122`; `phone/src-tauri/src/lib.rs:139-155` |
| Fading and letting go, setting, bring back | `crates/engine/src/lifecycle.rs:49-72`; `docs/guide/your-data.md` "Let go" |
| Downloads only after agreement, size first | `docs/guide/choosing-models.md:30-34`; engine offline except during an approved download `crates/lemonade/src/config.rs:47-50,81` |
| Downloads from Hugging Face and GitHub | Model ids in the pinned Lemonade release's model list are Hugging Face repository names (`lemonade.lock:2-3`; the list ships inside that release, not in this repository); decision models name Hugging Face repos `crates/lemonade/src/decision_models.rs:41,52`; the engine's backends and the connection part from GitHub `lemonade.lock:3`, `helpers.lock:18-19` |
| Privacy lists every download | `ui/src/copy/en-GB/privacy.json:14-25`; `src-tauri/src/models/changes.rs:166`; `src-tauri/src/remote/controller.rs:476` |
| Online answers off by default, one provider, own address and key | `docs/adr/0011-online-answers.md:30-35` |
| Up to 12 lines per question | `crates/mind/src/online.rs:11,34-45` |
| Journal entries and good things can be among them; check-ins, reflections, recaps not | `docs/adr/0011-online-answers.md:39-40` |
| Phone questions go online unless turned off | `src-tauri/src/online/controller.rs:961-962`; `crates/contract/src/online.rs:164`; on by default `src-tauri/src/online/mod.rs:182` |
| One model list request per engine start, key only | `crates/lemonade/src/online.rs:7-10`; `docs/adr/0011-online-answers.md:57` |
| Originals never sent, a line read from one may be | `src-tauri/src/ask.rs:740-744` (an original's own lines are held back; memories read from it are not); `docs/design/only-marked-private.md` "The explanation at setup" |
| Health readings never reach Ask or a mind | `crates/mind/src/prompts.rs:280`; `crates/intelligence/src/reader.rs:964-974`; `docs/design/phone-health.md:93-94` |
| Let go and forgetting never sent | `src-tauri/src/ask.rs:533`; `crates/engine/src/online.rs:450-476` |
| Private: do not rely on it on the computer | Today the computer holds private lines back (`src-tauri/src/ask.rs:756-782`, `crates/engine/src/online.rs:189-205`); the proposed revision sends them (`docs/design/only-marked-private.md`). The wording is true before and after |
| Sent online list kept 90 days | `crates/lemonade/src/online.rs:47`; `src-tauri/src/online/controller.rs:1293`; phone `phone/src-tauri/src/mind/sent.rs:1-5` |
| Phone online mind: what goes | `crates/provider/src/mind.rs:43-54,313-330,410-418,456,482,490-497,525-545,559-576`; `docs/adr/0020-an-online-mind-on-a-phone.md:39-44` |
| Phone online mind sends private things | `crates/engine/src/online.rs:455-476`; `phone/src-tauri/src/mind/ask.rs:99-120` |
| Calendar entries can be read by the mind | Calendar items are read as text `crates/mind/src/prompts.rs:279`; `crates/intelligence/src/reader.rs:970` |
| Phone asks for the model list when checking | `crates/provider/src/client.rs:338-366`; `phone/src-tauri/src/mind/mod.rs:558` |
| One host, no redirects, no proxy | `crates/provider/src/client.rs:1-8,215-231`; `phone/src-tauri/src/mind/tests.rs:825-834` |
| Away from home off by default; Simple and Most private | `docs/adr/0010-away-from-home.md:19-28`; `crates/contract/src/remote.rs:16-17` |
| What Cloudflare and Tailscale see | `ui/src/copy/en-GB/privacy.json:70-77` (`:73` the phone page); `docs/adr/0010-away-from-home.md:105-109` |
| Sealed between phone and computer | `crates/channel/src/crypto.rs:11,15,651-707`; `crates/channel/src/client/link.rs:63-68`; `docs/adr/0010-away-from-home.md:44-46` |
| Helper's own traffic | `docs/adr/0010-away-from-home.md:61-63,109-110` |
| Paired phone sends kept things, calendar, health | `phone/src-tauri/src/sources.rs:227-262`; `phone/src-tauri/src/sync/sink.rs:29-54` |
| Moving a phone's memory into a computer | `docs/design/moving-memory.md` "One home holds the pen"; built: `move.rs` in the phone app's `home` module |
| Nightly backup, iCloud and Drive, `drive.file` | `docs/adr/0019-a-locked-copy-online.md:27-31`; `docs/design/phone-backup.md:101-109,176-183`. Not built |
| Phrase, recovery key, nobody else can open | `docs/adr/0019-a-locked-copy-online.md:36-41` |
| Seven nights; forgetting reaches it | `docs/adr/0019-a-locked-copy-online.md:42-44`; `docs/design/phone-backup.md:564-572` |
| Health readings not in the backup | The owner's decision of 2026-11-24. `docs/design/phone-backup.md:91` still lists them and must change ([below](#what-must-change-elsewhere)) |
| Camera usage | `phone/src-tauri/Info.ios.plist:5-6` |
| Microphone usage | `phone/src-tauri/Info.ios.plist:7-8`; `phone/plugins/speech/android/src/main/AndroidManifest.xml:4` |
| Speech on iPhone stays on the phone | `phone/plugins/speech/ios/Sources/SpeechPlugin.swift:43-50,88`; `phone/src-tauri/Info.ios.plist:11-12` |
| Speech on Android: on-device first, else installed language, offline asked | `phone/plugins/speech/android/src/main/java/app/nocturne/speech/SpeechReader.kt:153-156,165`; `phone/plugins/speech/android/src/main/java/app/nocturne/speech/SpeechSeam.kt:62-71` |
| Calendar read only | `phone/plugins/calendar/android/src/main/AndroidManifest.xml:3-4`; `phone/src-tauri/Info.ios.plist:9-10` |
| Health read only, the kinds | `phone/plugins/health/android/src/main/AndroidManifest.xml:3-13`; `phone/plugins/health/android/src/main/java/app/nocturne/health/HealthRows.kt:43-51`; `phone/plugins/health/ios/Sources/HealthPlugin.swift:91-115,180` |
| Internet permission | `phone/src-tauri/gen/android/app/src/main/AndroidManifest.xml:3` |
| Background reading while charging, Android, online mind on | `phone/plugins/system/android/build.gradle.kts:31`; `Background.kt` in `phone/plugins/system/android` (`setRequiresCharging`, scheduled only while the mind is on) |
| Health readings to the paired computer only, or kept on a phone alone | `phone/src-tauri/src/sources.rs:229-236,258`; `phone/src-tauri/src/sync/sink.rs:1-3,29-82` |
| Export, backup to a file | `docs/guide/your-data.md` "Take a copy", "Back up and restore"; phone: `commands.rs:431-464` in the phone app's `home` module |
| Delete asks first; Delete everything needs typed words | `crates/engine/src/export.rs:21,699-705`; `ui/src/copy/en-GB/privacy.json:41-47` |
| Delete everything removes the key, Sent online log and phones | `crates/engine/src/export.rs:709-750`; `src-tauri/src/commands/settings.rs:96-140`; `ui/src/copy/en-GB/privacy.json:112`; phone `phone/src-tauri/src/mind/sent.rs:3-4`, `phone/src-tauri/src/mind/mod.rs:434` |
| Delete everything deletes the nightly backup | `docs/design/phone-backup.md:571` (designed) |
| Turning backup off can delete it | `docs/design/phone-backup.md:359-377` (designed) |
| Provider retention is the provider's | `ui/src/copy/en-GB/privacy.json:108` |
| Exported or printed is out of reach | `ui/src/copy/en-GB/memories.json:10` |
| Site: no cookies, no analytics, no outside scripts or fonts | `site/_headers:5` (CSP: scripts, styles, fonts and images from the site only, `connect-src 'none'`); fonts self-hosted `site/index.html:29-30`; `site/assets/js/lang.js:3` |
| Site: browser storage for theme and language line | `site/assets/js/theme.js:10,34`; `site/assets/js/lang.js:11,52` |
| Site hosted by Cloudflare as static files | `site/wrangler.jsonc:11-15`; `site/_headers:1` |
| Downloads lead to GitHub | `docs/guide/install.md:44-56` |
| Public source history | `README.md:13,44` (the repository is public on GitHub) |

### Claims not verified in code

| Claim | Why not | How to settle |
|---|---|---|
| Models come from Hugging Face | The model list ships inside the pinned Lemonade release; only its checkpoint names show the host | Pin the hosts in a check, or let the release network check record them |
| What Cloudflare (as site host) logs | Cloudflare's own behaviour | Its policy; the page says "may keep" |
| The nightly backup's behaviour | Designed, not built | Publish its two sections only when it ships |
| Health readings left out of the backup | The owner's decision; the design page still lists them | Update `phone-backup.md` and its ADR first |
| Android camera permission | Added at build time by the barcode scanner plugin, not in the repository's manifests | Read the merged manifest of a release build |
| Android speech never reaching a server | The fallback reader is the phone's own service with offline asked, not forced | [Question 4](#open-questions-for-the-owner) |
| Whether the online mind runs on iPhone | ADR 0020 says Android first; no platform gate found in code | The page says "a phone of its own" without naming one |

## Open questions for the owner

| # | Question | Options | Recommendation |
|---|---|---|---|
| 1 | Who is named as the maker (`PUBLISHER_NAME`)? Stores and data protection law expect a named person or company | A: the owner's legal name. B: a company or trading name. C: "the Nocturne project" | **B or A.** Stores reject a policy with no identifiable publisher; C does not meet that |
| 2 | Which address is `SUPPORT_EMAIL`? | A: a dedicated address on the project's domain. B: a forwarding alias. C: the owner's own | **A.** It survives a change of hands and keeps a personal address off a public page |
| 3 | Publish before the nightly backup ships? | A: publish now without the backup and Google sections, add them when it ships. B: publish all, marked "when it ships". C: wait | **A.** A store reviewer reads the page as what the app does today. Google's Drive review needs the Limited Use line before release, so it goes in with the backup |
| 4 | Android voice notes: the fallback reader may be the system's own service, which the code asks to work offline but cannot force | A: keep the fallback and say so (the text above). B: use only the strictly on-device reader and leave notes unread without it | **B.** "Never leaves the phone" can then be said on both phones, and it matches the iPhone |
| 5 | Private on the computer: the code still holds private lines back from Online answers, the revision of 2026-11-23 does not | A: the neutral words above, true before and after. B: describe today's behaviour and change the page with the revision | **A.** One text, no window where the page and the app disagree |
| 6 | Translate the policy? | A: English only, link text translated. B: all six site languages | **A** for now. A translated policy is a legal text in each language; translate when the stores in those countries ask |
| 7 | Data protection wording (controller, legal basis, rights) | A: the plain text above, which says we hold nothing. B: add a short section naming the controller and rights under GDPR and UK GDPR | **B, short.** Name the publisher as controller for the website's hosting only, and say rights requests are rarely needed since we hold nothing |

## What must change elsewhere

- `docs/design/phone-backup.md:91` and `:535`, and `docs/adr/0019-a-locked-copy-online.md`: drop raw
  health readings from what goes each night, and add that a restore reads them again from Health,
  per the owner's decision of 2026-11-24. The policy already says so.
- The phone's Health ask copy (`phone.health.why.where`, `docs/design/phone-first.md:291`) says
  readings go "into ... a nightly backup or a mind". Both are now untrue: readings never reach a
  mind, and will not be in the nightly backup.

## When features change

The policy is part of what a feature changes. A pull request that changes any of these updates this
page and the site page in the same stack, with a new effective date.

| Change | What the policy must say |
|---|---|
| A new host the app or the engine reaches, or a new download source | Add it under "Downloads" or the feature's section |
| Invariant 2 gains or loses a clause | Its own section, in the same words |
| Online answers or the online mind sends a new kind of thing, or stops sending one | The lists under those sections |
| Private changes meaning | The private lines in both online sections, and question 5 |
| The nightly backup ships, or changes place, scope or what it holds | Its section and "Google user data"; remove "when it ships" |
| Apple Intelligence as an iPhone mind (ADR 0020) | A new section: Apple's servers, what goes, Apple's terms |
| A household computer or a borrowed mind (ADR 0021) | A new section: what the person who runs it can see |
| A new phone permission, or a new health or calendar field | The permissions table and the health list |
| Notifications on the phone | Say what they show and that private things are left out |
| Any analytics, crash report or update check | The "In short" list and its own section. Invariant 2 forbids them today |
| The site gains a script, font, form or cookie from elsewhere | "This website" |
