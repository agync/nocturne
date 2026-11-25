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
| The site footer, every page | "Privacy policy", beside "Your data" | One footer link, all languages; the link text is translated, the page stays English until the owner decides otherwise ([question 5](#open-questions-for-the-owner)) |
| The install page | One line under the downloads: "Nocturne keeps what you give it on your own devices. Privacy policy" | One line and link |
| The computer app, Privacy | A quiet link "Privacy policy" at the foot, under the promise line | The app has no About screen; Privacy is the nearest place. Opens the site in the browser |
| The phone app, This phone | A quiet link "Privacy policy" at the foot | Opens the site in the browser. The phone already allows links to `https://nocturne.is/*` |
| The phone's Health Connect rationale page | A link "Privacy policy" under the existing rationale text | Health Connect asks for the policy from this page |
| Google Play and App Store listings | The address `https://nocturne.is/privacy/` | Store console fields, at release |

The computer and phone links are interface changes and wait for this design's approval with the
page. They add one link each and change no other words.

## The public text

The text below is what the page says, word for word. `PUBLISHER_NAME` is a placeholder the owner
has not yet filled in ([question 1](#open-questions-for-the-owner)); the page is not published
while it stands.

---

> # Privacy policy
>
> Effective 25 November 2026.
>
> ## In short
>
> - Nocturne keeps what you give it on your own computer or phone.
> - Nocturne sends us nothing. There is no account with us, and no analytics, ads, crash reports
>   or tracking.
> - Nothing you kept leaves your device unless you turn on something that needs it. Each of those
>   is off until you turn it on, and each sends only to a place you chose.
>
> ## Who we are
>
> Nocturne is free, open-source software made by **[PUBLISHER_NAME]** ("we" or "us"). It runs on
> Linux, Mac and Windows computers, and on Android phones and iPhones.
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
> - your journal: daily entries, good things from your day, check-ins about how you feel, and the
>   reflections and recaps Nocturne writes from them;
> - entries from your phone's calendar and readings from Apple Health or Health Connect, if you
>   let your phone share them;
> - your settings, and small log files that record what Nocturne did, with counts and kinds of
>   error, never what you kept.
>
> On a computer, all of this is in one folder on that computer. On a phone that keeps its own
> memory, it is in Nocturne's own storage on the phone. Nocturne keeps that storage out of the
> phone's own backup to iCloud or Google, and out of copying to a new phone. Removing the app
> removes it from the phone. If you turned on the nightly backup, a locked copy stays in your own
> iCloud or Google Drive until you delete it: see below.
>
> Memories you have not used fade after a time and are then set aside. The app calls this "let
> go". It is not deleting: you can bring them back until you delete them. You choose how long in
> Settings. Nocturne never deletes what you kept, your journal, your pages or what it remembers
> without you asking. It does clear its own records after a time: its log files, and its list of
> what was sent online after 90 days.
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
> Nocturne does not check for updates by itself. You get Nocturne and its updates from where you
> installed it: an app store, our Flatpak repository or GitHub. Each of those sees what any
> download site sees.
>
> ### Online answers, on a computer
>
> This is off until you turn it on. You choose one provider of online answers, and give its
> address and your own key. Your agreement with that provider is your own.
>
> When you set it up, Nocturne checks the connection: it asks the provider for its list of models
> and sends one fixed test question. When you choose from the provider's list, Nocturne asks the
> provider for that list. These carry your key and nothing you kept.
>
> While it is on:
>
> - Each question you ask goes to your provider with today's date, the time and your time zone,
>   and up to 24 short lines that help answer it, with their dates. They can come from anything
>   Nocturne holds: what it remembers from your notes, photos, voice notes, recordings and
>   documents, your calendar entries, and your journal entries, good things and check-ins.
> - When a question names a day or a time, such as "yesterday" or "last week", and a kind of
>   health reading, such as sleep, steps or heart, the readings your phone shared for those days
>   can go with it too.
> - Questions you ask from a paired phone go the same way, unless you turn that off. Things you
>   marked private stay on the computer for those questions.
> - Each time Nocturne starts or restarts its engine, the engine asks your provider for its list of
>   models. That request carries your key and nothing you kept.
> - Your photos, recordings and documents themselves are never sent, though what Nocturne remembers
>   from them can be. The reflections and recaps Nocturne writes are never sent. Nor is anything you
>   let go or asked Nocturne to forget.
>
> Marking something private stops it coming up on its own, for example in notifications. It does
> not keep it from your provider: a private thing can help answer a question you ask on the
> computer like anything else.
>
> Privacy, in the app, keeps a list of exactly what was sent, for 90 days.
>
> ### An online mind, on a phone of its own
>
> A phone with no computer can use an online provider you choose to read what you keep, write your
> morning page and the reflection on your day, and answer your questions. This is off until you
> turn it on. You give the provider's address and your own key. Recaps are written on the phone,
> without the provider.
>
> When you set it up, the phone checks the connection: it asks the provider for its list of models,
> or uses the list you just chose from, and sends one fixed test question. When you choose from the
> provider's list, the phone asks the provider for that list. These carry your key and nothing you
> kept. If the provider stops answering while the mind is on, the phone asks for that list again
> from time to time until it answers.
>
> While it is on, the phone sends that provider what each job needs to be done well:
>
> - each note, journal entry, good thing and calendar entry, to be read, and the words you added to
>   a check-in;
> - each photo you keep;
> - each voice note or recording, as words when the phone has already turned it into words, and
>   otherwise as sound, up to 25 MB;
> - each question, with today's date, the time and your time zone, and the lines that help answer
>   it. When a question names a day or a time, or few lines match its words, the provider may ask
>   to look further in what you kept: your calendar, your journal and check-ins for given days,
>   your health readings, or a search with other words. The phone sends back what it finds, up to
>   60 lines for one question;
> - the lines your page and the reflection on your day are written from.
>
> When the phone sends words to be read, it adds the day they were kept and the names of people,
> places and things already in your memory. Documents stay on the phone. Things you marked
> private go too. Marking something private stops it coming up on its own; it does not hold it
> back from the provider you chose. Health readings go only with a question they help answer.
> Reflections and recaps are never sent. Nor is anything you let go or asked Nocturne to forget.
>
> The phone keeps a list of every request it made, for 90 days.
>
> ### Nightly backup, on a phone of its own
>
> This is off until you turn it on. Each night while the phone charges, Nocturne saves what changed
> to your own account. It does this over mobile data too, unless you turn on "Wi-Fi only". "Back up
> now" saves at once.
>
> - On iPhone, it goes to Nocturne's own space in your iCloud.
> - On Android, it goes to a folder called "Nocturne backup" in your Google Drive. You sign in with
>   Google's own screens, and allow Nocturne to reach only the files it creates. It cannot see
>   anything else in your Drive.
>
> The backup is locked with a phrase only you hold, and a recovery key if you make one. Apple,
> Google and we cannot open it. If you lose both, nobody can. Nocturne keeps the last seven
> nights. When you forget something for good, or delete everything, Nocturne removes it from the
> backup straight away when the phone is online, or the next time it is. When you turn the backup
> off you choose whether to keep it or delete it; while it is kept, forgetting still reaches it.
>
> Your health readings are not in the backup, nor in a backup file you save on a phone. The pages,
> reflections and recaps in it do not hold readings either: Nocturne adds readings beside them when
> you look. After a restore, Nocturne reads your readings again from Apple Health or Health
> Connect, if you allow it. Anything you wrote about your health yourself, in a note or your
> journal, is in the backup like everything else you kept.
>
> ### Away from home
>
> This is off until you turn it on. It lets your own paired phones reach your computer when you
> are out. You choose how they connect:
>
> - **Simple** goes through Cloudflare. Cloudflare can see when your phone connects, its internet
>   address and roughly where it is, a fixed number for each phone, and how much goes each way and
>   when. If you use Nocturne in your phone's web browser rather than the phone app, Cloudflare
>   also delivers that app's screens to the browser, and so could change them.
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
> ## What your phone asks to use
>
> Your phone asks you before Nocturne can use each of these. Saying no turns off only the part that
> needs it.
>
> | Permission | Why |
> |---|---|
> | Camera | To scan the code on your computer's screen when you pair, and to take photos you choose to keep. |
> | Microphone | To record voice notes you choose to keep, while you record. On Android the phone's speech reader also needs it to turn a kept voice note into words; while it checks it can read, the microphone may be open for a moment, and nothing from it is kept. |
> | Speech recognition (iPhone) | To turn your voice notes into words on the iPhone, so you can find them. |
> | Calendar | To read your calendar, so your page and journal can show what's on. Entries go to your paired computer if you pair one, and can go to your online provider with what it reads or a question they help answer. Nocturne never changes your calendar. |
> | Apple Health or Health Connect | To read the kinds of reading you choose, to show them beside your journal and, for sleep, last night's sleep on your page, and to answer your questions about them. Readings go to your paired computer if you pair one, and can go to your online provider with a question they help answer. Nocturne never writes to Health. |
> | Google Drive (Android) | Only if you turn on the nightly backup: to save, list, restore and delete your locked backup in the files Nocturne creates. |
> | Notifications (Android) | Asked when you first tap Read now, to show how far reading has got, with Stop, while it reads in the background. |
> | Local network (iPhone) | Only if you choose an online provider on your own home network, to reach it. |
> | Internet (Android) | To reach your computer, your online provider if you choose one, and your Google Drive if you turn on the nightly backup. |
>
> With an online mind turned on, an Android phone reads what you kept in the background while it
> charges. If you tap Read now, the phone also reads on battery, until it is done or you stop it.
> On Android it shows a notification with how far it has got and Stop. On iPhone it shows iPhone's
> own progress, or reads for a short while and then waits until you open Nocturne. Read now stops
> when the battery reaches 20 per cent or battery saving is on. Read anyway goes on, never below 5
> per cent. These notifications never name what you kept.
>
> Turning voice notes into words happens on your phone. On iPhone it never leaves the phone. On
> Android Nocturne uses the phone's own on-device reader. Where a phone has none, Nocturne uses the
> phone's speech service, only for a language already installed on the phone, and asks it to work
> offline; Android does not force that service to stay offline. With an online mind on, a voice
> note the phone could not turn into words goes to your provider, as described above.
>
> To take a permission back: on Android, your calendar, the microphone and notifications in
> Settings, then Apps, then Nocturne, then Permissions, and Health in Health Connect, then App
> permissions, then Nocturne. On iPhone, your calendar, the microphone and speech recognition in
> Settings, then Nocturne, and Health in the Health app, then Sharing, then Apps, then Nocturne. You
> can also turn Calendar or Health off in This phone. Taking a permission back stops new reads. It
> does not delete what Nocturne already kept: see below.
>
> ## Health information
>
> If you allow it, Nocturne reads these from Apple Health or Health Connect: sleep, heart rate,
> resting heart rate, blood pressure, steps, workouts with their distance, and weight. It reads
> only the kinds you allow, and never writes, changes or deletes anything in Health.
>
> At first Nocturne reads the last 30 days. On Android, Nocturne asks to read readings older than
> that together with the other kinds, wherever Health Connect offers it. It reads up to a year back
> if you turn on "Also bring older readings", and as far back as your backup reached when it reads
> your readings again after a restore.
>
> - On a phone paired with a computer, readings go only to that computer, locked so only it can
>   read them. If you turned on Online answers there, a reading can go to your provider with a
>   question it helps answer, as described above.
> - On a phone of its own, readings stay in Nocturne's memory on the phone. If you turned on an
>   online mind, a reading can go to your provider with a question it helps answer, as described
>   above.
> - Readings go online only with a question you asked. They are never sent to be read, and never
>   used to write your page, your reflections or your recaps. They are not in the nightly backup,
>   nor in a backup file a phone saves.
> - Health information is never used for advertising and never sold. It goes nowhere but the
>   places above. We never receive it.
>
> To delete readings Nocturne already kept: on a phone of its own, "Delete what was kept" in This
> phone; on a computer, "From your phones".
>
> ## Google user data
>
> Nocturne on Android uses Google Drive with access only to the files it creates. It uses that
> access only to save, list, restore and delete your own locked backup. Nocturne's use and transfer
> to any other app of information received from Google APIs will adhere to the
> [Google API Services User Data Policy](https://developers.google.com/terms/api-services-user-data-policy),
> including the Limited Use requirements. Nothing from your Drive is used for advertising, sold,
> read by people, or sent to us.
>
> ## Seeing, exporting and deleting your data
>
> Nocturne sends us nothing, so you never need to ask us to see or delete what you keep in it. You
> do it in Nocturne:
>
> - **See it.** Everything Nocturne remembers leads back to what it came from. Privacy on a
>   computer shows what was downloaded and what was sent online. This phone, on a phone, shows
>   what was sent online.
> - **Take a copy.** On a computer, Export everything writes readable files to a folder you choose.
>   On a computer or a phone, a backup saves one locked file wherever you choose. A phone's backup
>   file leaves health readings out.
> - **Let go and delete.** You can let go of a thing, bring it back, or delete it for good.
>   Deleting asks first.
> - **Delete everything.** Removes everything you kept, your journal, what Nocturne remembers, your
>   pages and your settings from that device, after you type the words it shows. It also forgets
>   the key for your online provider and the list of what was sent online. On a computer it
>   removes your paired phones. On a phone with a nightly backup, it deletes the backup too, at once
>   when the phone is online, or the next time it is. What Nocturne downloaded to work, and its log
>   files, stay until you remove Nocturne.
>
> ## Copies held by others you chose
>
> Nocturne cannot reach copies outside your devices, except your own nightly backup. Their own
> policies apply:
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
> request and may keep standard records such as your internet address. Our Flatpak repository,
> flatpak.nocturne.is, is served by GitHub through Cloudflare, and the Flatpak itself comes from
> GitHub; your software centre checks there for updates. The other download links lead to GitHub,
> under GitHub's own terms.
>
> ## Data protection
>
> For this website, our Flatpak repository and emails you send us, **[PUBLISHER_NAME]** is the data
> controller. We rely on our legitimate interest in running the site and answering you. Cloudflare
> and GitHub may handle requests outside the UK and the EEA. We keep your emails only as long as we
> need to answer you. You can ask us to see, correct or erase an email you sent us, and you can
> complain to the UK Information Commissioner's Office or the data protection authority where you
> live.
>
> ## Children
>
> Nocturne is not made for children under 13, or under 16 where the law sets that age. Since
> Nocturne sends us nothing, we do not know who uses it.
>
> ## Changes to this policy
>
> When Nocturne changes what it keeps or sends, this page changes with it, and the date at the top
> changes. Earlier versions stay in Nocturne's public source history.
>
> ## Contact
>
> Questions about this policy or your privacy: hello@nocturne.is.

---

## Evidence

Every claim on the page, and where the repository shows it, as file and function, constant, key
or section. Paths are from the repository root.

| Claim | Evidence |
|---|---|
| Nocturne sends us nothing; local only | `AGENTS.md` invariant 2 |
| No analytics, no crash reports, no account (computer) | `ui/src/copy/en-GB/privacy.json` `privacy.promise`, `privacy.promise.online`; `docs/guide/your-data.md` "How you can tell"; no analytics or crash code found by grep in `crates`, `src-tauri`, `phone`, `ui/src` |
| No analytics or ads (phone) | `ui/src/copy/en-GB/phone-health.json` `phone.rationale.where`; `ui/src/copy/en-GB/phone-alone.json` `phone.alone.rationale.where`; phone web view CSP in `phone/src-tauri/tauri.conf.json` `app.security.csp` |
| The engine's own telemetry is off | `crates/lemonade/src/config.rs`: `write` (`telemetry.enabled` false) |
| No update check by the app | `ui/e2e/privacy.spec.ts`; `crates/engine/src/settings.rs`: `check_updates` defaults false and nothing reads it to reach the network; `crates/lemonade/src/config.rs`: `write` (`auto_check_model_updates`, `auto_update_models` false) |
| Open source, Apache 2.0 | `LICENSE`; `site/i18n/en.json` licence line |
| What is kept: notes, photos, screenshots, voice, audio, documents | `docs/design/concept.md` |
| Journal, check-ins, reflections, recaps, pages | `crates/engine/src/store/schema.sql`: tables `pages`, `reflections` |
| Calendar entries and health readings from phones | `crates/engine/src/store/schema.sql`: table `phone_records`; items of kind `calendar` and `reading` |
| Logs record what Nocturne did, counts and error kinds, never what was kept; cleared over time | `src-tauri/src/logging.rs`: module note and `init` (daily files, seven kept); `phone/src-tauri/src/log.rs`: module note, `MAX_BYTES` (one file, restarted past its size, the one before kept) |
| One folder on the computer | `docs/guide/your-data.md` "Where it is" |
| Removing the phone app removes its memory | `ui/src/copy/en-GB/phone-alone.json` `phone.alone.thisPhone.removing` |
| Phone store out of device backup and device transfer | `phone/src-tauri/gen/android/app/src/main/AndroidManifest.xml` (`allowBackup`, `fullBackupContent`, `dataExtractionRules`); `phone/src-tauri/gen/android/app/src/main/res/xml/data_extraction_rules.xml`; `phone/src-tauri/src/lib.rs`: `exclude_from_backup`; `phone/plugins/system/ios/Sources/SystemPlugin.swift` (the folder kept out of the backup) |
| Fading and letting go, setting, bring back; never deleted without asking | `crates/engine/src/lifecycle.rs`: `Engine::decay`, `Engine::restore`; `docs/guide/your-data.md` "Let go" |
| Sent online list cleared after 90 days | `src-tauri/src/online/controller.rs`: `prune`; `crates/lemonade/src/online.rs`: `SENT_KEPT_DAYS`; `phone/src-tauri/src/mind/sent.rs`: `KEEP_DAYS` |
| Downloads only after agreement, size first | `docs/guide/choosing-models.md`; engine offline except during an approved download, `crates/lemonade/src/config.rs`: `Settings::offline`, `write` |
| Downloads from Hugging Face and GitHub | Model ids in the pinned Lemonade release's model list are Hugging Face repository names (`lemonade.lock`; the list ships inside that release, not in this repository); `crates/lemonade/src/decision_models.rs` names Hugging Face repositories; the engine's backends and the connection part from GitHub, `lemonade.lock`, `helpers.lock` |
| Privacy lists every download | `ui/src/copy/en-GB/privacy.json` `privacy.ledger.*`; `src-tauri/src/models/changes.rs`: `fetch`; `src-tauri/src/remote/controller.rs`: `download` |
| Online answers off by default, one provider, own address and key | `docs/adr/0011-online-answers.md` |
| Today's date, time and time zone with every question | `crates/mind/src/prompts.rs`: `today_line`, `answer_frame`; `docs/design/ask-with-tools.md` "Today's date and time zone, in every answer" |
| Up to 24 lines per question on the computer | `crates/lemonade/src/mind/online.rs`: `OnlineMind::prepare` (`MAX_FACTS + DATE_STEP_LINES`); `crates/mind/src/online.rs`: `MAX_FACTS` (12); `crates/engine/src/lookup.rs`: `DATE_STEP_LINES` (12) |
| Lines come from any memory and from journal entries, good things and check-ins, with dates | `src-tauri/src/ask.rs`: `dated`, `ask_online`; `crates/engine/src/lookup.rs`: `look_up_journal`, `checkin_line`, `writing_line`; `crates/engine/src/online.rs`: `may_send` (a kept line goes like a memory); `ui/src/copy/en-GB/online.json` `online.consent.sent.body`; `site/i18n/en.json` `journal.quiet.detail`, `promise.extras.detail` |
| Health readings go only when the question names a time and a kind (computer) | `crates/engine/src/datestep.rs`: `date_step`, `KINDS`, `WORKOUT`; `crates/engine/src/lookup.rs`: `date_step_lines` (readings only for `step.kinds`), `look_up_health`; `src-tauri/src/ask.rs`: `dated`. The computer does not offer look-ups yet: `docs/design/ask-with-tools.md` status, "Look-ups on the computer" |
| Reflections and recaps never sent | `crates/engine/src/lookup.rs` module note (never items, so never lines); `crates/intelligence/src/reflect.rs` module note |
| The setup check: model list and one fixed question, key only | `crates/mind/src/online.rs`: `CHECK_QUESTION`; `src-tauri/src/online/controller.rs`: `check`; `docs/adr/0011-online-answers.md` |
| The list again when the person chooses from it (computer) | `src-tauri/src/online/controller.rs`: `list`; `src-tauri/src/commands/online.rs`: `list_online_models`; `AGENTS.md` invariant 2 |
| Phone questions go online unless turned off; private stays for them | `crates/contract/src/online.rs`: `OnlineView::phones`; on by default `src-tauri/src/online/mod.rs`: `off_view`; `src-tauri/src/ask.rs`: `answer` (`public_only` drops private hits), `dated` (`LookScope::COMPANION`, `phone_safe`); `docs/logs/2026-11/25-let-private-things-answer-online.md` |
| A model list request each time the engine starts or restarts, key only | `crates/lemonade/src/online.rs`: `write_row`, `entry`; `docs/adr/0011-online-answers.md` |
| Originals never sent, memories read from them may be | `src-tauri/src/ask.rs`: `ask_online` (lines of originals, `is_source_line`, stay here); `docs/design/only-marked-private.md` |
| Private things sent for the computer's own questions | `crates/engine/src/online.rs`: `may_send`, `Engine::sendable` (only let go, being forgotten or unknown are held back); `ui/src/copy/en-GB/online.json` `online.stays.markPrivate` |
| Let go and forgetting never sent | `crates/engine/src/online.rs`: `may_send`, `Engine::mind_may_read`; `crates/engine/src/lookup.rs`: `kept_line_may_go` |
| Phone online mind: what goes | `crates/provider/src/mind.rs`: `read_image_impl`, `transcribe_impl`, `answer_impl`, `headline_impl`, `reflect_impl`, `extract`; `docs/adr/0020-an-online-mind-on-a-phone.md` |
| Phone online mind: documents stay | `crates/provider/src/mind.rs`: `read_document` (`NotTaken`) |
| Phone online mind: recaps written on the phone; check-ins left out of the day's reflection; a check-in's own words read | `crates/intelligence/src/reflect.rs` module note; `crates/engine/src/journal/lookback.rs`: `reflect_items`; `crates/engine/src/journal/keep.rs`: `insert_checkin` |
| Phone online mind: words already transcribed go as words; sound otherwise, up to 25 MB | `crates/provider/src/mind.rs`: `MAX_RECORDING`, `transcribe_impl` |
| Phone online mind: the day kept and known names go with words to be read | `crates/mind/src/prompts.rs`: `extraction_user` |
| Phone online mind: look-ups, health included, up to 60 lines | `phone/src-tauri/src/mind/ask.rs`: module note, `look_up`; `crates/mind/src/lookup.rs` (the four look-ups); `crates/provider/src/lookup.rs`: `LookUps`; `docs/design/ask-with-tools.md` "The look-ups", "Limits" |
| Phone online mind: health readings never read, never in a page or reflection | `crates/mind/src/prompts.rs`: `extraction_user`; `crates/intelligence/src/reader.rs`: `process` (a reading is understood without the mind); `crates/engine/src/journal/mod.rs`: `items_in_days` (no readings among a day's items); `crates/provider/src/mind.rs`: `Handed::of_page` |
| Phone online mind sends private things | `crates/engine/src/online.rs`: `Engine::mind_may_read`; `phone/src-tauri/src/mind/ask.rs` (`LookScope::ALL`) |
| Calendar entries can be read by the mind | `crates/mind/src/prompts.rs`: `extraction_user` (`ItemKind::Calendar`) |
| Phone setup check: model list or the person's own look, and one fixed question; the list on request; looks again while the provider is unreachable | `crates/provider/src/client.rs`: `Provider::models`, `Provider::list`, `Provider::check_with`; `phone/src-tauri/src/mind/mod.rs`: `PhoneMind::list`, `look_again`; `phone/src-tauri/src/mind/commands.rs`: `mind_list` |
| One host, no redirects, no proxy | `crates/provider/src/client.rs`: module note, `client` (`redirect::Policy::none`) |
| Nightly backup: iCloud space and "Nocturne backup" in Drive, files the app creates only | `phone/src-tauri/src/backup/place_account.rs` module note; `phone/plugins/system/android/src/main/java/app/nocturne/system/Drive.kt`: `FOLDER`; `DriveAccount.kt`: `SCOPE` (`drive.file`), Google's `AuthorizationClient`; `phone/src-tauri/gen/apple/nocturne-phone_iOS/nocturne-phone_iOS.entitlements` (`iCloud.is.nocturne.phone`) |
| Nightly backup: each night while charging, Wi-Fi only, Back up now | `phone/src-tauri/src/backup/mod.rs`: module note, `PhoneBackup::pass_with`, `waits_for_wifi`; `phone/plugins/system/android/src/main/java/app/nocturne/system/Background.kt`: `Background.schedule` |
| Nightly backup: phrase, recovery key, nobody else can open | `crates/engine/src/nightly/lock.rs` module note, `PHRASE_WORDS`, `KEY_RECOVERY` |
| Nightly backup: seven nights; a forget reaches it at once, or when online; kept while off | `crates/engine/src/nightly/mod.rs`: module note, `NIGHTS_KEPT`, `FORGOT`; `phone/src-tauri/src/backup/mod.rs`: `pass_with`, `kept_off` |
| Health readings not in the backup nor a phone's backup file; read again after a restore | `crates/engine/src/nightly/mod.rs`: `leave_out_health`, `HEALTH_SINCE`; `docs/design/phone-backup.md` "Health readings after a restore" |
| Stored pages and recaps hold no readings; readings are added when viewed | `crates/engine/src/page.rs`: `stored_body`, `fill_phone_lines`; `crates/engine/src/journal/lookback.rs`: `recap_view`, `compose_recap_to_keep` |
| Older readings asked for with the kinds where offered | `phone/src-tauri/src/sources.rs`: `health_request` (`history \|\| health.older_available()`); `phone/plugins/health/android/src/main/java/app/nocturne/health/HealthPlugin.kt`: `request` |
| Away from home off by default; Simple and Most private | `docs/adr/0010-away-from-home.md`; `crates/contract/src/remote.rs`: `RemoteConnection` |
| What Cloudflare and Tailscale see; Cloudflare serves the browser app's screens and could change them | `ui/src/copy/en-GB/privacy.json` `privacy.away.*`; `src-tauri/src/remote/static_files.rs` module note; `docs/adr/0010-away-from-home.md` |
| Sealed between phone and computer | `crates/channel/src/crypto.rs`: `seal`, `open`; `crates/channel/src/client/link.rs`; `docs/adr/0010-away-from-home.md` |
| Helper's own traffic | `docs/adr/0010-away-from-home.md` |
| Paired phone sends kept things, calendar, health | `phone/src-tauri/src/sources.rs`: `send`; `phone/src-tauri/src/sync/sink.rs`: `send_calendar`, `send_readings` |
| Moving a phone's memory into a computer | `docs/design/moving-memory.md` "One home holds the pen"; `move.rs` in the phone app's `home` module |
| Camera usage | `phone/src-tauri/Info.ios.plist` `NSCameraUsageDescription`; Android through the barcode scanner plugin, `phone/src-tauri/Cargo.toml` |
| Microphone usage, and on Android for reading a kept file, open for a moment during the check | `phone/src-tauri/Info.ios.plist` `NSMicrophoneUsageDescription`; `phone/plugins/speech/android/src/main/AndroidManifest.xml`; `phone/plugins/speech/android/src/main/java/app/nocturne/speech/SpeechReader.kt`: `submit`, `installed` |
| Speech on iPhone stays on the phone | `phone/plugins/speech/ios/Sources/SpeechPlugin.swift`: `transcribe` (`requiresOnDeviceRecognition`); `phone/src-tauri/Info.ios.plist` `NSSpeechRecognitionUsageDescription` |
| Speech on Android: on-device first, else installed language, offline asked, not forced | `phone/plugins/speech/android/src/main/java/app/nocturne/speech/SpeechReader.kt`: module note, `recognizer`, `intent` (`EXTRA_PREFER_OFFLINE`); `SpeechSeam.kt` |
| Calendar read only; to the paired computer or the online provider | `phone/plugins/calendar/android/src/main/AndroidManifest.xml`; `phone/src-tauri/Info.ios.plist` `NSCalendarsFullAccessUsageDescription`; `phone/src-tauri/src/sync/sink.rs`: `send_calendar`; `crates/engine/src/lookup.rs`: `look_up_calendar` |
| Notifications on Android, asked at Read now | `phone/plugins/system/android/src/main/AndroidManifest.xml` (`POST_NOTIFICATIONS`, `FOREGROUND_SERVICE_DATA_SYNC`); `phone/plugins/system/android/src/main/java/app/nocturne/system/SystemPlugin.kt`: `askForNotes`; `Reading.kt`: `shouldAsk` |
| Local network on iPhone, only for a provider on the home network | `docs/adr/0020-an-online-mind-on-a-phone.md`; `crates/provider/src/address.rs`: `OWN_NETS`, `OWN_NAMES` |
| Background reading while charging, Android, online mind on | `phone/plugins/system/android/src/main/java/app/nocturne/system/Background.kt`: `Background.schedule` (`setRequiresCharging`, the mind's work only while it is on) |
| Read now on battery: 20 per cent, battery saving, Read anyway to 5 per cent; Android notification with Stop; iPhone progress or a short while | `phone/src-tauri/src/mind/read_now.rs`: `LOW`, `FLOOR`, `battery_stop`; `phone/src-tauri/src/mind/mod.rs`: `PhoneMind::read_now`, `read_now_holds`; `phone/plugins/system/android/src/main/java/app/nocturne/system/ReadingService.kt`; `phone/plugins/system/ios/Sources/Reading.swift`; `docs/design/mind-online-setup.md` "Problem 3: reading on battery" |
| Notifications never name a kept thing | `docs/design/mind-online-setup.md` "Problem 3: reading on battery"; `docs/logs/2026-11/24-designed-read-now-on-battery.md` |
| Only last night's sleep on the page | `crates/engine/src/page.rs`: `fill_phone_lines` (`sleep_last_night`); `ui/src/copy/en-GB/phone-health.json` `phone.health.why.for`, `phone.health.kind.sleep.hint` |
| First read 30 days, a year with older readings on | `phone/src-tauri/src/sync/mod.rs`: `HEALTH_DAYS`, `HEALTH_DAYS_OLDER`; `phone/plugins/health/android/src/main/AndroidManifest.xml` |
| Taking a permission back, and where | `ui/src/copy/en-GB/phone-alone.json` `phone.alone.permissions.android`, `phone.alone.permissions.iphone`; `ui/src/copy/en-GB/phone-health.json` `phone.rationale.stopping` |
| Deleting kept readings | `ui/src/copy/en-GB/phone-alone.json` `phone.alone.deleteKept.title.health`; `ui/src/copy/en-GB/phones.json` `screen.phones.title` |
| Health read only, the kinds | `phone/plugins/health/android/src/main/AndroidManifest.xml`; `phone/plugins/health/android/src/main/java/app/nocturne/health/HealthRows.kt`; `phone/plugins/health/ios/Sources/HealthPlugin.swift`: `sample`, `toRead`, `requestAuthorization(toShare: [])` |
| Internet permission | `phone/src-tauri/gen/android/app/src/main/AndroidManifest.xml` `INTERNET` |
| Health readings to the paired computer only, or kept on a phone alone | `phone/src-tauri/src/sources.rs`: `send`; `phone/src-tauri/src/sync/sink.rs` module note |
| Export (computer only), backup to a file (both) | `docs/guide/your-data.md` "Take a copy", "Back up and restore"; phone `ui/src/copy/en-GB/phone-alone.json` `phone.alone.backup.*`; `save_backup_file` in `commands.rs` of the phone app's `home` module |
| This phone shows what was sent online | `ui/src/copy/en-GB/phone-mind.json` `phone.mind.sent.title`, `phone.mind.sent.dek` |
| Delete asks first; Delete everything needs typed words | `crates/engine/src/export.rs`: `Engine::delete_everything`; `ui/src/copy/en-GB/privacy.json` `privacy.delete.*` |
| Delete everything removes the key, Sent online log and phones; keeps downloads and logs | `crates/engine/src/export.rs`: `Engine::delete_everything`; `src-tauri/src/commands/settings.rs`: `delete_everything`; `ui/src/copy/en-GB/privacy.json` `privacy.delete.dialog.online`; `docs/guide/your-data.md` "Delete everything"; phone `phone/src-tauri/src/mind/sent.rs` module note |
| Delete everything deletes the nightly backup, at once or when online | `commands.rs` of the phone app's `home` module (`PhoneBackup::erase` after `Deleted`); `phone/src-tauri/src/backup/mod.rs`: `erase`, `delete_pending` |
| Turning backup off can delete it | `phone/src-tauri/src/backup/mod.rs`: `turn_off`; `docs/design/phone-backup.md` "Turn off" |
| Google Drive used only to save, list, restore and delete | `phone/plugins/system/android/src/main/java/app/nocturne/system/Drive.kt` (its calls); `crates/engine/src/nightly/place.rs`: `BackupPlace` |
| Provider retention is the provider's | `ui/src/copy/en-GB/privacy.json` `privacy.online.keeps` |
| Exported or printed is out of reach | `ui/src/copy/en-GB/memories.json` `forget.stays.out` |
| Site: no cookies, no analytics, no outside scripts or fonts | `site/_headers` (CSP: scripts, styles, fonts and images from the site only, `connect-src 'none'`); fonts self-hosted, `site/index.html`; `site/assets/js/lang.js` |
| Site: browser storage for theme and language line | `site/assets/js/theme.js`; `site/assets/js/lang.js` |
| Site hosted by Cloudflare as static files | `site/wrangler.jsonc`; `site/_headers` |
| Contact address | The owner, 2026-11-24 |
| Downloads lead to GitHub | `docs/guide/install.md` |
| Flatpak repository: index on GitHub Pages behind Cloudflare, the app from GitHub's registry; software centres check it | `site/install.html`; `docs/contribution/flatpak.md` |
| Updates come from where it was installed | No update check in the app (above); `docs/guide/install.md` |
| Public source history | `README.md` (the repository is public on GitHub) |

### Claims not verified in code

| Claim | Why not | How to settle |
|---|---|---|
| Models come from Hugging Face | The model list ships inside the pinned Lemonade release; only its checkpoint names show the host | Pin the hosts in a check, or let the release network check record them |
| What Cloudflare (as site host) logs | Cloudflare's own behaviour | Its policy; the page says "may keep" |
| Cloudflare and GitHub handling requests outside the UK and EEA; how long emails are kept | Hosting companies' behaviour; an owner commitment | [Question 6](#open-questions-for-the-owner) |
| Android camera permission | Added at build time by the barcode scanner plugin, not in the repository's manifests | Read the merged manifest of a release build |
| iPhone's local network prompt | iOS shows it on first reach to a home address; the plist has no purpose string for it | Add `NSLocalNetworkUsageDescription` ([below](#what-must-change-elsewhere)) |
| Android speech never reaching a server | The fallback reader is the phone's own service with offline asked, not forced | [Question 3](#open-questions-for-the-owner) |
| Read now and the backup on a real iPhone | `Reading.swift` and the iCloud place say they are not yet tried on an iPhone | Try both on an iPhone before the App Store submission |
| Read now's notifications never name a kept thing | The words come from the core as copy; no test pins that they name nothing kept | A test over the notification strings |

## Open questions for the owner

| # | Question | Options | Recommendation |
|---|---|---|---|
| 1 | Who is named as the maker and data controller (`PUBLISHER_NAME`)? Google Play expects the developer named on the listing; data protection law expects the controller's identity and contact details, which some readers take to include a postal address | A: the owner's legal name. B: a company or trading name. C: "the Nocturne project" | **A or B, matching the store listings, with a postal or service address if the owner is willing.** C does not identify anyone. The page is not published while the placeholder stands |
| 2 | The App Store's guideline 5.1.3(ii) bars personal health information in iCloud. Raw readings stay out of the backup, and stored pages, reflections and recaps hold none. A person's own words about their health (a note, a journal entry, a memory read from it) are in the backup, and Apple's reviewer could read the rule as covering them | A: keep the owner's decision as it stands and explain it in the review notes: no HealthKit data is stored, only what the person wrote. B: on iPhone, back up to a file the person picks instead of iCloud. C: leave out memories and entries about health. C breaks the rule that Nocturne never judges what a person's data is about | **A.** It is true to the code and the decision. If Apple rejects it, B is the fallback; C is ruled out by the owner's rule of 2026-11-22 |
| 3 | Android voice notes: the fallback reader may be the system's own service, which the code asks to work offline but cannot force | A: keep the fallback and say so (the text above). B: use only the strictly on-device reader and leave notes unread without it | **B.** "Never leaves the phone" can then be said on both phones, and it matches the iPhone |
| 4 | Private on the computer | Settled on 2026-11-25: the computer sends private things with its own questions, as decision 0011 (amended 2026-11-23) says, and the text above says so. A paired phone's questions still leave them on the computer until that phone's change is built | None; reopen when a paired phone's private change ships ([When features change](#when-features-change)) |
| 5 | Translate the policy? | A: English only, link text translated. B: all six site languages | **A** for now. A translated policy is a legal text in each language; translate when the stores in those countries ask |
| 6 | Data protection: the section names the controller, the basis, transfers, retention and rights. Two lines are the owner's commitments, not facts in the code: how long emails are kept, and that Cloudflare and GitHub may handle requests outside the UK and EEA | A: keep the section as written. B: name a period ("deleted within a year") and the transfer safeguard. C: drop the section | **A**, confirmed by the owner. B if a reviewer asks for a period. C leaves the website's and the mailbox's records undescribed |
| 7 | Health readings to a provider the person chose. With Ask's look-ups, a reading read from HealthKit or Health Connect can go to an online provider with a question. The stores' rules: Apple 5.1.2(i) asks apps to "clearly disclose where personal data will be shared with third parties, including with third-party AI, and obtain explicit permission before doing so"; 5.1.2(vi) and 5.1.3(i) bar HealthKit data for advertising, marketing or data mining, and ask apps to disclose the health data they collect; Apple's developer agreement adds HealthKit terms on third parties that the owner should read at submission. Google Play's Health Connect policy bars "sharing health data with third parties without explicit, informed user consent", and the Data safety form must list health data that leaves the phone | A: keep the behaviour and rely on what is already said: the consent steps name health readings (`online.consent.sent.body`, `phone.mind.what.sent`), the Health asks name the online path (`phone.health.why.where`), and this policy; declare health data as shared with the provider the person chooses in Play's Data safety form and Apple's privacy details, and say so in both review notes. B: a separate yes for readings to go online, off until the person turns it on, through design. C: keep readings off the online path on phones | **A.** The person names the provider, gives their own key, and agrees to a step that names health readings, which meets the explicit and informed consent both stores ask for. Fix the Health Connect rationale and the iPhone purpose string first ([below](#what-must-change-elsewhere)), since a reviewer reads those. B is the fallback if a reviewer asks for consent specific to health. C goes against the owner's decision that Ask reads readings on every mind |

## What must change elsewhere

- The Health Connect rationale pages are what Google's reviewer reads, and both are now untrue.
  `phone.rationale.where` says readings go "only to the computer you paired" and "nobody else
  receives them"; with Online answers on there, a reading can go to the provider.
  `phone.alone.rationale.where` says readings are "in any backup you save" and "nobody else
  receives them"; a backup file leaves them out, and an online mind can send one with a question.
  Both need new words, through design, before the next store submission.
- `phone.alone.health.why.where` says readings "go wherever your memory goes: into a backup you
  save, a locked copy or a mind". The backup file and the nightly backup leave readings out; a
  mind gets one only with a question.
- The iPhone purpose strings in `phone/src-tauri/Info.ios.plist` and the generated
  `phone/src-tauri/gen/apple/nocturne-phone_iOS/Info.plist` say calendar and health go to "your
  computer", and health "only to your own computer". On a phone of its own they stay on the
  phone, and calendar entries and readings can go to an online provider. They need new words,
  through design, before the next store submission.
- `phone/src-tauri/Info.ios.plist` has no `NSLocalNetworkUsageDescription`. iOS asks for local
  network access when the phone's mind reaches a provider on the home network, and without the key
  shows its own words. It needs a purpose string, through design.
- The iPhone lists only `app.nocturne.readnow.*` in `BGTaskSchedulerPermittedIdentifiers`
  (`phone/src-tauri/gen/apple/project.yml`), not `app.nocturne.mind`, so
  `phone/plugins/system/ios/Sources/Background.swift` registers no task. Reading while charging and
  the nightly backup then run on an iPhone only while Nocturne is open. The page says background
  reading while charging on Android only, and "each night while the phone charges" for the backup;
  add the name, or say on the page that an iPhone backs up while Nocturne is open.

## When features change

The policy is part of what a feature changes. A pull request that changes any of these updates this
page and the site page in the same stack, with a new effective date.

| Change | What the policy must say |
|---|---|
| A new host the app or the engine reaches, or a new download source | Add it under "Downloads" or the feature's section |
| Invariant 2 gains or loses a clause | Its own section, in the same words |
| Online answers or the online mind sends a new kind of thing, or stops sending one | The lists under those sections, and "Health information" for readings |
| Look-ups on the computer ship (the pinned engine passes them on, or a size passes the look-up check) | "Online answers, on a computer": the provider may then ask to look further, readings included, and the line count changes |
| Private changes meaning, or a paired phone's private change ships | The private lines in both online sections, the paired phone bullet, and question 4 |
| The nightly backup changes place, scope, schedule or what it holds | "Nightly backup", "Google user data", the permissions table and "Copies held by others" |
| Read now or background reading changes its limits, or runs in the background on iPhone | The paragraph under the permissions table |
| Apple Intelligence as an iPhone mind (ADR 0020) | A new section: Apple's servers, what goes, Apple's terms |
| A household computer or a borrowed mind (ADR 0021) | A new section: what the person who runs it can see |
| A new phone permission, or a new health or calendar field | The permissions table and the health list |
| Notifications on the phone beyond Read now | Say what they show and that private things are left out |
| Any analytics, crash report or update check | The "In short" list and its own section. Invariant 2 forbids them today |
| The site gains a script, font, form or cookie from elsewhere | "This website" |
