---
type: Design
title: Site copy, Nederlands
description: The approved Dutch words for the landing page, the install page and the not-found page, with the glossary they follow.
status: stable
tags: [approved, copy, site, localisation, nl]
---

# Site copy, Nederlands

The Dutch words for the pages described in [the site in five languages](../site-localisation.md).
The strings file `site/i18n/nl.json` carries the same words; change both together.

## Register and conventions

- Register: Je (jij where stressed). Dutch consumer apps, banks and public services mostly use je now; u would make the calm English sound distant.
- Quotes ‘…’, inner “…”. Apostrophe ’ (foto’s, ’s nachts).
- Numbers: 5,2 GB, with a no-break space between number and unit. Dates: vrijdag 13 november.
- Keys: `Ctrl Shift Spatie`, and `Cmd Shift Spatie` on a Mac; Dutch keyboards print Ctrl and Shift.
- Nocturne takes no article and is het (the app as a thing); pdf is written in lower case, as Dutch does.
- A plain space before a unit, and in French before `: ; ? !` and inside « », stands for a no-break space; the build writes it.
- `code` marks a key or a command, **bold** a word the system shows, [brackets] the text of a link, *italic* the emphasised half of the hero title.
- ◊ quotes the operating system's own words. L appears only on translated pages.

## Glossary

Each term is rendered the same way on every page.

| English | Nederlands | Why |
|---|---|---|
| keep (verb, button) | bewaren; knop ‘Bewaren’ | The everyday word for putting something away to find later. |
| Kept. | Bewaard. | Short confirmation. |
| ask (verb, button) | vragen; knop ‘Vragen’ | Plain. |
| remember | onthouden | Plain. |
| the morning page | de ochtendpagina | One compound, reads as a name. |
| what needs you | wat op je wacht | Idiomatic; ‘wat jou nodig heeft’ sounds translated. |
| memories (things kept) | herinneringen | Kept apart from geheugen. |
| memory (of a computer) | geheugen | As the systems say it. |
| space | opslag, opslagruimte | As the systems say it. |
| receipts; the original | bewijs; het origineel | Plain. |
| private; marked private | privé; als privé gemarkeerd | Plain. |
| fade; let go; bring back | vervagen; loslaten; terughalen | Loslaten is natural Dutch for letting go and implies it can be picked up again. |
| forget; forget for good | vergeten; voorgoed vergeten | Voorgoed marks the one final step. |
| tidy; the daily tidy | opruimen; het dagelijkse opruimen | Household word. |
| Away from home | Onderweg | Short, everyday, fits a switch. |
| Online answers | Online antwoorden | Plain. |
| setup (first run) | het instellen | Keeps installatie for installing the app itself. |
| setup (size): Large, Standard, Small, Tiny | variant: Groot, Standaard, Klein, Mini | Variant is plain and does not clash. |
| voice note | spraakmemo | As on phones. |
| graphics card | videokaart | The common Dutch word. |
| its mind | zijn brein | Plain image. |
| AI chip | AI-chip | Only in Systeemeisen, as in English. |
| demo data | voorbeeldgegevens | Plain. |
| journal | dagboek | The everyday word. |
| entry | dagboektekst | Notitie is kept for notes. |
| check in (verb); a check-in | noteren hoe je je voelt; gevoelsnotitie | Inchecken would read as a hotel or an airport. |
| Wound up, Lit up, Low, Settled (the four parts) | Gespannen, Opgetogen, Somber, Rustig | With the English in brackets while the app is English. |
| good thing | iets moois; Mooie dingen | Warmer than goede dingen. |
| reflection | reflectie | Kept apart from the ochtendpagina and from terugblik. |
| recap | terugblik | For a week, a month or a year. |
| streak | reeks | As streak counters in Dutch apps say it. |

## Landing page, head and link preview

| Id | English | Nederlands |
|---|---|---|
| `head.title` | Nocturne · Keep everything, ask about it, get one page each morning | Nocturne · Bewaar alles, vraag ernaar, elke ochtend één pagina |
| `head.description` | Nocturne is a free, private app for your computer. Save notes, photos, voice notes and files without sorting them, now or later. Ask about any of it later, and each morning get one page of what needs you, with the original behind every line. Keep a journal too, if you like. | Nocturne is een gratis, privé-app voor je computer. Bewaar notities, foto’s, spraakmemo’s en bestanden zonder ze te sorteren, niet nu en niet later. Vraag er later alles over, en krijg elke ochtend één pagina met wat op je wacht, met het origineel achter elke regel. Houd ook een dagboek bij, als je wilt. |
| `head.og.description` | Save notes, photos, voice notes and files without sorting them, now or later. Ask about any of it later, and each morning get one page of what needs you, all on your own computer. | Bewaar notities, foto’s, spraakmemo’s en bestanden zonder ze te sorteren, niet nu en niet later. Vraag er later alles over, en krijg elke ochtend één pagina met wat op je wacht, allemaal op je eigen computer. |
| `head.og.image.alt` | Nocturne. Keep everything. Nocturne remembers it for you. Ask about any of it later, and get one page each morning of what needs you. Beside the words, the morning page from the app with demo data. | Nocturne. Bewaar alles. Nocturne onthoudt het voor je. Vraag er later alles over, en krijg elke ochtend één pagina met wat op je wacht. Naast de tekst de ochtendpagina uit de app met voorbeeldgegevens. |

## Header, footer and switches (every page)

| Id | English | Nederlands |
|---|---|---|
| `header.skip` | Skip to content | Naar de inhoud |
| `header.brand.aria` | Nocturne, home | Nocturne, startpagina |
| `nav.aria` | Main | Hoofdmenu |
| `nav.how` | How it works | Zo werkt het |
| `nav.privacy` | Privacy | Privacy |
| `nav.engine` | Engine | Motor |
| `nav.requirements` | Requirements | Systeemeisen |
| `nav.download` | Download | Downloaden |
| `nav.docs` | Docs | Documentatie |
| `theme.aria` | Colour theme | Kleurthema |
| `theme.system` | System | Systeem |
| `theme.light` | Light | Licht |
| `theme.dark` | Dark | Donker |
| `lang.aria` L | Language | Taal |
| `docs.inEnglish` L | in English (read by screen readers after a docs link; the visible mark is EN) | in het Engels |
| `suggest.text` L | This page is also in German. (Shown on the English page, in the suggested language.) | Deze pagina is er ook in het Nederlands. |
| `suggest.link` L | Read in German | Lees in het Nederlands |
| `suggest.close` L | Close this note | Melding sluiten |
| `foot.brand` | Nocturne · engine by Lemonade · Apache 2.0 | Nocturne · motor van Lemonade · Apache 2.0 |
| `foot.aria` | Footer | Voettekst |
| `foot.docs` | Docs | Documentatie |
| `foot.data` | Your data | Je gegevens |
| `foot.download` | Download | Downloaden |
| `foot.install` | Install guide | Installatiehandleiding |
| `foot.support` | Support | Hulp |
| `foot.font` | Font licence | Lettertypelicentie |

## Hero

| Id | English | Nederlands |
|---|---|---|
| `hero.title` | Keep everything. *Nocturne remembers it for you.* | Bewaar alles. *Nocturne onthoudt het voor je.* |
| `hero.lede` | Save notes, photos, voice notes and files without sorting them, now or later. Ask about any of it later, and each morning get one page of what needs you, all on your own computer. | Bewaar notities, foto’s, spraakmemo’s en bestanden zonder ze te sorteren, niet nu en niet later. Vraag er later alles over, en krijg elke ochtend één pagina met wat op je wacht, allemaal op je eigen computer. |
| `hero.download` | Download free | Gratis downloaden |
| `hero.how` | See how it works | Zo werkt het |
| `hero.facts.aria` | At a glance | In het kort |
| `hero.fact.platforms` | Mac, Windows, Linux | Mac, Windows, Linux |
| `hero.fact.offline` | Offline after setup | Offline na het instellen |
| `hero.fact.free` | Free and open source | Gratis en open source |
| `hero.page.alt` | The morning page in Nocturne: Friday 13 November, two things need you today. The Year 4 trip form is due next Friday, with the letter it came from. The dentist appointment changed, with a choice between the card and the voice note. | De ochtendpagina in Nocturne: vrijdag 13 november, vandaag wachten er twee dingen op je. Het formulier voor het schoolreisje moet volgende week vrijdag binnen zijn, met de brief waar het uit komt. De afspraak bij de tandarts is veranderd, met een keuze tussen de afsprakenkaart en de spraakmemo. |
| `hero.keep.alt` | The keep box with “Gran’s new number ends 4471” typed in, and Keep and Ask buttons. | Het bewaarvak met getypte tekst: het nieuwe nummer van oma eindigt op 4471, en de knoppen Bewaren en Vragen. |
| `note.english` L | The app is in English for now. | De app is voorlopig in het Engels. |
| `note.understanding` L | How well it understands notes and voice in this language depends on the mind it downloads, not on Nocturne itself. | Hoe goed het notities en spraak in het Nederlands begrijpt, hangt af van het brein dat het downloadt, niet van Nocturne zelf. |
| `hero.caption` | Keep it in one box, read it on the morning page. From the app, with demo data. | Bewaar het in één vak, lees het op de ochtendpagina. Uit de app, met voorbeeldgegevens. |

## How it works

| Id | English | Nederlands |
|---|---|---|
| `how.eyebrow` | How it works | Zo werkt het |
| `how.title` | Keep it now. Read it once. | Nu bewaren. Eén keer lezen. |
| `keep.eyebrow` | Keep · any time | Bewaren · altijd |
| `keep.title` | Keep anything, file nothing. | Bewaar alles, archiveer niets. |
| `keep.body` | Press `Ctrl Shift Space`, or `Cmd Shift Space` on a Mac, then type, talk, paste or drop a photo or a PDF. Nocturne says “Kept.” and you get back to your day. No folders or tags to keep up: you ask, and it finds it. If it cannot read something, it tells you what to do. | Druk op `Ctrl Shift Spatie`, of `Cmd Shift Spatie` op een Mac, en typ, praat, plak of sleep een foto of pdf erin. Nocturne zegt ‘Bewaard.’ (Kept.) en je gaat verder met je dag. Geen mappen of labels om bij te houden: jij vraagt, en Nocturne vindt het. Als het iets niet kan lezen, zegt het wat je kunt doen. |
| `keep.box.alt` | The quick keep box with “Gran’s new number ends 4471” typed in, and Keep and Ask buttons. | Het snelle bewaarvak met getypte tekst: het nieuwe nummer van oma eindigt op 4471, en de knoppen Bewaren en Vragen. |
| `keep.recent.alt` | Recently kept: a photo still being read, voice notes and pasted messages marked understood, a password-locked PDF Nocturne could not read with what to do about it, and a voice note that needs you. | Pas bewaard: een foto die nog wordt gelezen, spraakmemo’s en geplakte berichten gemarkeerd als begrepen, een pdf met wachtwoord die Nocturne niet kon lezen, met wat je kunt doen, en een spraakmemo die op je wacht. |
| `ask.eyebrow` | Ask · any time | Vragen · altijd |
| `ask.title` | Answers with receipts. | Antwoorden met bewijs. |
| `ask.body` | Ask in plain words. Nocturne answers only from what it can show you, with the originals underneath so you can check. When it does not know, it says so. If it gets something wrong, tell it what is right and it remembers. | Vraag het in gewone woorden. Nocturne antwoordt alleen met wat het je kan laten zien, met de originelen eronder zodat je het kunt nakijken. Als het iets niet weet, zegt het dat. Zit het ernaast, vertel dan wat klopt en het onthoudt het. |
| `ask.alt` | An answer to “What does Leo need for his trip?”: a packed lunch, and the trip form is due next Friday with a signature and £12. Below it, the two photographed letters it came from. | Een antwoord op ‘Wat heeft Leo nodig voor zijn schoolreisje?’: een lunchpakket, en het formulier moet volgende week vrijdag binnen zijn met een handtekening en 12 pond. Eronder de twee gefotografeerde brieven waar het antwoord uit komt. |
| `page.eyebrow` | The morning page · at your time | De ochtendpagina · op jouw tijd |
| `page.title` | Wake to what needs you. | Word wakker met wat op je wacht. |
| `page.body` | Whenever the computer is plugged in and idle, by night or by day, Nocturne reads what you kept and notices what changed. At the time you choose, you get a short list to read, hear or print. When two things disagree, it asks instead of guessing. | Telkens als de computer aan de stroom hangt en niets doet, ’s nachts of overdag, leest Nocturne wat je hebt bewaard en ziet het wat er is veranderd. Op het tijdstip dat jij kiest, krijg je een kort lijstje om te lezen, te beluisteren of te printen. Als twee dingen elkaar tegenspreken, vraagt het in plaats van te gokken. |
| `page.alt` | The morning page: two things need you today. The Year 4 trip form is due next Friday, with the letter it came from. The dentist appointment changed, with a choice between the appointment card and the voice note. | De ochtendpagina: vandaag wachten er twee dingen op je. Het formulier voor het schoolreisje moet volgende week vrijdag binnen zijn, met de brief waar het uit komt. De afspraak bij de tandarts is veranderd, met een keuze tussen de afsprakenkaart en de spraakmemo. |

## The journal

| Id | English | Nederlands |
|---|---|---|
| `journal.eyebrow` | The journal · when you want to | Het dagboek · wanneer je wilt |
| `journal.title` | Your day, in your own words. | Je dag, in je eigen woorden. |
| `journal.body` | Everything you keep during the day is already in your journal, so it starts half written. Add a few lines, a voice note or a photo, and say how you felt. Nocturne looks back on each day, and on each week, month and year in a recap. Every line points to what it came from. | Alles wat je overdag bewaart, staat al in je dagboek, dus het is al half geschreven. Voeg een paar regels, een spraakmemo of een foto toe en zeg hoe je je voelde. Nocturne kijkt terug op elke dag, en in een terugblik op elke week, maand en jaar. Elke regel verwijst naar waar hij vandaan komt. |
| `journal.felt.term` | How you felt, never guessed | Hoe je je voelde, nooit geraden |
| `journal.felt.detail` | Check in as Wound up, Lit up, Low or Settled, and choose the words that fit. A reflection quotes what you said you felt, and never scores the day or gives advice. | Noteer hoe je je voelt: Gespannen (Wound up), Opgetogen (Lit up), Somber (Low) of Rustig (Settled), en kies de woorden die passen. Een reflectie citeert wat je zei dat je voelde, en geeft de dag nooit een cijfer of advies. |
| `journal.good.term` | Good things | Mooie dingen |
| `journal.good.detail` | Up to three a day, with a photo if you like. Your recaps bring them back. | Tot drie per dag, met een foto als je wilt. Je terugblikken halen ze weer naar boven. |
| `journal.calm.term` | No streaks, no nagging | Geen reeksen, geen gezeur |
| `journal.calm.detail` | Nothing counts the days you write or says you missed one. A reminder comes only if you turn one on. When a reflection or recap is new, the morning page says so in one line. | Niets telt de dagen dat je schrijft of zegt dat je er een hebt overgeslagen. Een seintje om te schrijven komt alleen als je dat aanzet. Is er een nieuwe reflectie of terugblik, dan zegt de ochtendpagina dat in één regel. |
| `journal.quiet.term` | It stays quiet | Het blijft stil |
| `journal.quiet.detail` | Nothing from your journal is read aloud or shown in a notification or on a lock screen. If you turn on Online answers, a line from your journal or a check-in can go with a question it helps answer. Your reflections and recaps never do. | Niets uit je dagboek wordt voorgelezen of getoond in een melding of op een vergrendelscherm. Zet je Online antwoorden (Online answers) aan, dan kan een regel uit je dagboek of een gevoelsnotitie meegaan met een vraag die hij helpt beantwoorden. Je reflecties en terugblikken nooit. |
| `journal.day.alt` | Today in the journal, Thursday 12 November: an entry about lunch with Priya, with a voice note and a photo. Beside it, how today felt, with three check-ins (tired, pleased, relieved and content). | Vandaag in het dagboek, donderdag 12 november: een dagboektekst over de lunch met Priya, met een spraakmemo en een foto. Ernaast hoe vandaag voelde, met drie gevoelsnotities (moe, blij, opgelucht en tevreden). |
| `journal.look.alt` | The day, looked back on: a short paragraph about Leo forgetting his PE kit, a tired morning and lunch with Priya at last, with the notes, check-ins, document and entry it came from listed underneath. | De dag, achteraf bekeken: een korte alinea over Leo die zijn gymspullen vergat, een vermoeide ochtend en eindelijk de lunch met Priya, met daaronder de notities, gevoelsnotities, het document en de dagboektekst waar hij uit komt. |
| `journal.caption` | Today’s journal, and the day looked back on. From the app, with demo data. | Het dagboek van vandaag, en de dag achteraf bekeken. Uit de app, met voorbeeldgegevens. |

## What it promises

| Id | English | Nederlands |
|---|---|---|
| `privacy.eyebrow` | What it promises | Wat het belooft |
| `privacy.title` | Your life stays on your computer. | Je leven blijft op je computer. |
| `privacy.body` | Out of the box, Nocturne goes online only for downloads you agree to: its mind at setup and any update you allow. Privacy in the app lists each one with the day and the size. | Standaard gaat Nocturne alleen online voor downloads waarmee je instemt: zijn brein bij het instellen en elke update die je toestaat. Privacy, in de app, toont ze een voor een met de dag en de grootte. |
| `promise.signin.term` | Nothing to sign in to | Nergens op inloggen |
| `promise.signin.detail` | No account, no analytics, no crash reports, no cloud copy. To keep a copy somewhere safe, back everything up to one file, locked with a passphrase you choose. | Geen account, geen analyse, geen crashrapporten, geen kopie in de cloud. Wil je ergens een veilige kopie, maak dan een back-up van alles in één bestand, vergrendeld met een wachtwoord dat jij kiest. |
| `promise.checked.term` | Checked, not just claimed | Gecontroleerd, niet alleen beloofd |
| `promise.checked.detail` | Before a release, Nocturne is run with its network cut off from everything but this computer, and any attempt to reach further fails the check. | Voor een release draait Nocturne met zijn netwerk afgesloten van alles behalve deze computer, en elke poging om verder te komen laat de controle mislukken. |
| `promise.edit.term` | It never edits what you kept | Het past nooit aan wat je hebt bewaard |
| `promise.edit.detail` | Your notes, photos and files are never changed. Every answer shows them underneath. If one is wrong, tell it what is right; the old version is kept, not overwritten. | Je notities, foto’s en bestanden worden nooit veranderd. Elk antwoord toont ze eronder. Klopt er een niet, vertel dan wat wel klopt; de oude versie blijft bewaard en wordt niet overschreven. |
| `promise.forget.term` | It forgets on purpose | Het vergeet met opzet |
| `promise.forget.detail` | Things you no longer use fade, and the page says when one is let go. Bring any back; forgetting for good always asks first. | Wat je niet meer gebruikt, vervaagt, en de pagina zegt het als iets wordt losgelaten. Alles is terug te halen; voorgoed vergeten vraagt altijd eerst. |
| `promise.note.term` | One quiet note a day | Eén rustige melding per dag |
| `promise.note.detail` | When the page is ready, one note says so and how many things need you. Never what you kept. | Als de pagina klaar is, zegt één melding dat, en hoeveel dingen op je wachten. Nooit wat je hebt bewaard. |
| `promise.extras.term` | Two extras, off until you ask | Twee extra’s, uit tot je erom vraagt |
| `promise.extras.detail` | Away from home lets your own phone ask, keep and read your page and journal, sealed so the company in between cannot read it. Online answers sends a question, and the few remembered sentences it needs, to one service you choose. They can include lines from your journal, your check-ins and things you marked private, but never your reflections or recaps. Marking something private keeps it out of notifications, reminders and the lock screen, but it still helps answer what you ask. | Met Onderweg (Away from home) kan je eigen telefoon vragen, bewaren en je pagina en dagboek lezen, verzegeld zodat het bedrijf ertussen het niet kan lezen. Online antwoorden (Online answers) stuurt een vraag, en de paar onthouden zinnen die daarvoor nodig zijn, naar één dienst die jij kiest. Daar kunnen regels uit je dagboek, je gevoelsnotities en als privé gemarkeerde dingen bij zitten, maar nooit je reflecties of terugblikken. Iets als privé markeren houdt het uit meldingen, seintjes en het vergrendelscherm, maar het helpt nog steeds om te beantwoorden wat je vraagt. |
| `phone.alt` | Nocturne on a paired phone: connected to the home computer, with Ask, Keep, today’s page and the journal, and a note that things marked private stay on the computer. | Nocturne op een gekoppelde telefoon: verbonden met de computer thuis, met Vragen, Bewaren, de pagina van vandaag en het dagboek, en een opmerking dat wat als privé is gemarkeerd op de computer blijft. |
| `phone.caption` | Away from home on a paired phone, with demo data. | Onderweg op een gekoppelde telefoon, met voorbeeldgegevens. |

## The engine

| Id | English | Nederlands |
|---|---|---|
| `engine.eyebrow` | The engine | De motor |
| `engine.title` | Built on (followed by the Lemonade logo, alt text “Lemonade”) | Gebouwd op (alt-tekst van het logo ‘Lemonade’) |
| `engine.body1` | Nocturne’s thinking runs on **Embeddable Lemonade**, the portable build of [Lemonade], the open-source local AI server. Nocturne ships its own copy, Lemonade {{LEMONADE_VERSION}}, that only this computer can reach. | Het denkwerk van Nocturne draait op **Embeddable Lemonade**, de draagbare versie van [Lemonade], de opensource lokale AI-server. Nocturne levert een eigen exemplaar mee, Lemonade {{LEMONADE_VERSION}}, dat alleen deze computer kan bereiken. |
| `engine.body2` | Lemonade is free under the Apache 2.0 licence and built by its community, with optimisations by AMD engineers. Nocturne would not exist without it. | Lemonade is gratis onder de Apache 2.0-licentie en wordt gebouwd door zijn community, met optimalisaties door engineers van AMD. Zonder Lemonade zou Nocturne niet bestaan. |

## Requirements (the install page repeats these)

| Id | English | Nederlands |
|---|---|---|
| `req.eyebrow` | Requirements | Systeemeisen |
| `req.title` | Will it run on my computer? | Werkt het op mijn computer? |
| `req.body` | Almost certainly, if it has 8 GB of memory: an 8 GB MacBook Air, or a typical 8 GB Windows or Linux laptop. | Vrijwel zeker, als hij 8 GB geheugen heeft: een MacBook Air met 8 GB, of een gewone Windows- of Linux-laptop met 8 GB. |
| `req.memory.term` | Memory | Geheugen |
| `req.memory.detail` | 8 GB is enough. On an 8 GB computer Nocturne uses about 5 GB at its busiest and gives it back when it has nothing to do. With more memory it offers a larger setup with clearer answers. | 8 GB is genoeg. Op een computer met 8 GB gebruikt Nocturne op zijn drukst ongeveer 5 GB en geeft het dat terug als het niets te doen heeft. Met meer geheugen biedt het een grotere variant met duidelijkere antwoorden. |
| `req.graphics.term` | A graphics card | Een videokaart |
| `req.graphics.detail` | No graphics card needed. Macs with Apple silicon, and computers with AMD or Intel graphics built in, run Nocturne on the chip and share the computer’s memory. An AMD or NVIDIA graphics card works too, best with 6 GB or more. Other computers run it on the processor. | Geen videokaart nodig. Macs met een Apple-chip, en computers met ingebouwde graphics van AMD of Intel, draaien Nocturne op de chip en delen het geheugen van de computer. Een videokaart van AMD of NVIDIA werkt ook, het best met 6 GB of meer. Andere computers draaien het op de processor. |
| `req.chip.term` | An AI chip | Een AI-chip |
| `req.chip.detail` | Helpful, never needed. On AMD Ryzen AI 300 series computers and newer, with 16 GB or more, the AI chip reads your voice notes. On many with 32 GB or more it also reads what you keep, and on some it answers your questions too, so the rest of the computer has less to do. | Handig, nooit nodig. Op computers met AMD Ryzen AI 300-serie of nieuwer, met 16 GB of meer, leest de AI-chip je spraakmemo’s. Op veel computers met 32 GB of meer leest hij ook wat je bewaart, en op sommige beantwoordt hij ook je vragen, zodat de rest van de computer minder te doen heeft. |
| `req.system.term` | The system | Het systeem |
| `req.system.detail` | macOS 26 or later on an Apple chip, Windows 10 or 11 (64-bit), or 64-bit Linux on Intel, AMD or Arm. | macOS 26 of nieuwer op een Apple-chip, Windows 10 of 11 (64-bit), of 64-bit Linux op Intel, AMD of Arm. |
| `tier.large.name` | Large | Groot |
| `tier.large.spec` | 60 GB+ memory · 28 GB space | 60 GB geheugen of meer · 28 GB opslag |
| `tier.large.text` | Clearer answers to harder questions. | Duidelijkere antwoorden op moeilijkere vragen. |
| `tier.standard.name` | Standard | Standaard |
| `tier.standard.spec` | 28 GB+ memory · 11 GB space | 28 GB geheugen of meer · 11 GB opslag |
| `tier.standard.text` | Many recent laptops with 32 GB. | Veel recente laptops met 32 GB. |
| `tier.small.name` | Small | Klein |
| `tier.small.spec` | 14 GB+ memory · 7 GB space | 14 GB geheugen of meer · 7 GB opslag |
| `tier.small.text` | Same page. The daily tidy takes longer. | Dezelfde pagina. Het dagelijkse opruimen duurt langer. |
| `tier.tiny.name` | Tiny | Mini |
| `tier.tiny.spec` | 6 GB+ memory · 5.2 GB space | 6 GB geheugen of meer · 5,2 GB opslag |
| `tier.tiny.text` | Made for 8 GB computers. Simpler answers that may miss details. | Gemaakt voor computers met 8 GB. Eenvoudigere antwoorden die details kunnen missen. |
| `req.downloads` | Downloads at setup are between about 3 GB and 26 GB, depending on the setup, and about 9 GB more where the AI chip reads what you keep. After that, Nocturne works with no internet at all. | De downloads bij het instellen zijn ongeveer 3 GB tot 26 GB, afhankelijk van de variant, en ongeveer 9 GB extra waar de AI-chip leest wat je bewaart. Daarna werkt Nocturne helemaal zonder internet. |

## Get Nocturne (end of the landing page)

| Id | English | Nederlands |
|---|---|---|
| `get.title` | Get Nocturne. | Download Nocturne. |
| `get.body` | Free and open source, for Mac, Windows and Linux. Download it, open it, and it sets itself up for the computer it is on. | Gratis en open source, voor Mac, Windows en Linux. Download het, open het, en het stelt zichzelf in voor de computer waarop het staat. |
| `get.download` | Download | Downloaden |
| `get.source` | Build it from source | Zelf bouwen uit de broncode |

## Not-found page

| Id | English | Nederlands |
|---|---|---|
| `missing.title` | Not here · Nocturne | Niet hier · Nocturne |
| `missing.eyebrow` | Not found | Niet gevonden |
| `missing.heading` | There is no page here. | Hier staat geen pagina. |
| `missing.body` | The link may be old, or the page may have moved. | De link is misschien oud, of de pagina is verhuisd. |
| `missing.links` | [Go to the front page] or [read the docs]. | [Ga naar de voorpagina] of [lees de documentatie]. |

## Install page, head and top

| Id | English | Nederlands |
|---|---|---|
| `install.title` | Get Nocturne · Download for Mac, Windows and Linux | Download Nocturne · voor Mac, Windows en Linux |
| `install.description` | Download Nocturne for a Mac with an Apple chip, a Windows 10 or 11 computer, or a Linux desktop. Free and open source, with no account. The first time it opens, it sets itself up for the computer it is on. | Download Nocturne voor een Mac met een Apple-chip, een computer met Windows 10 of 11, of een Linux-desktop. Gratis en open source, zonder account. De eerste keer dat het opent, stelt het zichzelf in voor de computer waarop het staat. |
| `install.og.description` | Free and open source, with no account. The first time it opens, Nocturne sets itself up for the computer it is on. | Gratis en open source, zonder account. De eerste keer dat het opent, stelt Nocturne zichzelf in voor de computer waarop het staat. |
| `install.heading` | Get Nocturne. | Download Nocturne. |
| `install.lede` | Free and open source, for Mac, Windows and Linux. No account, nothing to sign in to. | Gratis en open source, voor Mac, Windows en Linux. Geen account, nergens op inloggen. |
| `install.here` | Choose your computer below. Each link gives the latest version. | Kies hieronder je computer. Elke link geeft de nieuwste versie. |
| `install.hereTag` | For this computer | Voor deze computer |
| `install.firstOpen` | The first time you open it | De eerste keer openen |
| `install.others` | Other computers | Andere computers |

## Install page, Mac

| Id | English | Nederlands |
|---|---|---|
| `mac.spec` | Apple chip, M1 or later · macOS 26 or later | Apple-chip, M1 of nieuwer · macOS 26 of nieuwer |
| `mac.download` | Download for Mac | Downloaden voor Mac |
| `mac.note` ◊ | Macs with an Intel chip are not supported. To see which chip yours has, open the Apple menu and choose About This Mac. | Macs met een Intel-chip worden niet ondersteund. Welke chip de jouwe heeft, zie je in het Apple-menu onder Over deze Mac. |
| `mac.step1` ◊ | Open the downloaded file, then drag Nocturne into Applications. | Open het gedownloade bestand en sleep Nocturne naar Programma’s. |
| `mac.step2` ◊ | Open Nocturne from Applications. Your Mac says “Nocturne” Not Opened, because it could not check the app. Click **Done**. | Open Nocturne vanuit Programma’s. Je Mac meldt dat ‘Nocturne’ niet is geopend, omdat hij de app niet kon controleren. Klik op **Gereed**. |
| `mac.step3` ◊ | Open System Settings, then **Privacy & Security**. Scroll down to the line saying Nocturne was blocked and click **Open Anyway**. | Open Systeeminstellingen en dan **Privacy en beveiliging**. Scrol omlaag naar de regel dat Nocturne is geblokkeerd en klik op **Open toch**. |
| `mac.step4` ◊ | Enter your password, then click **Open Anyway** again. | Voer je wachtwoord in en klik nog een keer op **Open toch**. |
| `mac.why` | You only do this once. Apple charges a yearly fee to sign an app so it opens without asking; Nocturne is free and open source, and this release is not signed yet. | Dit doe je maar één keer. Apple rekent een jaarlijks bedrag om een app te ondertekenen zodat hij opent zonder te vragen; Nocturne is gratis en open source, en deze versie is nog niet ondertekend. |

## Install page, Windows

| Id | English | Nederlands |
|---|---|---|
| `win.spec` | Windows 10 or 11, 64-bit | Windows 10 of 11, 64-bit |
| `win.download` | Download for Windows | Downloaden voor Windows |
| `win.note` | Installs for you alone and needs no administrator password. Computers managed by an organisation can use the [.msi installer] instead. | Installeert alleen voor jou en vraagt geen beheerderswachtwoord. Computers die een organisatie beheert, kunnen in plaats daarvan het [.msi-installatieprogramma] gebruiken. |
| `win.step1` ◊ | Your browser may warn that the file is not commonly downloaded. In Edge, open the **…** menu beside the file, choose **Keep**, then **Show more**, then **Keep anyway**. | Je browser waarschuwt misschien dat het bestand niet vaak wordt gedownload. Open in Edge het menu **…** naast het bestand, kies **Behouden**, dan **Meer weergeven**, dan **Toch behouden**. |
| `win.step2` ◊ | Open the installer. Windows shows “Windows protected your PC”. Click **More info**, then **Run anyway**. | Open het installatieprogramma. Windows toont ‘Uw pc is beveiligd door Windows’. Klik op **Meer informatie** en dan op **Toch uitvoeren**. |
| `win.why` | You only do this once. Microsoft charges a yearly fee to sign an app so it opens without asking; Nocturne is free and open source, and this release is not signed yet. | Dit doe je maar één keer. Microsoft rekent een jaarlijks bedrag om een app te ondertekenen zodat hij opent zonder te vragen; Nocturne is gratis en open source, en deze versie is nog niet ondertekend. |

## Install page, Linux

| Id | English | Nederlands |
|---|---|---|
| `linux.spec` | 64-bit computer with an Intel, AMD or Arm chip | 64-bit computer met een Intel-, AMD- of Arm-chip |
| `linux.choices.aria` | Ways to install on Linux | Manieren om het op Linux te installeren |
| `linux.flatpak.title` | From your software centre | Uit je softwarecentrum |
| `linux.flatpak.text` | The choice for most people, on any Linux desktop. Your software centre keeps Nocturne up to date. | De keuze voor de meeste mensen, op elke Linux-desktop. Je softwarecentrum houdt Nocturne up-to-date. |
| `linux.flatpak.button` | Get the Flatpak | De Flatpak ophalen |
| `linux.flatpak.terminal` | Or install it from a terminal | Of installeer het vanuit een terminal |
| `linux.deb.title` | Ubuntu, Linux Mint or Debian | Ubuntu, Linux Mint of Debian |
| `linux.package.text` | Open the file with your software centre, and Nocturne appears with your other apps. (Shared by .deb and .rpm.) | Open het bestand met je softwarecentrum, en Nocturne verschijnt bij je andere apps. |
| `linux.deb.intel` | .deb for Intel or AMD | .deb voor Intel of AMD |
| `linux.deb.arm` | .deb for Arm | .deb voor Arm |
| `linux.rpm.title` | Fedora, Red Hat or openSUSE | Fedora, Red Hat of openSUSE |
| `linux.rpm.intel` | .rpm for Intel or AMD | .rpm voor Intel of AMD |
| `linux.rpm.arm` | .rpm for Arm | .rpm voor Arm |
| `linux.appimage.title` | Try it without installing | Probeer het zonder te installeren |
| `linux.appimage.text` | One file that runs on most desktops. In its properties, allow it to run as a program, then open it. | Eén bestand dat op de meeste desktops werkt. Sta in de eigenschappen toe dat het als programma wordt uitgevoerd, en open het dan. |
| `linux.appimage.intel` | AppImage for Intel or AMD | AppImage voor Intel of AMD |
| `linux.appimage.arm` | AppImage for Arm | AppImage voor Arm |
| `linux.note` | Most Linux computers have an Intel or AMD chip. To check, run `uname -m` in a terminal: `x86_64` means Intel or AMD, `aarch64` means Arm. The Flatpak picks the right one by itself. | De meeste Linux-computers hebben een Intel- of AMD-chip. Om het te controleren, voer je `uname -m` uit in een terminal: `x86_64` betekent Intel of AMD, `aarch64` betekent Arm. De Flatpak kiest zelf de juiste. |
| `linux.why` | Linux shows no warning the first time you open Nocturne. | Linux geeft geen waarschuwing de eerste keer dat je Nocturne opent. |

## Install page, checks, needs and after

| Id | English | Nederlands |
|---|---|---|
| `install.checks` | To make sure a download is the one we published, compare it with the fingerprints in [SHA256SUMS]. The [install guide] shows how, explains each package and how to take Nocturne off again. Earlier versions are on the [releases page]. | Om zeker te weten dat een download de versie is die wij hebben gepubliceerd, vergelijk je hem met de vingerafdrukken in [SHA256SUMS]. De [installatiehandleiding] laat zien hoe, legt elk pakket uit en hoe je Nocturne weer verwijdert. Eerdere versies staan op de [releasepagina]. |
| `needs.title` | What your computer needs. (The rest of this section is the Requirements rows above.) | Wat je computer nodig heeft. |
| `after.title` | After you install. | Na de installatie. |
| `after.body` | Nocturne opens a short setup. You can keep things straight away. | Nocturne begint met kort instellen. Je kunt meteen dingen bewaren. |
| `after.1.when` | When it opens | Als het opent |
| `after.1.title` | It looks at the computer | Het bekijkt de computer |
| `after.1.text` | It tells you which setup it will use. You can always pick a smaller one, which uses less memory, space and battery. | Het vertelt welke variant het gaat gebruiken. Je kunt altijd een kleinere kiezen, die minder geheugen, opslag en batterij gebruikt. |
| `after.2.when` | Once, when you agree | Eén keer, als je instemt |
| `after.2.title` | It downloads what it needs | Het downloadt wat het nodig heeft |
| `after.2.text` | It asks first, then downloads what this computer can run: between about 3 GB and 26 GB, and about 9 GB more where the AI chip reads what you keep. This is the only time Nocturne needs the internet, and it carries on by itself if the connection drops. | Het vraagt het eerst en downloadt dan wat deze computer kan draaien: ongeveer 3 GB tot 26 GB, en ongeveer 9 GB extra waar de AI-chip leest wat je bewaart. Dit is de enige keer dat Nocturne internet nodig heeft, en het gaat zelf verder als de verbinding wegvalt. |
| `after.3.when` | At the time you choose | Op het tijdstip dat jij kiest |
| `after.3.title` | Your first page | Je eerste pagina |
| `after.3.text` | From then on, Nocturne works with no internet at all. | Vanaf dan werkt Nocturne helemaal zonder internet. |

## Install page, messages from install.js

| Id | English | Nederlands |
|---|---|---|
| `js.mac` | This looks like a Mac, so its download comes first. Nocturne needs a Mac with an Apple chip. | Dit lijkt een Mac, dus de download daarvoor staat bovenaan. Nocturne heeft een Mac met een Apple-chip nodig. |
| `js.macArm` | This looks like a Mac with an Apple chip, so its download comes first. | Dit lijkt een Mac met een Apple-chip, dus de download daarvoor staat bovenaan. |
| `js.macIntel` ◊ | This looks like a Mac with an Intel chip. Nocturne needs a Mac with an Apple chip (M1 or later), so it will not run on this one. About This Mac, in the Apple menu, shows which chip it has. | Dit lijkt een Mac met een Intel-chip. Nocturne heeft een Mac met een Apple-chip nodig (M1 of nieuwer), dus op deze werkt het niet. Over deze Mac, in het Apple-menu, laat zien welke chip hij heeft. |
| `js.windows` | This looks like a Windows computer, so its download comes first. | Dit lijkt een Windows-computer, dus de download daarvoor staat bovenaan. |
| `js.linux` | This looks like a Linux computer, so its choices come first. | Dit lijkt een Linux-computer, dus de keuzes daarvoor staan bovenaan. |
| `js.linuxArm` | This looks like a Linux computer with an Arm chip, so its choices come first, Arm downloads first. | Dit lijkt een Linux-computer met een Arm-chip, dus de keuzes daarvoor staan bovenaan, met de Arm-downloads eerst. |
| `js.other` | Nocturne is a desktop app, for Mac, Windows and Linux computers. Open this page on the computer you want to use it on, or choose one below. | Nocturne is een desktop-app voor Mac-, Windows- en Linux-computers. Open deze pagina op de computer waarop je het wilt gebruiken, of kies er hieronder een. |

## Support page

Drafted with DeepL on 2026-11-24 (with the glossary's form of address) and reviewed against the glossary and the neighbouring strings the same day. The `req.chip.detail`, `req.downloads`, `linux.flatpak.text`, `after.2.text`, `after.3.when` and `foot.support` rows below and above went through the same pass. Words the app shows are bold, with the English in brackets while the app is English.

| Id | English | Nederlands |
|---|---|---|
| `support.title` | Support · Nocturne | Hulp · Nocturne |
| `support.description` | Help with Nocturne on a computer or a phone: answers from its own guide, what to try first, and how to reach the people who make it. | Hulp bij Nocturne op een computer of telefoon: antwoorden uit de eigen handleiding, wat je eerst kunt proberen en hoe je de mensen erachter bereikt. |
| `support.heading` | Help with Nocturne. | Hulp bij Nocturne. |
| `support.lede` | Nocturne keeps everything on your own computer or phone, so no help desk can see your memory. Help starts with Nocturne itself and its guide, and the people who make it are a message away. | Nocturne bewaart alles op je eigen computer of telefoon, dus geen helpdesk kan je herinneringen zien. Hulp begint bij Nocturne zelf en de handleiding, en de mensen erachter zijn één bericht verwijderd. |
| `support.first.title` | Start here. | Begin hier. |
| `support.ask.title` | Ask Nocturne | Vraag het aan Nocturne |
| `support.ask.text` | On a computer, type your question in the ask box, like “how do I add a phone”. Nocturne answers from the guide that comes with it and shows the part it used. | Typ op een computer je vraag in het vraagvak, bijvoorbeeld ‘hoe voeg ik een telefoon toe’. Nocturne antwoordt uit de handleiding die erbij zit en laat het deel zien dat het heeft gebruikt. |
| `support.phone.title` | On your phone | Op je telefoon |
| `support.phone.text` | Open **This phone**, then **Help**: what this phone can do, its permissions with a way to change them, and the privacy policy. | Open **Deze telefoon** (This phone), dan **Help**: wat deze telefoon kan, de machtigingen met een manier om ze te wijzigen, en het privacybeleid. |
| `support.guide.title` | Read the guide | Lees de handleiding |
| `support.guide.text` | [Installing], [your computer’s desktop], [Away from home], [Online answers] and [your data]. | [Installatie], [het bureaublad van je computer], [Onderweg], [Online antwoorden] en [je gegevens]. |
| `support.qa.title` | Common questions. | Veelgestelde vragen. |
| `support.qa.page.term` | My page didn’t come | Mijn pagina is niet gekomen |
| `support.qa.page.detail` | Nocturne writes it in the hours before your page time, while the computer is on, plugged in and not in use. If it was off or asleep, Nocturne catches up when it wakes. [The guide] explains each desktop. | Nocturne schrijft de pagina in de uren voor het tijdstip van je pagina, terwijl de computer aan staat, aan de stroom hangt en niet gebruikt wordt. Als hij uit stond of sliep, haalt Nocturne het in zodra hij wakker wordt. [De handleiding] legt elk bureaublad uit. |
| `support.qa.lost.term` | I lost my phone | Ik ben mijn telefoon kwijt |
| `support.qa.lost.detail` | If it was paired with your computer, your memory is on the computer: open **Away from home** there, choose **See phones**, then **Remove** beside it. If the phone kept your memory on its own, install Nocturne on the new phone and choose **I have a backup**. | Als hij aan je computer was gekoppeld, staan je herinneringen op de computer: open daar **Onderweg** (Away from home), kies **Telefoons bekijken** (See phones) en dan **Verwijderen** (Remove) ernaast. Als de telefoon je herinneringen zelf bewaarde, installeer dan Nocturne op de nieuwe telefoon en kies **Ik heb een back-up** (I have a backup). |
| `support.qa.passphrase.term` | I forgot my passphrase | Ik ben mijn wachtwoord vergeten |
| `support.qa.passphrase.detail` | No one can open a locked backup without its passphrase or recovery key, not even the people who make Nocturne. Make a new backup with a passphrase you can keep. | Niemand kan een vergrendelde back-up openen zonder het wachtwoord of de herstelsleutel, ook de mensen achter Nocturne niet. Maak een nieuwe back-up met een wachtwoord dat je kunt bewaren. |
| `support.qa.delete.term` | How do I delete everything? | Hoe verwijder ik alles? |
| `support.qa.delete.detail` | Choose **Delete everything…** in **Privacy** on a computer, or in **This phone** on a phone that keeps your memory itself. Nocturne’s makers hold nothing of yours, so there is nothing else to ask for. A service you chose for answers keeps what it received under its own rules. | Kies **Alles verwijderen…** (Delete everything…) bij **Privacy** op een computer, of bij **Deze telefoon** (This phone) op een telefoon die je herinneringen zelf bewaart. De makers van Nocturne bewaren niets van jou, dus er is verder niets om te vragen. Een dienst die je voor antwoorden hebt gekozen, bewaart wat hij heeft ontvangen volgens zijn eigen regels. |
| `support.qa.collect.term` | What does Nocturne collect? | Wat verzamelt Nocturne? |
| `support.qa.collect.detail` | Nothing. No account, no analytics, no crash reports. Only what you turn on can leave your computer or phone. | Niets. Geen account, geen analyse, geen crashrapporten. Alleen wat je zelf aanzet, kan je computer of telefoon verlaten. |
| `support.contact.title` | Write to us. | Schrijf ons. |
| `support.contact.email` | Something not working, or a word that reads wrong? Email [hello@nocturne.is]. A person reads every message. Say which computer or phone you use and what you saw; never send what you kept, your passphrase or your online service key. | Werkt er iets niet, of klinkt een woord verkeerd? Mail naar [hello@nocturne.is]. Elk bericht wordt door een mens gelezen. Zeg welke computer of telefoon je gebruikt en wat je zag; stuur nooit wat je hebt bewaard, je wachtwoord of de sleutel van je online dienst. |
| `support.contact.github` | Or open an issue on [GitHub]. Issues are public, so leave out anything personal. | Of maak een issue aan op [GitHub]. Issues zijn openbaar, dus laat alles wat persoonlijk is weg. |
| `support.contact.security` | To report a security problem, use [GitHub’s private reporting], not a public issue. | Meld een beveiligingsprobleem via [de privémelding van GitHub], niet via een openbaar issue. |

## Decisions

- ◊ rows follow the system's wording and were accepted as written; the Mac folder is written as Programma’s.
- Windows addresses the reader as u in ‘Uw pc is beveiligd door Windows’; the quote keeps it because it is the system’s own text.

## Approval

The owner approved these words on 2026-11-16, with the answers in [the site in five languages](../site-localisation.md#approval).

The journal band, the two reworded Privacy details, the phone picture's description and the meta description were
translated on 2026-11-19 from the English in [the journal on the site](../site-capabilities.md),
with the glossary rows above for the journal.
