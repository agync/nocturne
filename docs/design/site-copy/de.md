---
type: Design
title: Site copy, Deutsch
description: The approved German words for the landing page, the install page and the not-found page, with the glossary they follow.
status: stable
tags: [approved, copy, site, localisation, de]
---

# Site copy, Deutsch

The German words for the pages described in [the site in five languages](../site-localisation.md).
The strings file `site/i18n/de.json` carries the same words; change both together.

## Register and conventions

- Register: Du, kleingeschrieben. A calm consumer app that sits in someone's day reads warmer with du, as the English reads warm with you; Sie would make the privacy promises sound like a contract. Confirmed by the owner.
- Quotes „…“, inner ‚…‘. Apostrophe ’.
- Numbers: 5,2 GB; a no-break space between number and unit. Dates: Freitag, 13. November.
- Keys: `Strg Umschalt Leertaste`, and `Cmd Umschalt Leertaste` on a Mac, as the keys and Windows name them.
- Nocturne takes no article in running text and is referred to as es (the app).
- A plain space before a unit, and in French before `: ; ? !` and inside « », stands for a no-break space; the build writes it.
- `code` marks a key or a command, **bold** a word the system shows, [brackets] the text of a link, *italic* the emphasised half of the hero title.
- ◊ quotes the operating system's own words. L appears only on translated pages.

## Glossary

Each term is rendered the same way on every page.

| English | Deutsch | Why |
|---|---|---|
| keep (verb, button) | festhalten; button „Festhalten“ | Capturing a thought in passing. Behalten implies already owning it, speichern sounds like saving a file. |
| Kept. | Festgehalten. | The natural past participle; short and calm. |
| ask (verb, button) | fragen; button „Fragen“ | Plain. |
| remember | sich merken | Everyday word; keeps festhalten for the person's action. |
| the morning page | die Morgenseite | One compound, works as a name. |
| what needs you | was ansteht | Idiomatic for what waits on you; the app label would be „Steht an“. |
| memories (things kept) | Erinnerungen | Kept apart from Arbeitsspeicher. |
| memory (of a computer) | Arbeitsspeicher | As Windows and macOS say it; never Erinnerung. |
| space | Speicherplatz | As the systems say it. |
| receipts; the original | Beleg; das Original | Beleg is the everyday word for proof of where something came from. |
| private; marked private | privat; als privat markiert | Matches a future „Als privat markieren“. |
| fade; let go; bring back | verblassen; beiseitelegen; zurückholen | Beiseitelegen says it can be picked up again; aussortieren or löschen would not. |
| forget; forget for good | vergessen; endgültig vergessen | Endgültig marks the one step that cannot be undone. |
| tidy; the daily tidy | aufräumen; das tägliche Aufräumen | Household word, no technical tone. |
| Away from home | Unterwegs | Short, everyday, fits a switch. |
| Online answers | Online-Antworten | Literal; online is plain German. |
| setup (first run) | Einrichtung | What German systems call setting up a new device. |
| setup (size): Large, Standard, Small, Tiny | Variante: Groß, Standard, Klein, Mini | Variante avoids a clash with Einrichtung. Mini is warmer than winzig. |
| voice note | Sprachnotiz | Common on phones. |
| its mind | sein Gehirn | Same plain metaphor, no jargon. |
| AI chip | KI-Chip | Only in Requirements, as in English. |
| demo data | Beispieldaten | Demodaten reads technical. |
| journal | Tagebuch | The everyday word for writing your day; Journal reads as a magazine. |
| entry | Eintrag | Plain. |
| check in (verb); a check-in | notieren, wie es dir geht; Gefühlsnotiz | Einchecken would read as a hotel or an airport, and festhalten is kept for keep. Gefühlsnotiz sits beside Sprachnotiz. |
| Wound up, Lit up, Low, Settled (the four parts) | Angespannt, Beschwingt, Gedrückt, Ausgeglichen | With the English in brackets while the app is English. |
| good thing | Schönes; Schöne Dinge | Warmer than gute Dinge. |
| reflection | Reflexion | Kept apart from the Morgenseite and from Rückblick. |
| recap | Rückblick | As in Jahresrückblick, for a week, a month or a year. |
| streak | Serie | As streak counters in German apps say it. |

## Landing page, head and link preview

| Id | English | Deutsch |
|---|---|---|
| `head.title` | Nocturne · Keep everything, ask about it, get one page each morning | Nocturne · Alles festhalten, danach fragen, jeden Morgen eine Seite |
| `head.description` | Nocturne is a free, private app for your computer. Save notes, photos, voice notes and files without sorting them, now or later. Ask about any of it later, and each morning get one page of what needs you, with the original behind every line. Keep a journal too, if you like. | Nocturne ist eine kostenlose, private App für deinen Computer. Halte Notizen, Fotos, Sprachnotizen und Dateien fest, ohne sie zu sortieren, weder jetzt noch später. Frag später nach allem davon, und jeden Morgen bekommst du eine Seite mit dem, was ansteht, mit dem Original hinter jeder Zeile. Führ auch ein Tagebuch, wenn du magst. |
| `head.og.description` | Save notes, photos, voice notes and files without sorting them, now or later. Ask about any of it later, and each morning get one page of what needs you, all on your own computer. | Halte Notizen, Fotos, Sprachnotizen und Dateien fest, ohne sie zu sortieren, weder jetzt noch später. Frag später nach allem davon, und jeden Morgen bekommst du eine Seite mit dem, was ansteht, alles auf deinem eigenen Computer. |
| `head.og.image.alt` | Nocturne. Keep everything. Nocturne remembers it for you. Ask about any of it later, and get one page each morning of what needs you. Beside the words, the morning page from the app with demo data. | Nocturne. Halte alles fest. Nocturne merkt es sich für dich. Frag später nach allem davon, und jeden Morgen bekommst du eine Seite mit dem, was ansteht. Daneben die Morgenseite aus der App mit Beispieldaten. |

## Header, footer and switches (every page)

| Id | English | Deutsch |
|---|---|---|
| `header.skip` | Skip to content | Zum Inhalt springen |
| `header.brand.aria` | Nocturne, home | Nocturne, Startseite |
| `nav.aria` | Main | Hauptmenü |
| `nav.how` | How it works | So funktioniert es |
| `nav.privacy` | Privacy | Datenschutz |
| `nav.engine` | Engine | Antrieb |
| `nav.requirements` | Requirements | Anforderungen |
| `nav.download` | Download | Herunterladen |
| `nav.docs` | Docs | Doku |
| `theme.aria` | Colour theme | Farbschema |
| `theme.system` | System | System |
| `theme.light` | Light | Hell |
| `theme.dark` | Dark | Dunkel |
| `lang.aria` L | Language | Sprache |
| `docs.inEnglish` L | in English (read by screen readers after a docs link; the visible mark is EN) | auf Englisch |
| `suggest.text` L | This page is also in German. (Shown on the English page, in the suggested language.) | Diese Seite gibt es auch auf Deutsch. |
| `suggest.link` L | Read in German | Auf Deutsch lesen |
| `suggest.close` L | Close this note | Hinweis schließen |
| `foot.brand` | Nocturne · engine by Lemonade · Apache 2.0 | Nocturne · Antrieb von Lemonade · Apache 2.0 |
| `foot.aria` | Footer | Fußzeile |
| `foot.docs` | Docs | Doku |
| `foot.data` | Your data | Deine Daten |
| `foot.download` | Download | Herunterladen |
| `foot.install` | Install guide | Installationsanleitung |
| `foot.support` | Support | Hilfe |
| `foot.font` | Font licence | Schriftlizenz |

## Hero

| Id | English | Deutsch |
|---|---|---|
| `hero.title` | Keep everything. *Nocturne remembers it for you.* | Halte alles fest. *Nocturne merkt es sich für dich.* |
| `hero.lede` | Save notes, photos, voice notes and files without sorting them, now or later. Ask about any of it later, and each morning get one page of what needs you, all on your own computer. | Halte Notizen, Fotos, Sprachnotizen und Dateien fest, ohne sie zu sortieren, weder jetzt noch später. Frag später nach allem davon, und jeden Morgen bekommst du eine Seite mit dem, was ansteht, alles auf deinem eigenen Computer. |
| `hero.download` | Download free | Kostenlos herunterladen |
| `hero.how` | See how it works | So funktioniert es |
| `hero.facts.aria` | At a glance | Auf einen Blick |
| `hero.fact.platforms` | Mac, Windows, Linux | Mac, Windows, Linux |
| `hero.fact.offline` | Offline after setup | Nach der Einrichtung offline |
| `hero.fact.free` | Free and open source | Kostenlos und Open Source |
| `hero.page.alt` | The morning page in Nocturne: Friday 13 November, two things need you today. The Year 4 trip form is due next Friday, with the letter it came from. The dentist appointment changed, with a choice between the card and the voice note. | Die Morgenseite in Nocturne: Freitag, 13. November, heute stehen zwei Dinge an. Das Formular für den Klassenausflug muss bis nächsten Freitag abgegeben werden, mit dem Brief, aus dem es stammt. Der Zahnarzttermin hat sich geändert, mit der Wahl zwischen der Terminkarte und der Sprachnotiz. |
| `hero.keep.alt` | The keep box with “Gran’s new number ends 4471” typed in, and Keep and Ask buttons. | Das Feld zum Festhalten, darin getippt: Omas neue Nummer endet auf 4471, dazu die Knöpfe Festhalten und Fragen. |
| `note.english` L | The app is in English for now. | Die App ist vorerst auf Englisch. |
| `note.understanding` L | How well it understands notes and voice in this language depends on the mind it downloads, not on Nocturne itself. | Wie gut es Notizen und Sprache auf Deutsch versteht, hängt vom Gehirn ab, das es herunterlädt, nicht von Nocturne selbst. |
| `hero.caption` | Keep it in one box, read it on the morning page. From the app, with demo data. | In einem Feld festhalten, auf der Morgenseite lesen. Aus der App, mit Beispieldaten. |

## How it works

| Id | English | Deutsch |
|---|---|---|
| `how.eyebrow` | How it works | So funktioniert es |
| `how.title` | Keep it now. Read it once. | Jetzt festhalten. Einmal lesen. |
| `keep.eyebrow` | Keep · any time | Festhalten · jederzeit |
| `keep.title` | Keep anything, file nothing. | Alles festhalten, nichts ablegen. |
| `keep.body` | Press `Ctrl Shift Space`, or `Cmd Shift Space` on a Mac, then type, talk, paste or drop a photo or a PDF. Nocturne says “Kept.” and you get back to your day. No folders or tags to keep up: you ask, and it finds it. If it cannot read something, it tells you what to do. | Drück `Strg Umschalt Leertaste`, auf dem Mac `Cmd Umschalt Leertaste`, dann tipp, sprich, füg etwas ein oder zieh ein Foto oder ein PDF hinein. Nocturne sagt „Festgehalten.“ (Kept.), und du machst mit deinem Tag weiter. Keine Ordner oder Schlagwörter zu pflegen: Du fragst, und Nocturne findet es. Wenn es etwas nicht lesen kann, sagt es dir, was zu tun ist. |
| `keep.box.alt` | The quick keep box with “Gran’s new number ends 4471” typed in, and Keep and Ask buttons. | Das Schnellfeld zum Festhalten, darin getippt: Omas neue Nummer endet auf 4471, dazu die Knöpfe Festhalten und Fragen. |
| `keep.recent.alt` | Recently kept: a photo still being read, voice notes and pasted messages marked understood, a password-locked PDF Nocturne could not read with what to do about it, and a voice note that needs you. | Zuletzt festgehalten: ein Foto, das noch gelesen wird, Sprachnotizen und eingefügte Nachrichten, als verstanden markiert, ein passwortgeschütztes PDF, das Nocturne nicht lesen konnte, mit dem, was zu tun ist, und eine Sprachnotiz, bei der etwas ansteht. |
| `ask.eyebrow` | Ask · any time | Fragen · jederzeit |
| `ask.title` | Answers with receipts. | Antworten mit Beleg. |
| `ask.body` | Ask in plain words. Nocturne answers only from what it can show you, with the originals underneath so you can check. When it does not know, it says so. If it gets something wrong, tell it what is right and it remembers. | Frag in einfachen Worten. Nocturne antwortet nur mit dem, was es dir zeigen kann, mit den Originalen darunter, damit du nachsehen kannst. Wenn es etwas nicht weiß, sagt es das. Liegt es falsch, sag ihm, was stimmt, und es merkt sich das. |
| `ask.alt` | An answer to “What does Leo need for his trip?”: a packed lunch, and the trip form is due next Friday with a signature and £12. Below it, the two photographed letters it came from. | Eine Antwort auf die Frage, was Leo für seinen Ausflug braucht: ein Lunchpaket, und das Formular muss bis nächsten Freitag abgegeben werden, mit Unterschrift und 12 Pfund. Darunter die zwei fotografierten Briefe, aus denen die Antwort stammt. |
| `page.eyebrow` | The morning page · at your time | Die Morgenseite · zu deiner Uhrzeit |
| `page.title` | Wake to what needs you. | Morgens sehen, was ansteht. |
| `page.body` | Whenever the computer is plugged in and idle, by night or by day, Nocturne reads what you kept and notices what changed. At the time you choose, you get a short list to read, hear or print. When two things disagree, it asks instead of guessing. | Immer wenn der Computer am Strom hängt und nichts zu tun hat, bei Nacht oder bei Tag, liest Nocturne, was du festgehalten hast, und bemerkt, was sich geändert hat. Zu der Uhrzeit, die du wählst, bekommst du eine kurze Liste zum Lesen, Anhören oder Ausdrucken. Wenn sich zwei Dinge widersprechen, fragt es nach, statt zu raten. |
| `page.alt` | The morning page: two things need you today. The Year 4 trip form is due next Friday, with the letter it came from. The dentist appointment changed, with a choice between the appointment card and the voice note. | Die Morgenseite: Heute stehen zwei Dinge an. Das Formular für den Klassenausflug muss bis nächsten Freitag abgegeben werden, mit dem Brief, aus dem es stammt. Der Zahnarzttermin hat sich geändert, mit der Wahl zwischen der Terminkarte und der Sprachnotiz. |

## The journal

| Id | English | Deutsch |
|---|---|---|
| `journal.eyebrow` | The journal · when you want to | Das Tagebuch · wann du willst |
| `journal.title` | Your day, in your own words. | Dein Tag, in deinen eigenen Worten. |
| `journal.body` | Everything you keep during the day is already in your journal, so it starts half written. Add a few lines, a voice note or a photo, and say how you felt. Nocturne looks back on each day, and on each week, month and year in a recap. Every line points to what it came from. | Alles, was du im Lauf des Tages festhältst, steht schon in deinem Tagebuch, darum beginnt es halb geschrieben. Füg ein paar Zeilen, eine Sprachnotiz oder ein Foto hinzu und sag, wie du dich gefühlt hast. Nocturne blickt auf jeden Tag zurück, und in einem Rückblick auf jede Woche, jeden Monat und jedes Jahr. Jede Zeile verweist auf das, woraus sie stammt. |
| `journal.felt.term` | How you felt, never guessed | Wie du dich gefühlt hast, nie geraten |
| `journal.felt.detail` | Check in as Wound up, Lit up, Low or Settled, and choose the words that fit. A reflection quotes what you said you felt, and never scores the day or gives advice. | Notier, wie es dir geht: Angespannt (Wound up), Beschwingt (Lit up), Gedrückt (Low) oder Ausgeglichen (Settled), und wähl die Worte, die passen. Eine Reflexion zitiert, was du über dein Gefühl gesagt hast, bewertet nie den Tag und gibt keine Ratschläge. |
| `journal.good.term` | Good things | Schöne Dinge |
| `journal.good.detail` | Up to three a day, with a photo if you like. Your recaps bring them back. | Bis zu drei am Tag, mit einem Foto, wenn du magst. Deine Rückblicke holen sie wieder hervor. |
| `journal.calm.term` | No streaks, no nagging | Keine Serien, kein Drängeln |
| `journal.calm.detail` | Nothing counts the days you write or says you missed one. A reminder comes only if you turn one on. When a reflection or recap is new, the morning page says so in one line. | Nichts zählt die Tage, an denen du schreibst, oder sagt dir, dass du einen verpasst hast. Ans Schreiben erinnert es dich nur, wenn du das einschaltest. Wenn eine Reflexion oder ein Rückblick neu ist, sagt die Morgenseite das in einer Zeile. |
| `journal.quiet.term` | It stays quiet | Es bleibt leise |
| `journal.quiet.detail` | Nothing from your journal is read aloud or shown in a notification or on a lock screen. If you turn on Online answers, a line from your journal or a check-in can go with a question it helps answer. Your reflections and recaps never do. | Nichts aus deinem Tagebuch wird vorgelesen oder in einer Benachrichtigung oder auf dem Sperrbildschirm gezeigt. Wenn du Online-Antworten (Online answers) einschaltest, kann eine Zeile aus deinem Tagebuch oder eine Gefühlsnotiz mit einer Frage mitgehen, wenn sie hilft, sie zu beantworten. Deine Reflexionen und Rückblicke gehen nie mit. |
| `journal.day.alt` | Today in the journal, Thursday 12 November: an entry about lunch with Priya, with a voice note and a photo. Beside it, how today felt, with three check-ins (tired, pleased, relieved and content). | Heute im Tagebuch, Donnerstag, 12. November: ein Eintrag über das Mittagessen mit Priya, mit einer Sprachnotiz und einem Foto. Daneben, wie sich der Tag angefühlt hat, mit drei Gefühlsnotizen (müde, froh, erleichtert und zufrieden). |
| `journal.look.alt` | The day, looked back on: a short paragraph about Leo forgetting his PE kit, a tired morning and lunch with Priya at last, with the notes, check-ins, document and entry it came from listed underneath. | Der Blick zurück auf den Tag: ein kurzer Absatz darüber, dass Leo sein Sportzeug vergessen hat, über einen müden Morgen und endlich das Mittagessen mit Priya, darunter die Notizen, Gefühlsnotizen, das Dokument und der Eintrag, aus denen er stammt. |
| `journal.caption` | Today’s journal, and the day looked back on. From the app, with demo data. | Das heutige Tagebuch und der Blick zurück auf den Tag. Aus der App, mit Beispieldaten. |

## What it promises

| Id | English | Deutsch |
|---|---|---|
| `privacy.eyebrow` | What it promises | Was es verspricht |
| `privacy.title` | Your life stays on your computer. | Dein Leben bleibt auf deinem Computer. |
| `privacy.body` | Out of the box, Nocturne goes online only for downloads you agree to: its mind at setup and any update you allow. Privacy in the app lists each one with the day and the size. | Ab Werk geht Nocturne nur für Downloads online, denen du zustimmst: sein Gehirn bei der Einrichtung und jedes Update, das du erlaubst. Unter Datenschutz (Privacy) in der App steht jeder einzelne mit Tag und Größe. |
| `promise.signin.term` | Nothing to sign in to | Keine Anmeldung |
| `promise.signin.detail` | No account, no analytics, no crash reports, no cloud copy. To keep a copy somewhere safe, back everything up to one file, locked with a passphrase you choose. | Kein Konto, keine Analyse, keine Absturzberichte, keine Kopie in der Cloud. Willst du eine Kopie an einem sicheren Ort, sichere alles in einer einzigen Datei, geschützt mit einem Passwort, das du wählst. |
| `promise.checked.term` | Checked, not just claimed | Geprüft, nicht nur behauptet |
| `promise.checked.detail` | Before a release, Nocturne is run with its network cut off from everything but this computer, and any attempt to reach further fails the check. | Vor einer Veröffentlichung läuft Nocturne mit einem Netz, das zu allem außer diesem Computer gekappt ist, und jeder Versuch, weiter zu kommen, lässt die Prüfung scheitern. |
| `promise.edit.term` | It never edits what you kept | Es ändert nie, was du festgehalten hast |
| `promise.edit.detail` | Your notes, photos and files are never changed. Every answer shows them underneath. If one is wrong, tell it what is right; the old version is kept, not overwritten. | Deine Notizen, Fotos und Dateien werden nie verändert. Jede Antwort zeigt sie darunter. Ist eine falsch, sag ihm, was stimmt; die alte Fassung bleibt erhalten und wird nicht überschrieben. |
| `promise.forget.term` | It forgets on purpose | Es vergisst mit Absicht |
| `promise.forget.detail` | Things you no longer use fade, and the page says when one is let go. Bring any back; forgetting for good always asks first. | Was du nicht mehr nutzt, verblasst, und die Seite sagt dir, wenn etwas beiseitegelegt wird. Alles lässt sich zurückholen; bevor es etwas endgültig vergisst, fragt es immer. |
| `promise.note.term` | One quiet note a day | Ein ruhiger Hinweis am Tag |
| `promise.note.detail` | When the page is ready, one note says so and how many things need you. Never what you kept. | Wenn die Seite fertig ist, sagt ein Hinweis Bescheid und wie viele Dinge anstehen. Nie, was du festgehalten hast. |
| `promise.extras.term` | Two extras, off until you ask | Zwei Extras, aus, bis du sie willst |
| `promise.extras.detail` | Away from home lets your own phone ask, keep and read your page and journal, sealed so the company in between cannot read it. Online answers sends a question, and the few remembered sentences it needs, to one service you choose. They can include lines from your journal, your check-ins and things you marked private, but never your reflections or recaps. Marking something private keeps it out of notifications, reminders and the lock screen, but it still helps answer what you ask. | Mit Unterwegs (Away from home) kann dein eigenes Handy fragen, festhalten und deine Seite und dein Tagebuch lesen, versiegelt, sodass die Firma dazwischen nichts davon lesen kann. Online-Antworten (Online answers) schickt eine Frage und die wenigen gemerkten Sätze, die sie braucht, an einen Dienst deiner Wahl. Darunter können Zeilen aus deinem Tagebuch, deine Gefühlsnotizen und als privat markierte Dinge sein, aber nie deine Reflexionen oder Rückblicke. Etwas als privat zu markieren hält es aus Benachrichtigungen, Erinnerungshinweisen und vom Sperrbildschirm fern, aber es hilft trotzdem, zu beantworten, was du fragst. |
| `phone.alt` | Nocturne on a paired phone: connected to the home computer, with Ask, Keep, today’s page and the journal, and a note that things marked private stay on the computer. | Nocturne auf einem gekoppelten Handy: verbunden mit dem Computer zu Hause, mit Fragen, Festhalten, der heutigen Seite und dem Tagebuch, und einem Hinweis, dass als privat markierte Dinge auf dem Computer bleiben. |
| `phone.caption` | Away from home on a paired phone, with demo data. | Unterwegs auf einem gekoppelten Handy, mit Beispieldaten. |

## The engine

| Id | English | Deutsch |
|---|---|---|
| `engine.eyebrow` | The engine | Der Antrieb |
| `engine.title` | Built on (followed by the Lemonade logo, alt text “Lemonade”) | Gebaut auf (Logo-Alternativtext „Lemonade“) |
| `engine.body1` | Nocturne’s thinking runs on **Embeddable Lemonade**, the portable build of [Lemonade], the open-source local AI server. Nocturne ships its own copy, Lemonade {{LEMONADE_VERSION}}, that only this computer can reach. | Nocturnes Denken läuft auf **Embeddable Lemonade**, der portablen Ausgabe von [Lemonade], dem quelloffenen lokalen KI-Server. Nocturne bringt eine eigene Kopie mit, Lemonade {{LEMONADE_VERSION}}, die nur dieser Computer erreichen kann. |
| `engine.body2` | Lemonade is free under the Apache 2.0 licence and built by its community, with optimisations by AMD engineers. Nocturne would not exist without it. | Lemonade ist kostenlos unter der Apache-2.0-Lizenz und wird von seiner Community entwickelt, mit Optimierungen von Fachleuten bei AMD. Ohne Lemonade gäbe es Nocturne nicht. |

## Requirements (the install page repeats these)

| Id | English | Deutsch |
|---|---|---|
| `req.eyebrow` | Requirements | Anforderungen |
| `req.title` | Will it run on my computer? | Läuft es auf meinem Computer? |
| `req.body` | Almost certainly, if it has 8 GB of memory: an 8 GB MacBook Air, or a typical 8 GB Windows or Linux laptop. | Fast sicher, wenn er 8 GB Arbeitsspeicher hat: ein MacBook Air mit 8 GB oder ein typischer Windows- oder Linux-Laptop mit 8 GB. |
| `req.memory.term` | Memory | Arbeitsspeicher |
| `req.memory.detail` | 8 GB is enough. On an 8 GB computer Nocturne uses about 5 GB at its busiest and gives it back when it has nothing to do. With more memory it offers a larger setup with clearer answers. | 8 GB reichen. Auf einem Computer mit 8 GB braucht Nocturne in der geschäftigsten Phase etwa 5 GB und gibt sie wieder frei, wenn es nichts zu tun hat. Mit mehr Arbeitsspeicher bietet es eine größere Variante mit klareren Antworten. |
| `req.graphics.term` | A graphics card | Eine Grafikkarte |
| `req.graphics.detail` | No graphics card needed. Macs with Apple silicon, and computers with AMD or Intel graphics built in, run Nocturne on the chip and share the computer’s memory. An AMD or NVIDIA graphics card works too, best with 6 GB or more. Other computers run it on the processor. | Keine Grafikkarte nötig. Macs mit Apple Chip und Computer mit eingebauter AMD- oder Intel-Grafik lassen Nocturne auf dem Chip laufen und teilen sich den Arbeitsspeicher des Computers. Eine Grafikkarte von AMD oder NVIDIA geht auch, am besten mit 6 GB oder mehr. Andere Computer lassen es auf dem Prozessor laufen. |
| `req.chip.term` | An AI chip | Ein KI-Chip |
| `req.chip.detail` | Helpful, never needed. On AMD Ryzen AI 300 series computers and newer, with 16 GB or more, the AI chip reads your voice notes. On many with 32 GB or more it also reads what you keep, and on some it answers your questions too, so the rest of the computer has less to do. | Hilfreich, nie nötig. Auf Computern der Serie AMD Ryzen AI 300 und neuer, mit 16 GB oder mehr, liest der KI-Chip deine Sprachnotizen. Auf vielen mit 32 GB oder mehr liest er auch, was du festgehalten hast, und auf manchen beantwortet er auch deine Fragen, sodass der Rest des Computers weniger zu tun hat. |
| `req.system.term` | The system | Das System |
| `req.system.detail` | macOS 26 or later on an Apple chip, Windows 10 or 11 (64-bit), or 64-bit Linux on Intel, AMD or Arm. | macOS 26 oder neuer auf einem Apple Chip, Windows 10 oder 11 (64 Bit) oder 64-Bit-Linux auf Intel, AMD oder Arm. |
| `tier.large.name` | Large | Groß |
| `tier.large.spec` | 60 GB+ memory · 28 GB space | ab 60 GB Arbeitsspeicher · 28 GB Speicherplatz |
| `tier.large.text` | Clearer answers to harder questions. | Klarere Antworten auf schwierigere Fragen. |
| `tier.standard.name` | Standard | Standard |
| `tier.standard.spec` | 28 GB+ memory · 11 GB space | ab 28 GB Arbeitsspeicher · 11 GB Speicherplatz |
| `tier.standard.text` | Many recent laptops with 32 GB. | Viele neuere Laptops mit 32 GB. |
| `tier.small.name` | Small | Klein |
| `tier.small.spec` | 14 GB+ memory · 7 GB space | ab 14 GB Arbeitsspeicher · 7 GB Speicherplatz |
| `tier.small.text` | Same page. The daily tidy takes longer. | Dieselbe Seite. Das tägliche Aufräumen dauert länger. |
| `tier.tiny.name` | Tiny | Mini |
| `tier.tiny.spec` | 6 GB+ memory · 5.2 GB space | ab 6 GB Arbeitsspeicher · 5,2 GB Speicherplatz |
| `tier.tiny.text` | Made for 8 GB computers. Simpler answers that may miss details. | Für Computer mit 8 GB gemacht. Einfachere Antworten, denen Details entgehen können. |
| `req.downloads` | Downloads at setup are between about 3 GB and 26 GB, depending on the setup, and about 9 GB more where the AI chip reads what you keep. After that, Nocturne works with no internet at all. | Die Downloads bei der Einrichtung sind je nach Variante etwa 3 GB bis 26 GB groß, dazu etwa 9 GB mehr dort, wo der KI-Chip liest, was du festgehalten hast. Danach arbeitet Nocturne ganz ohne Internet. |

## Get Nocturne (end of the landing page)

| Id | English | Deutsch |
|---|---|---|
| `get.title` | Get Nocturne. | Hol dir Nocturne. |
| `get.body` | Free and open source, for Mac, Windows and Linux. Download it, open it, and it sets itself up for the computer it is on. | Kostenlos und Open Source, für Mac, Windows und Linux. Herunterladen, öffnen, und es richtet sich selbst für den Computer ein, auf dem es läuft. |
| `get.download` | Download | Herunterladen |
| `get.source` | Build it from source | Aus dem Quellcode bauen |

## Not-found page

| Id | English | Deutsch |
|---|---|---|
| `missing.title` | Not here · Nocturne | Nicht hier · Nocturne |
| `missing.eyebrow` | Not found | Nicht gefunden |
| `missing.heading` | There is no page here. | Hier gibt es keine Seite. |
| `missing.body` | The link may be old, or the page may have moved. | Der Link ist vielleicht alt, oder die Seite ist umgezogen. |
| `missing.links` | [Go to the front page] or [read the docs]. | [Zur Startseite] oder [die Doku lesen]. |

## Install page, head and top

| Id | English | Deutsch |
|---|---|---|
| `install.title` | Get Nocturne · Download for Mac, Windows and Linux | Hol dir Nocturne · Download für Mac, Windows und Linux |
| `install.description` | Download Nocturne for a Mac with an Apple chip, a Windows 10 or 11 computer, or a Linux desktop. Free and open source, with no account. The first time it opens, it sets itself up for the computer it is on. | Lade Nocturne für einen Mac mit Apple Chip, einen Computer mit Windows 10 oder 11 oder einen Linux-Desktop herunter. Kostenlos und Open Source, ohne Konto. Beim ersten Öffnen richtet es sich selbst für den Computer ein, auf dem es läuft. |
| `install.og.description` | Free and open source, with no account. The first time it opens, Nocturne sets itself up for the computer it is on. | Kostenlos und Open Source, ohne Konto. Beim ersten Öffnen richtet sich Nocturne selbst für den Computer ein, auf dem es läuft. |
| `install.heading` | Get Nocturne. | Hol dir Nocturne. |
| `install.lede` | Free and open source, for Mac, Windows and Linux. No account, nothing to sign in to. | Kostenlos und Open Source, für Mac, Windows und Linux. Kein Konto, keine Anmeldung. |
| `install.here` | Choose your computer below. Each link gives the latest version. | Wähl unten deinen Computer. Jeder Link führt zur neuesten Version. |
| `install.hereTag` | For this computer | Für diesen Computer |
| `install.firstOpen` | The first time you open it | Beim ersten Öffnen |
| `install.others` | Other computers | Andere Computer |

## Install page, Mac

| Id | English | Deutsch |
|---|---|---|
| `mac.spec` | Apple chip, M1 or later · macOS 26 or later | Apple Chip, M1 oder neuer · macOS 26 oder neuer |
| `mac.download` | Download for Mac | Für Mac herunterladen |
| `mac.note` ◊ | Macs with an Intel chip are not supported. To see which chip yours has, open the Apple menu and choose About This Mac. | Macs mit Intel-Chip werden nicht unterstützt. Welchen Chip deiner hat, siehst du im Apple-Menü unter „Über diesen Mac“. |
| `mac.step1` ◊ | Open the downloaded file, then drag Nocturne into Applications. | Öffne die heruntergeladene Datei und zieh Nocturne in den Ordner „Programme“. |
| `mac.step2` ◊ | Open Nocturne from Applications. Your Mac says “Nocturne” Not Opened, because it could not check the app. Click **Done**. | Öffne Nocturne aus „Programme“. Dein Mac meldet: „Nocturne“ wurde nicht geöffnet, weil er die App nicht prüfen konnte. Klick auf **Fertig**. |
| `mac.step3` ◊ | Open System Settings, then **Privacy & Security**. Scroll down to the line saying Nocturne was blocked and click **Open Anyway**. | Öffne die Systemeinstellungen, dann **Datenschutz & Sicherheit**. Scroll nach unten zu der Zeile, dass Nocturne blockiert wurde, und klick auf **Dennoch öffnen**. |
| `mac.step4` ◊ | Enter your password, then click **Open Anyway** again. | Gib dein Passwort ein und klick noch einmal auf **Dennoch öffnen**. |
| `mac.why` | You only do this once. Apple charges a yearly fee to sign an app so it opens without asking; Nocturne is free and open source, and this release is not signed yet. | Das machst du nur einmal. Apple verlangt eine jährliche Gebühr, um eine App zu signieren, damit sie ohne Nachfrage startet; Nocturne ist kostenlos und Open Source, und diese Version ist noch nicht signiert. |

## Install page, Windows

| Id | English | Deutsch |
|---|---|---|
| `win.spec` | Windows 10 or 11, 64-bit | Windows 10 oder 11, 64 Bit |
| `win.download` | Download for Windows | Für Windows herunterladen |
| `win.note` | Installs for you alone and needs no administrator password. Computers managed by an organisation can use the [.msi installer] instead. | Installiert sich nur für dich und braucht kein Administratorkennwort. Computer, die eine Organisation verwaltet, können stattdessen das [.msi-Installationsprogramm] nutzen. |
| `win.step1` ◊ | Your browser may warn that the file is not commonly downloaded. In Edge, open the **…** menu beside the file, choose **Keep**, then **Show more**, then **Keep anyway**. | Dein Browser warnt vielleicht, dass die Datei nicht häufig heruntergeladen wird. Öffne in Edge das Menü **…** neben der Datei, wähl **Beibehalten**, dann **Mehr anzeigen**, dann **Trotzdem beibehalten**. |
| `win.step2` ◊ | Open the installer. Windows shows “Windows protected your PC”. Click **More info**, then **Run anyway**. | Öffne das Installationsprogramm. Windows zeigt „Der Computer wurde durch Windows geschützt“. Klick auf **Weitere Informationen**, dann auf **Trotzdem ausführen**. |
| `win.why` | You only do this once. Microsoft charges a yearly fee to sign an app so it opens without asking; Nocturne is free and open source, and this release is not signed yet. | Das machst du nur einmal. Microsoft verlangt eine jährliche Gebühr, um eine App zu signieren, damit sie ohne Nachfrage startet; Nocturne ist kostenlos und Open Source, und diese Version ist noch nicht signiert. |

## Install page, Linux

| Id | English | Deutsch |
|---|---|---|
| `linux.spec` | 64-bit computer with an Intel, AMD or Arm chip | 64-Bit-Computer mit Intel-, AMD- oder Arm-Chip |
| `linux.choices.aria` | Ways to install on Linux | Wege, es unter Linux zu installieren |
| `linux.flatpak.title` | From your software centre | Aus deinem Software-Center |
| `linux.flatpak.text` | The choice for most people, on any Linux desktop. Your software centre keeps Nocturne up to date. | Für die meisten die richtige Wahl, auf jedem Linux-Desktop. Dein Software-Center hält Nocturne aktuell. |
| `linux.flatpak.button` | Get the Flatpak | Flatpak holen |
| `linux.flatpak.terminal` | Or install it from a terminal | Oder im Terminal installieren |
| `linux.deb.title` | Ubuntu, Linux Mint or Debian | Ubuntu, Linux Mint oder Debian |
| `linux.package.text` | Open the file with your software centre, and Nocturne appears with your other apps. (Shared by .deb and .rpm.) | Öffne die Datei mit deinem Software-Center, und Nocturne erscheint bei deinen anderen Apps. |
| `linux.deb.intel` | .deb for Intel or AMD | .deb für Intel oder AMD |
| `linux.deb.arm` | .deb for Arm | .deb für Arm |
| `linux.rpm.title` | Fedora, Red Hat or openSUSE | Fedora, Red Hat oder openSUSE |
| `linux.rpm.intel` | .rpm for Intel or AMD | .rpm für Intel oder AMD |
| `linux.rpm.arm` | .rpm for Arm | .rpm für Arm |
| `linux.appimage.title` | Try it without installing | Ohne Installation ausprobieren |
| `linux.appimage.text` | One file that runs on most desktops. In its properties, allow it to run as a program, then open it. | Eine Datei, die auf den meisten Desktops läuft. Erlaube in ihren Eigenschaften, sie als Programm auszuführen, und öffne sie dann. |
| `linux.appimage.intel` | AppImage for Intel or AMD | AppImage für Intel oder AMD |
| `linux.appimage.arm` | AppImage for Arm | AppImage für Arm |
| `linux.note` | Most Linux computers have an Intel or AMD chip. To check, run `uname -m` in a terminal: `x86_64` means Intel or AMD, `aarch64` means Arm. The Flatpak picks the right one by itself. | Die meisten Linux-Computer haben einen Intel- oder AMD-Chip. Zum Nachsehen führ `uname -m` im Terminal aus: `x86_64` heißt Intel oder AMD, `aarch64` heißt Arm. Das Flatpak wählt selbst das richtige. |
| `linux.why` | Linux shows no warning the first time you open Nocturne. | Linux zeigt beim ersten Öffnen von Nocturne keine Warnung. |

## Install page, checks, needs and after

| Id | English | Deutsch |
|---|---|---|
| `install.checks` | To make sure a download is the one we published, compare it with the fingerprints in [SHA256SUMS]. The [install guide] shows how, explains each package and how to take Nocturne off again. Earlier versions are on the [releases page]. | Um sicherzugehen, dass ein Download der von uns veröffentlichte ist, vergleich ihn mit den Fingerabdrücken in [SHA256SUMS]. Die [Installationsanleitung] zeigt, wie das geht, erklärt jedes Paket und wie du Nocturne wieder entfernst. Ältere Versionen findest du auf der [Release-Seite]. |
| `needs.title` | What your computer needs. (The rest of this section is the Requirements rows above.) | Was dein Computer braucht. |
| `after.title` | After you install. | Nach der Installation. |
| `after.body` | Nocturne opens a short setup. You can keep things straight away. | Nocturne öffnet eine kurze Einrichtung. Du kannst sofort Dinge festhalten. |
| `after.1.when` | When it opens | Beim Öffnen |
| `after.1.title` | It looks at the computer | Es sieht sich den Computer an |
| `after.1.text` | It tells you which setup it will use. You can always pick a smaller one, which uses less memory, space and battery. | Es sagt dir, welche Variante es nehmen wird. Du kannst jederzeit eine kleinere wählen, die weniger Arbeitsspeicher, Speicherplatz und Akku braucht. |
| `after.2.when` | Once, when you agree | Einmal, wenn du zustimmst |
| `after.2.title` | It downloads what it needs | Es lädt herunter, was es braucht |
| `after.2.text` | It asks first, then downloads what this computer can run: between about 3 GB and 26 GB, and about 9 GB more where the AI chip reads what you keep. This is the only time Nocturne needs the internet, and it carries on by itself if the connection drops. | Es fragt zuerst und lädt dann herunter, was dieser Computer ausführen kann: etwa 3 GB bis 26 GB, dazu etwa 9 GB mehr dort, wo der KI-Chip liest, was du festgehalten hast. Nur dieses eine Mal braucht Nocturne das Internet, und wenn die Verbindung abbricht, macht es von selbst weiter. |
| `after.3.when` | At the time you choose | Zu der Uhrzeit, die du wählst |
| `after.3.title` | Your first page | Deine erste Seite |
| `after.3.text` | From then on, Nocturne works with no internet at all. | Ab dann arbeitet Nocturne ganz ohne Internet. |

## Install page, messages from install.js

| Id | English | Deutsch |
|---|---|---|
| `js.mac` | This looks like a Mac, so its download comes first. Nocturne needs a Mac with an Apple chip. | Das sieht nach einem Mac aus, darum steht sein Download oben. Nocturne braucht einen Mac mit Apple Chip. |
| `js.macArm` | This looks like a Mac with an Apple chip, so its download comes first. | Das sieht nach einem Mac mit Apple Chip aus, darum steht sein Download oben. |
| `js.macIntel` ◊ | This looks like a Mac with an Intel chip. Nocturne needs a Mac with an Apple chip (M1 or later), so it will not run on this one. About This Mac, in the Apple menu, shows which chip it has. | Das sieht nach einem Mac mit Intel-Chip aus. Nocturne braucht einen Mac mit Apple Chip (M1 oder neuer) und läuft auf diesem daher nicht. Welchen Chip er hat, zeigt „Über diesen Mac“ im Apple-Menü. |
| `js.windows` | This looks like a Windows computer, so its download comes first. | Das sieht nach einem Windows-Computer aus, darum steht sein Download oben. |
| `js.linux` | This looks like a Linux computer, so its choices come first. | Das sieht nach einem Linux-Computer aus, darum stehen seine Möglichkeiten oben. |
| `js.linuxArm` | This looks like a Linux computer with an Arm chip, so its choices come first, Arm downloads first. | Das sieht nach einem Linux-Computer mit Arm-Chip aus, darum stehen seine Möglichkeiten oben, die Arm-Downloads zuerst. |
| `js.other` | Nocturne is a desktop app, for Mac, Windows and Linux computers. Open this page on the computer you want to use it on, or choose one below. | Nocturne ist eine Desktop-App für Mac-, Windows- und Linux-Computer. Öffne diese Seite auf dem Computer, auf dem du es nutzen willst, oder wähl unten einen aus. |

## Support page

Drafted with DeepL on 2026-11-24 (with the glossary's form of address) and reviewed against the glossary and the neighbouring strings the same day. The `req.chip.detail`, `req.downloads`, `linux.flatpak.text`, `after.2.text`, `after.3.when` and `foot.support` rows below and above went through the same pass. Words the app shows are bold, with the English in brackets while the app is English.

| Id | English | Deutsch |
|---|---|---|
| `support.title` | Support · Nocturne | Hilfe · Nocturne |
| `support.description` | Help with Nocturne on a computer or a phone: answers from its own guide, what to try first, and how to reach the people who make it. | Hilfe zu Nocturne auf dem Computer oder dem Handy: Antworten aus der eigenen Anleitung, was du zuerst probieren kannst und wie du die Menschen erreichst, die es machen. |
| `support.heading` | Help with Nocturne. | Hilfe bei Nocturne. |
| `support.lede` | Nocturne keeps everything on your own computer or phone, so no help desk can see your memory. Help starts with Nocturne itself and its guide, and the people who make it are a message away. | Bei Nocturne bleibt alles auf deinem eigenen Computer oder Handy, also kann kein Helpdesk deine Erinnerungen sehen. Die Hilfe beginnt bei Nocturne selbst und seiner Anleitung, und die Menschen dahinter sind nur eine Nachricht entfernt. |
| `support.first.title` | Start here. | Fang hier an. |
| `support.ask.title` | Ask Nocturne | Nocturne fragen |
| `support.ask.text` | On a computer, type your question in the ask box, like “how do I add a phone”. Nocturne answers from the guide that comes with it and shows the part it used. | Tipp am Computer deine Frage ins Fragefeld, zum Beispiel „wie füge ich ein Handy hinzu“. Nocturne antwortet aus der Anleitung, die dabei ist, und zeigt die Stelle, die es benutzt hat. |
| `support.phone.title` | On your phone | Auf deinem Handy |
| `support.phone.text` | Open **This phone**, then **Help**: what this phone can do, its permissions with a way to change them, and the privacy policy. | Öffne **Dieses Handy** (This phone), dann **Hilfe** (Help): was dieses Handy kann, seine Berechtigungen mit einer Möglichkeit, sie zu ändern, und die Datenschutzerklärung. |
| `support.guide.title` | Read the guide | Die Anleitung lesen |
| `support.guide.text` | [Installing], [your computer’s desktop], [Away from home], [Online answers] and [your data]. | [Installation], [der Desktop deines Computers], [Unterwegs], [Online-Antworten] und [deine Daten]. |
| `support.qa.title` | Common questions. | Häufige Fragen. |
| `support.qa.page.term` | My page didn’t come | Meine Seite ist nicht gekommen |
| `support.qa.page.detail` | Nocturne writes it in the hours before your page time, while the computer is on, plugged in and not in use. If it was off or asleep, Nocturne catches up when it wakes. [The guide] explains each desktop. | Nocturne schreibt sie in den Stunden vor der Uhrzeit deiner Seite, solange der Computer an ist, am Strom hängt und nicht benutzt wird. War er aus oder im Ruhezustand, holt Nocturne es nach, sobald er aufwacht. [Die Anleitung] erklärt jeden Desktop. |
| `support.qa.lost.term` | I lost my phone | Ich habe mein Handy verloren |
| `support.qa.lost.detail` | If it was paired with your computer, your memory is on the computer: open **Away from home** there, choose **See phones**, then **Remove** beside it. If the phone kept your memory on its own, install Nocturne on the new phone and choose **I have a backup**. | Wenn es mit deinem Computer gekoppelt war, liegen deine Erinnerungen auf dem Computer: Öffne dort **Unterwegs** (Away from home), wähle **Handys ansehen** (See phones), dann daneben **Entfernen** (Remove). Hat das Handy deine Erinnerungen selbst behalten, installiere Nocturne auf dem neuen Handy und wähle **Ich habe eine Sicherung** (I have a backup). |
| `support.qa.passphrase.term` | I forgot my passphrase | Ich habe mein Passwort vergessen |
| `support.qa.passphrase.detail` | No one can open a locked backup without its passphrase or recovery key, not even the people who make Nocturne. Make a new backup with a passphrase you can keep. | Ohne Passwort oder Wiederherstellungsschlüssel kann niemand eine gesperrte Sicherung öffnen, auch die Menschen hinter Nocturne nicht. Mach eine neue Sicherung mit einem Passwort, das du aufbewahren kannst. |
| `support.qa.delete.term` | How do I delete everything? | Wie lösche ich alles? |
| `support.qa.delete.detail` | Choose **Delete everything…** in **Privacy** on a computer, or in **This phone** on a phone that keeps your memory itself. Nocturne’s makers hold nothing of yours, so there is nothing else to ask for. A service you chose for answers keeps what it received under its own rules. | Wähle **Alles löschen…** (Delete everything…) unter **Datenschutz** (Privacy) auf einem Computer, oder unter **Dieses Handy** (This phone) auf einem Handy, das deine Erinnerungen selbst behält. Die Macher von Nocturne haben nichts von dir, also gibt es nichts weiter, worum du bitten müsstest. Ein Dienst, den du für Antworten gewählt hast, behält, was er bekommen hat, nach seinen eigenen Regeln. |
| `support.qa.collect.term` | What does Nocturne collect? | Was sammelt Nocturne? |
| `support.qa.collect.detail` | Nothing. No account, no analytics, no crash reports. Only what you turn on can leave your computer or phone. | Nichts. Kein Konto, keine Analyse, keine Absturzberichte. Nur was du einschaltest, kann deinen Computer oder dein Handy verlassen. |
| `support.contact.title` | Write to us. | Schreib uns. |
| `support.contact.email` | Something not working, or a word that reads wrong? Email [hello@nocturne.is]. A person reads every message. Say which computer or phone you use and what you saw; never send what you kept, your passphrase or your online service key. | Etwas funktioniert nicht, oder ein Wort klingt falsch? Schreib an [hello@nocturne.is]. Jede Nachricht liest ein Mensch. Sag, welchen Computer oder welches Handy du nutzt und was du gesehen hast; schick nie, was du festgehalten hast, dein Passwort oder den Schlüssel deines Online-Dienstes. |
| `support.contact.github` | Or open an issue on [GitHub]. Issues are public, so leave out anything personal. | Oder eröffne ein Issue auf [GitHub]. Issues sind öffentlich, lass also alles Persönliche weg. |
| `support.contact.security` | To report a security problem, use [GitHub’s private reporting], not a public issue. | Ein Sicherheitsproblem meldest du über [GitHubs private Meldung], nicht über ein öffentliches Issue. |

## Decisions

- Register: du, confirmed by the owner.
- „Doku“ for Docs: short enough for the header, and it promises no more than the docs are.
- ◊ rows follow the system's wording and were accepted as written.

## Approval

The owner approved these words on 2026-11-16, with the answers in [the site in five languages](../site-localisation.md#approval).

The journal band, the two reworded Privacy details, the phone picture's description and the meta description were
translated on 2026-11-19 from the English in [the journal on the site](../site-capabilities.md),
with the glossary rows above for the journal.
