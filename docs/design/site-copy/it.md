---
type: Design
title: Site copy, Italiano
description: The approved Italian words for the landing page, the install page and the not-found page, with the glossary they follow.
status: stable
tags: [approved, copy, site, localisation, it]
---

# Site copy, Italiano

The Italian words for the pages described in [the site in five languages](../site-localisation.md).
The strings file `site/i18n/it.json` carries the same words; change both together.

## Register and conventions

- Register: Tu. Italian consumer apps and phone makers address people with tu, and it matches the warm English; Lei would read as an office letter.
- Quotes «…», inner “…”. Apostrophe ’.
- Numbers: 5,2 GB, with a no-break space between number and unit. Dates: venerdì 13 novembre.
- Keys: `Ctrl Maiusc Spazio`, and `Cmd Maiusc Spazio` on a Mac.
- Nocturne takes no article and is masculine (lo, il programma); l’app is feminine where it is the subject.
- A plain space before a unit, and in French before `: ; ? !` and inside « », stands for a no-break space; the build writes it.
- `code` marks a key or a command, **bold** a word the system shows, [brackets] the text of a link, *italic* the emphasised half of the hero title.
- ◊ quotes the operating system's own words. L appears only on translated pages.

## Glossary

Each term is rendered the same way on every page.

| English | Italiano | Why |
|---|---|---|
| keep (verb, button) | conservare; pulsante «Conserva» | Putting something away safely; salvare would sound like saving a file. |
| Kept. | Conservato. | Short confirmation. |
| ask (verb, button) | chiedere; pulsante «Chiedi» | Plain. |
| remember | ricordare | Plain. |
| sort (as in without sorting) | smistare | Keeps riordinare free for tidy. |
| the morning page | la pagina del mattino | Reads as a name. |
| what needs you | quello che ti aspetta | Idiomatic; «ciò che ha bisogno di te» sounds translated. |
| memories (things kept) | ricordi | Kept apart from memoria. |
| memory (of a computer) | memoria | As the systems say it. |
| space | spazio | As the systems say it. |
| receipts; the original | prove; l’originale | Plain. |
| private; marked private | privato; segnato come privato | Segnare is plainer than contrassegnare. |
| fade; let go; bring back | sbiadire; mettere da parte; recuperare | Mettere da parte says it can be taken back. |
| forget; forget for good | dimenticare; dimenticare per sempre | Per sempre marks the one final step. |
| tidy; the daily tidy | riordinare; il riordino quotidiano | Household word. |
| Away from home | Fuori casa | Common phrase, fits a switch. |
| Online answers | Risposte online | Online is plain Italian. |
| setup (first run) | primo avvio | Keeps configurazione free for the sizes. |
| setup (size): Large, Standard, Small, Tiny | configurazione: Grande, Standard, Piccola, Mini | Feminine to agree with configurazione. |
| voice note | nota vocale | As on phones. |
| its mind | il suo cervello | Plain image. |
| AI chip | chip IA | Only in Requisiti, as in English. |
| demo data | dati di esempio | Plain. |
| journal | diario | The everyday word. |
| entry | annotazione | Voce would read as a list item or a voice. |
| check in (verb); a check-in | annotare il tuo stato d’animo; stato d’animo | Fare il check-in would read as a hotel or an airport. |
| Wound up, Lit up, Low, Settled (the four parts) | Sotto pressione, Entusiasta, Giù, A posto | Each fits a man or a woman alike. With the English in brackets while the app is English. |
| good thing | cosa bella | Warmer than cosa buona. |
| reflection | riflessione | Kept apart from the pagina del mattino and from riepilogo. |
| recap | riepilogo | Plain. |
| streak | serie | As streak counters in Italian apps say it. |

## Landing page, head and link preview

| Id | English | Italiano |
|---|---|---|
| `head.title` | Nocturne · Keep everything, ask about it, get one page each morning | Nocturne · Conserva tutto, fai domande, ricevi una pagina ogni mattina |
| `head.description` | Nocturne is a free, private app for your computer. Save notes, photos, voice notes and files without sorting them, now or later. Ask about any of it later, and each morning get one page of what needs you, with the original behind every line. Keep a journal too, if you like. | Nocturne è un’app gratuita e privata per il tuo computer. Conserva note, foto, note vocali e file senza smistarli, né ora né dopo. Più tardi chiedi qualsiasi cosa al riguardo, e ogni mattina ricevi una pagina con quello che ti aspetta, con l’originale dietro ogni riga. Tieni anche un diario, se ti va. |
| `head.og.description` | Save notes, photos, voice notes and files without sorting them, now or later. Ask about any of it later, and each morning get one page of what needs you, all on your own computer. | Conserva note, foto, note vocali e file senza smistarli, né ora né dopo. Più tardi chiedi qualsiasi cosa al riguardo, e ogni mattina ricevi una pagina con quello che ti aspetta, tutto sul tuo computer. |
| `head.og.image.alt` | Nocturne. Keep everything. Nocturne remembers it for you. Ask about any of it later, and get one page each morning of what needs you. Beside the words, the morning page from the app with demo data. | Nocturne. Conserva tutto. Nocturne lo ricorda per te. Più tardi chiedi qualsiasi cosa al riguardo, e ogni mattina ricevi una pagina con quello che ti aspetta. Accanto al testo, la pagina del mattino dell’app con dati di esempio. |

## Header, footer and switches (every page)

| Id | English | Italiano |
|---|---|---|
| `header.skip` | Skip to content | Vai al contenuto |
| `header.brand.aria` | Nocturne, home | Nocturne, pagina iniziale |
| `nav.aria` | Main | Principale |
| `nav.how` | How it works | Come funziona |
| `nav.privacy` | Privacy | Privacy |
| `nav.engine` | Engine | Motore |
| `nav.requirements` | Requirements | Requisiti |
| `nav.download` | Download | Scarica |
| `nav.docs` | Docs | Documentazione |
| `theme.aria` | Colour theme | Tema colori |
| `theme.system` | System | Sistema |
| `theme.light` | Light | Chiaro |
| `theme.dark` | Dark | Scuro |
| `lang.aria` L | Language | Lingua |
| `docs.inEnglish` L | in English (read by screen readers after a docs link; the visible mark is EN) | in inglese |
| `suggest.text` L | This page is also in German. (Shown on the English page, in the suggested language.) | Questa pagina c’è anche in italiano. |
| `suggest.link` L | Read in German | Leggi in italiano |
| `suggest.close` L | Close this note | Chiudi questo avviso |
| `foot.brand` | Nocturne · engine by Lemonade · Apache 2.0 | Nocturne · motore di Lemonade · Apache 2.0 |
| `foot.aria` | Footer | Piè di pagina |
| `foot.docs` | Docs | Documentazione |
| `foot.data` | Your data | I tuoi dati |
| `foot.download` | Download | Scarica |
| `foot.install` | Install guide | Guida all’installazione |
| `foot.support` | Support | Assistenza |
| `foot.font` | Font licence | Licenza dei caratteri |

## Hero

| Id | English | Italiano |
|---|---|---|
| `hero.title` | Keep everything. *Nocturne remembers it for you.* | Conserva tutto. *Nocturne lo ricorda per te.* |
| `hero.lede` | Save notes, photos, voice notes and files without sorting them, now or later. Ask about any of it later, and each morning get one page of what needs you, all on your own computer. | Conserva note, foto, note vocali e file senza smistarli, né ora né dopo. Più tardi chiedi qualsiasi cosa al riguardo, e ogni mattina ricevi una pagina con quello che ti aspetta, tutto sul tuo computer. |
| `hero.download` | Download free | Scarica gratis |
| `hero.how` | See how it works | Guarda come funziona |
| `hero.facts.aria` | At a glance | In breve |
| `hero.fact.platforms` | Mac, Windows, Linux | Mac, Windows, Linux |
| `hero.fact.offline` | Offline after setup | Offline dopo il primo avvio |
| `hero.fact.free` | Free and open source | Gratuito e open source |
| `hero.page.alt` | The morning page in Nocturne: Friday 13 November, two things need you today. The Year 4 trip form is due next Friday, with the letter it came from. The dentist appointment changed, with a choice between the card and the voice note. | La pagina del mattino in Nocturne: venerdì 13 novembre, oggi ti aspettano due cose. Il modulo per la gita scolastica va consegnato venerdì prossimo, con la lettera da cui viene. L’appuntamento dal dentista è cambiato, con la scelta tra il biglietto dell’appuntamento e la nota vocale. |
| `hero.keep.alt` | The keep box with “Gran’s new number ends 4471” typed in, and Keep and Ask buttons. | La casella per conservare con un testo scritto: il nuovo numero della nonna finisce con 4471, e i pulsanti Conserva e Chiedi. |
| `note.english` L | The app is in English for now. | Per ora l’app è in inglese. |
| `note.understanding` L | How well it understands notes and voice in this language depends on the mind it downloads, not on Nocturne itself. | Quanto bene capisce note e voce in italiano dipende dal cervello che scarica, non da Nocturne in sé. |
| `hero.caption` | Keep it in one box, read it on the morning page. From the app, with demo data. | Conserva in una sola casella, leggi sulla pagina del mattino. Dall’app, con dati di esempio. |

## How it works

| Id | English | Italiano |
|---|---|---|
| `how.eyebrow` | How it works | Come funziona |
| `how.title` | Keep it now. Read it once. | Conserva ora. Leggi una volta. |
| `keep.eyebrow` | Keep · any time | Conserva · quando vuoi |
| `keep.title` | Keep anything, file nothing. | Conserva tutto, non archiviare niente. |
| `keep.body` | Press `Ctrl Shift Space`, or `Cmd Shift Space` on a Mac, then type, talk, paste or drop a photo or a PDF. Nocturne says “Kept.” and you get back to your day. No folders or tags to keep up: you ask, and it finds it. If it cannot read something, it tells you what to do. | Premi `Ctrl Maiusc Spazio`, o `Cmd Maiusc Spazio` su un Mac, poi scrivi, parla, incolla o trascina una foto o un PDF. Nocturne dice «Conservato.» (Kept.) e torni alla tua giornata. Niente cartelle o etichette da tenere in ordine: tu chiedi, e Nocturne lo trova. Se non riesce a leggere qualcosa, ti dice cosa fare. |
| `keep.box.alt` | The quick keep box with “Gran’s new number ends 4471” typed in, and Keep and Ask buttons. | La casella per conservare al volo con un testo scritto: il nuovo numero della nonna finisce con 4471, e i pulsanti Conserva e Chiedi. |
| `keep.recent.alt` | Recently kept: a photo still being read, voice notes and pasted messages marked understood, a password-locked PDF Nocturne could not read with what to do about it, and a voice note that needs you. | Conservati di recente: una foto ancora in lettura, note vocali e messaggi incollati segnati come capiti, un PDF protetto da password che Nocturne non è riuscito a leggere, con cosa fare, e una nota vocale che ti aspetta. |
| `ask.eyebrow` | Ask · any time | Chiedi · quando vuoi |
| `ask.title` | Answers with receipts. | Risposte con le prove. |
| `ask.body` | Ask in plain words. Nocturne answers only from what it can show you, with the originals underneath so you can check. When it does not know, it says so. If it gets something wrong, tell it what is right and it remembers. | Chiedi con parole semplici. Nocturne risponde solo in base a ciò che può mostrarti, con gli originali sotto per poter controllare. Quando non lo sa, lo dice. Se sbaglia qualcosa, digli cosa è giusto e se lo ricorda. |
| `ask.alt` | An answer to “What does Leo need for his trip?”: a packed lunch, and the trip form is due next Friday with a signature and £12. Below it, the two photographed letters it came from. | Una risposta a «Cosa serve a Leo per la gita?»: un pranzo al sacco, e il modulo della gita va consegnato venerdì prossimo con una firma e 12 sterline. Sotto, le due lettere fotografate da cui viene la risposta. |
| `page.eyebrow` | The morning page · at your time | La pagina del mattino · all’ora che scegli |
| `page.title` | Wake to what needs you. | Al risveglio, quello che ti aspetta. |
| `page.body` | Whenever the computer is plugged in and idle, by night or by day, Nocturne reads what you kept and notices what changed. At the time you choose, you get a short list to read, hear or print. When two things disagree, it asks instead of guessing. | Ogni volta che il computer è collegato alla corrente e inattivo, di notte o di giorno, Nocturne legge ciò che hai conservato e nota cosa è cambiato. All’ora che scegli ricevi un breve elenco da leggere, ascoltare o stampare. Quando due cose si contraddicono, chiede invece di tirare a indovinare. |
| `page.alt` | The morning page: two things need you today. The Year 4 trip form is due next Friday, with the letter it came from. The dentist appointment changed, with a choice between the appointment card and the voice note. | La pagina del mattino: oggi ti aspettano due cose. Il modulo per la gita scolastica va consegnato venerdì prossimo, con la lettera da cui viene. L’appuntamento dal dentista è cambiato, con la scelta tra il biglietto dell’appuntamento e la nota vocale. |

## The journal

| Id | English | Italiano |
|---|---|---|
| `journal.eyebrow` | The journal · when you want to | Il diario · quando ne hai voglia |
| `journal.title` | Your day, in your own words. | La tua giornata, con le tue parole. |
| `journal.body` | Everything you keep during the day is already in your journal, so it starts half written. Add a few lines, a voice note or a photo, and say how you felt. Nocturne looks back on each day, and on each week, month and year in a recap. Every line points to what it came from. | Tutto quello che conservi durante la giornata è già nel tuo diario, così parte scritto a metà. Aggiungi qualche riga, una nota vocale o una foto, e di’ come ti senti. Nocturne ripercorre ogni giornata, e ogni settimana, mese e anno in un riepilogo. Ogni riga rimanda a ciò da cui viene. |
| `journal.felt.term` | How you felt, never guessed | Come ti senti, senza supposizioni |
| `journal.felt.detail` | Check in as Wound up, Lit up, Low or Settled, and choose the words that fit. A reflection quotes what you said you felt, and never scores the day or gives advice. | Annota il tuo stato d’animo: Sotto pressione (Wound up), Entusiasta (Lit up), Giù (Low) o A posto (Settled), e scegli le parole giuste. Una riflessione riporta ciò che hai detto di provare, e non dà mai voti alla giornata né consigli. |
| `journal.good.term` | Good things | Cose belle |
| `journal.good.detail` | Up to three a day, with a photo if you like. Your recaps bring them back. | Fino a tre al giorno, con una foto se vuoi. I tuoi riepiloghi te le riportano. |
| `journal.calm.term` | No streaks, no nagging | Niente serie, niente assilli |
| `journal.calm.detail` | Nothing counts the days you write or says you missed one. A reminder comes only if you turn one on. When a reflection or recap is new, the morning page says so in one line. | Niente conta i giorni in cui scrivi né ti dice che ne hai saltato uno. Un promemoria arriva solo se lo attivi. Quando c’è una riflessione o un riepilogo nuovo, la pagina del mattino lo dice in una riga. |
| `journal.quiet.term` | It stays quiet | Resta discreto |
| `journal.quiet.detail` | Nothing from your journal is read aloud or shown in a notification or on a lock screen. If you turn on Online answers, a line from your journal can go with a question it helps answer. Your check-ins, reflections and recaps never do. | Niente del tuo diario viene letto ad alta voce o mostrato in una notifica o sulla schermata di blocco. Se attivi Risposte online (Online answers), una riga del tuo diario può accompagnare una domanda a cui aiuta a rispondere. I tuoi stati d’animo, le riflessioni e i riepiloghi, mai. |
| `journal.day.alt` | Today in the journal, Thursday 12 November: an entry about lunch with Priya, with a voice note and a photo. Beside it, how today felt, with three check-ins (tired, pleased, relieved and content). | Oggi nel diario, giovedì 12 novembre: un’annotazione sul pranzo con Priya, con una nota vocale e una foto. Accanto, gli stati d’animo di oggi, annotati tre volte (stanchezza, soddisfazione, sollievo e appagamento). |
| `journal.look.alt` | The day, looked back on: a short paragraph about Leo forgetting his PE kit, a tired morning and lunch with Priya at last, with the notes, check-ins, document and entry it came from listed underneath. | La giornata, ripercorsa: un breve paragrafo su Leo che ha dimenticato la roba di ginnastica, una mattina di stanchezza e finalmente il pranzo con Priya, con sotto le note, gli stati d’animo, il documento e l’annotazione da cui viene. |
| `journal.caption` | Today’s journal, and the day looked back on. From the app, with demo data. | Il diario di oggi e la giornata ripercorsa. Dall’app, con dati di esempio. |

## What it promises

| Id | English | Italiano |
|---|---|---|
| `privacy.eyebrow` | What it promises | Cosa promette |
| `privacy.title` | Your life stays on your computer. | La tua vita resta sul tuo computer. |
| `privacy.body` | Out of the box, Nocturne goes online only for downloads you agree to: its mind at setup and any update you allow. Privacy in the app lists each one with the day and the size. | Di serie, Nocturne va online solo per i download che approvi: il suo cervello al primo avvio e ogni aggiornamento che consenti. Privacy, nell’app, li elenca uno per uno con giorno e dimensione. |
| `promise.signin.term` | Nothing to sign in to | Nessun accesso da fare |
| `promise.signin.detail` | No account, no analytics, no crash reports, no cloud copy. To keep a copy somewhere safe, back everything up to one file, locked with a passphrase you choose. | Nessun account, nessuna statistica, nessun rapporto sugli arresti anomali, nessuna copia nel cloud. Per tenere una copia al sicuro, fai il backup di tutto in un solo file, protetto da una password che scegli tu. |
| `promise.checked.term` | Checked, not just claimed | Verificato, non solo promesso |
| `promise.checked.detail` | Before a release, Nocturne is run with its network cut off from everything but this computer, and any attempt to reach further fails the check. | Prima di una versione, Nocturne viene eseguito con la rete tagliata verso tutto tranne questo computer, e qualsiasi tentativo di andare oltre fa fallire la verifica. |
| `promise.edit.term` | It never edits what you kept | Non modifica mai ciò che hai conservato |
| `promise.edit.detail` | Your notes, photos and files are never changed. Every answer shows them underneath. If one is wrong, tell it what is right; the old version is kept, not overwritten. | Le tue note, foto e file non vengono mai cambiati. Ogni risposta li mostra sotto. Se una è sbagliata, digli cosa è giusto; la versione precedente resta, non viene sovrascritta. |
| `promise.forget.term` | It forgets on purpose | Dimentica apposta |
| `promise.forget.detail` | Things you no longer use fade, and the page says when one is let go. Bring any back; forgetting for good always asks first. | Ciò che non usi più sbiadisce, e la pagina ti dice quando qualcosa viene messo da parte. Puoi recuperare qualsiasi cosa; per dimenticare per sempre chiede sempre prima. |
| `promise.note.term` | One quiet note a day | Un solo avviso discreto al giorno |
| `promise.note.detail` | When the page is ready, one note says so and how many things need you. Never what you kept. | Quando la pagina è pronta, un avviso lo dice, insieme a quante cose ti aspettano. Mai cosa hai conservato. |
| `promise.extras.term` | Two extras, off until you ask | Due extra, spenti finché non li chiedi |
| `promise.extras.detail` | Away from home lets your own phone ask, keep and read your page and journal, sealed so the company in between cannot read it. Online answers sends a question, and the few remembered sentences it needs, to one service you choose. They can include lines from your journal, your check-ins and things you marked private, but never your reflections or recaps. Marking something private keeps it out of notifications, reminders and the lock screen, but it still helps answer what you ask. | Fuori casa (Away from home) permette al tuo telefono di chiedere, conservare e leggere la tua pagina e il tuo diario, in forma sigillata perché l’azienda in mezzo non possa leggerli. Risposte online (Online answers) invia una domanda, e le poche frasi ricordate che le servono, a un solo servizio che scegli tu. Possono includere righe del tuo diario, i tuoi stati d’animo e cose segnate come private, ma mai le riflessioni o i riepiloghi. Segnare qualcosa come privato lo tiene fuori dalle notifiche, dai promemoria e dalla schermata di blocco, ma continua ad aiutare a rispondere a ciò che chiedi. |
| `phone.alt` | Nocturne on a paired phone: connected to the home computer, with Ask, Keep, today’s page and the journal, and a note that things marked private stay on the computer. | Nocturne su un telefono abbinato: collegato al computer di casa, con Chiedi, Conserva, la pagina di oggi e il diario, e una nota che dice che le cose segnate come private restano sul computer. |
| `phone.caption` | Away from home on a paired phone, with demo data. | Fuori casa su un telefono abbinato, con dati di esempio. |

## The engine

| Id | English | Italiano |
|---|---|---|
| `engine.eyebrow` | The engine | Il motore |
| `engine.title` | Built on (followed by the Lemonade logo, alt text “Lemonade”) | Costruito su (testo alternativo del logo «Lemonade») |
| `engine.body1` | Nocturne’s thinking runs on **Embeddable Lemonade**, the portable build of [Lemonade], the open-source local AI server. Nocturne ships its own copy, Lemonade {{LEMONADE_VERSION}}, that only this computer can reach. | Il pensiero di Nocturne gira su **Embeddable Lemonade**, la versione portatile di [Lemonade], il server di IA locale e open source. Nocturne include una sua copia, Lemonade {{LEMONADE_VERSION}}, che solo questo computer può raggiungere. |
| `engine.body2` | Lemonade is free under the Apache 2.0 licence and built by its community, with optimisations by AMD engineers. Nocturne would not exist without it. | Lemonade è gratuito con licenza Apache 2.0 ed è sviluppato dalla sua comunità, con ottimizzazioni degli ingegneri di AMD. Senza Lemonade, Nocturne non esisterebbe. |

## Requirements (the install page repeats these)

| Id | English | Italiano |
|---|---|---|
| `req.eyebrow` | Requirements | Requisiti |
| `req.title` | Will it run on my computer? | Funzionerà sul mio computer? |
| `req.body` | Almost certainly, if it has 8 GB of memory: an 8 GB MacBook Air, or a typical 8 GB Windows or Linux laptop. | Quasi certamente, se ha 8 GB di memoria: un MacBook Air da 8 GB, o un normale portatile Windows o Linux da 8 GB. |
| `req.memory.term` | Memory | Memoria |
| `req.memory.detail` | 8 GB is enough. On an 8 GB computer Nocturne uses about 5 GB at its busiest and gives it back when it has nothing to do. With more memory it offers a larger setup with clearer answers. | 8 GB bastano. Su un computer da 8 GB, Nocturne usa circa 5 GB nei momenti di massimo lavoro e li restituisce quando non ha niente da fare. Con più memoria propone una configurazione più grande con risposte più chiare. |
| `req.graphics.term` | A graphics card | Una scheda grafica |
| `req.graphics.detail` | No graphics card needed. Macs with Apple silicon, and computers with AMD or Intel graphics built in, run Nocturne on the chip and share the computer’s memory. An AMD or NVIDIA graphics card works too, best with 6 GB or more. Other computers run it on the processor. | Non serve una scheda grafica. I Mac con chip Apple, e i computer con grafica AMD o Intel integrata, eseguono Nocturne sul chip e condividono la memoria del computer. Va bene anche una scheda grafica AMD o NVIDIA, meglio con 6 GB o più. Gli altri computer lo eseguono sul processore. |
| `req.chip.term` | An AI chip | Un chip IA |
| `req.chip.detail` | Helpful, never needed. On AMD Ryzen AI 300 series computers and newer, with 16 GB or more, the AI chip reads your voice notes. On many with 32 GB or more it also reads what you keep, and on some it answers your questions too, so the rest of the computer has less to do. | Utile, mai necessario. Sui computer AMD Ryzen AI serie 300 e successivi, con 16 GB o più, il chip IA legge le tue note vocali. Su molti con 32 GB o più legge anche ciò che conservi, e su alcuni risponde anche alle tue domande, così il resto del computer ha meno da fare. |
| `req.system.term` | The system | Il sistema |
| `req.system.detail` | macOS 26 or later on an Apple chip, Windows 10 or 11 (64-bit), or 64-bit Linux on Intel, AMD or Arm. | macOS 26 o successivo su chip Apple, Windows 10 o 11 (64 bit), o Linux a 64 bit su Intel, AMD o Arm. |
| `tier.large.name` | Large | Grande |
| `tier.large.spec` | 60 GB+ memory · 28 GB space | 60 GB di memoria o più · 28 GB di spazio |
| `tier.large.text` | Clearer answers to harder questions. | Risposte più chiare a domande più difficili. |
| `tier.standard.name` | Standard | Standard |
| `tier.standard.spec` | 28 GB+ memory · 11 GB space | 28 GB di memoria o più · 11 GB di spazio |
| `tier.standard.text` | Many recent laptops with 32 GB. | Molti portatili recenti con 32 GB. |
| `tier.small.name` | Small | Piccola |
| `tier.small.spec` | 14 GB+ memory · 7 GB space | 14 GB di memoria o più · 7 GB di spazio |
| `tier.small.text` | Same page. The daily tidy takes longer. | La stessa pagina. Il riordino quotidiano richiede più tempo. |
| `tier.tiny.name` | Tiny | Mini |
| `tier.tiny.spec` | 6 GB+ memory · 5.2 GB space | 6 GB di memoria o più · 5,2 GB di spazio |
| `tier.tiny.text` | Made for 8 GB computers. Simpler answers that may miss details. | Pensata per computer da 8 GB. Risposte più semplici che possono perdere dei dettagli. |
| `req.downloads` | Downloads at setup are between about 3 GB and 26 GB, depending on the setup, and about 9 GB more where the AI chip reads what you keep. After that, Nocturne works with no internet at all. | I download del primo avvio vanno da circa 3 GB a 26 GB, a seconda della configurazione, e circa 9 GB in più dove il chip IA legge ciò che conservi. Dopo, Nocturne funziona senza alcuna connessione a internet. |

## Get Nocturne (end of the landing page)

| Id | English | Italiano |
|---|---|---|
| `get.title` | Get Nocturne. | Scarica Nocturne. |
| `get.body` | Free and open source, for Mac, Windows and Linux. Download it, open it, and it sets itself up for the computer it is on. | Gratuito e open source, per Mac, Windows e Linux. Scaricalo, aprilo, e si prepara da solo per il computer su cui si trova. |
| `get.download` | Download | Scarica |
| `get.source` | Build it from source | Compilalo dal codice sorgente |

## Not-found page

| Id | English | Italiano |
|---|---|---|
| `missing.title` | Not here · Nocturne | Non c’è · Nocturne |
| `missing.eyebrow` | Not found | Non trovata |
| `missing.heading` | There is no page here. | Qui non c’è nessuna pagina. |
| `missing.body` | The link may be old, or the page may have moved. | Il link potrebbe essere vecchio, o la pagina potrebbe essere stata spostata. |
| `missing.links` | [Go to the front page] or [read the docs]. | [Vai alla pagina iniziale] o [leggi la documentazione]. |

## Install page, head and top

| Id | English | Italiano |
|---|---|---|
| `install.title` | Get Nocturne · Download for Mac, Windows and Linux | Scarica Nocturne · per Mac, Windows e Linux |
| `install.description` | Download Nocturne for a Mac with an Apple chip, a Windows 10 or 11 computer, or a Linux desktop. Free and open source, with no account. The first time it opens, it sets itself up for the computer it is on. | Scarica Nocturne per un Mac con chip Apple, un computer con Windows 10 o 11, o un desktop Linux. Gratuito e open source, senza account. La prima volta che si apre, si prepara da solo per il computer su cui si trova. |
| `install.og.description` | Free and open source, with no account. The first time it opens, Nocturne sets itself up for the computer it is on. | Gratuito e open source, senza account. La prima volta che si apre, Nocturne si prepara da solo per il computer su cui si trova. |
| `install.heading` | Get Nocturne. | Scarica Nocturne. |
| `install.lede` | Free and open source, for Mac, Windows and Linux. No account, nothing to sign in to. | Gratuito e open source, per Mac, Windows e Linux. Nessun account, nessun accesso da fare. |
| `install.here` | Choose your computer below. Each link gives the latest version. | Scegli il tuo computer qui sotto. Ogni link porta all’ultima versione. |
| `install.hereTag` | For this computer | Per questo computer |
| `install.firstOpen` | The first time you open it | La prima volta che lo apri |
| `install.others` | Other computers | Altri computer |

## Install page, Mac

| Id | English | Italiano |
|---|---|---|
| `mac.spec` | Apple chip, M1 or later · macOS 26 or later | Chip Apple, M1 o successivo · macOS 26 o successivo |
| `mac.download` | Download for Mac | Scarica per Mac |
| `mac.note` ◊ | Macs with an Intel chip are not supported. To see which chip yours has, open the Apple menu and choose About This Mac. | I Mac con chip Intel non sono supportati. Per vedere quale chip ha il tuo, apri il menu Apple e scegli Informazioni su questo Mac. |
| `mac.step1` ◊ | Open the downloaded file, then drag Nocturne into Applications. | Apri il file scaricato, poi trascina Nocturne in Applicazioni. |
| `mac.step2` ◊ | Open Nocturne from Applications. Your Mac says “Nocturne” Not Opened, because it could not check the app. Click **Done**. | Apri Nocturne da Applicazioni. Il Mac dice che «Nocturne» non è stato aperto, perché non ha potuto verificare l’app. Fai clic su **Fine**. |
| `mac.step3` ◊ | Open System Settings, then **Privacy & Security**. Scroll down to the line saying Nocturne was blocked and click **Open Anyway**. | Apri Impostazioni di Sistema, poi **Privacy e sicurezza**. Scorri fino alla riga che dice che Nocturne è stato bloccato e fai clic su **Apri comunque**. |
| `mac.step4` ◊ | Enter your password, then click **Open Anyway** again. | Inserisci la password, poi fai di nuovo clic su **Apri comunque**. |
| `mac.why` | You only do this once. Apple charges a yearly fee to sign an app so it opens without asking; Nocturne is free and open source, and this release is not signed yet. | Lo fai una volta sola. Apple chiede una quota annuale per firmare un’app in modo che si apra senza chiedere; Nocturne è gratuito e open source, e questa versione non è ancora firmata. |

## Install page, Windows

| Id | English | Italiano |
|---|---|---|
| `win.spec` | Windows 10 or 11, 64-bit | Windows 10 o 11, 64 bit |
| `win.download` | Download for Windows | Scarica per Windows |
| `win.note` | Installs for you alone and needs no administrator password. Computers managed by an organisation can use the [.msi installer] instead. | Si installa solo per te e non serve la password di amministratore. I computer gestiti da un’organizzazione possono usare invece il [programma di installazione .msi]. |
| `win.step1` ◊ | Your browser may warn that the file is not commonly downloaded. In Edge, open the **…** menu beside the file, choose **Keep**, then **Show more**, then **Keep anyway**. | Il browser potrebbe avvisarti che il file non viene scaricato spesso. In Edge, apri il menu **…** accanto al file, scegli **Mantieni**, poi **Mostra altro**, poi **Mantieni comunque**. |
| `win.step2` ◊ | Open the installer. Windows shows “Windows protected your PC”. Click **More info**, then **Run anyway**. | Apri il programma di installazione. Windows mostra «PC protetto da Windows». Fai clic su **Ulteriori informazioni**, poi su **Esegui comunque**. |
| `win.why` | You only do this once. Microsoft charges a yearly fee to sign an app so it opens without asking; Nocturne is free and open source, and this release is not signed yet. | Lo fai una volta sola. Microsoft chiede una quota annuale per firmare un’app in modo che si apra senza chiedere; Nocturne è gratuito e open source, e questa versione non è ancora firmata. |

## Install page, Linux

| Id | English | Italiano |
|---|---|---|
| `linux.spec` | 64-bit computer with an Intel, AMD or Arm chip | Computer a 64 bit con chip Intel, AMD o Arm |
| `linux.choices.aria` | Ways to install on Linux | Modi per installarlo su Linux |
| `linux.flatpak.title` | From your software centre | Dal tuo centro software |
| `linux.flatpak.text` | The choice for most people, on any Linux desktop. Your software centre keeps Nocturne up to date. | La scelta per la maggior parte delle persone, su qualsiasi desktop Linux. Il tuo centro software tiene Nocturne aggiornato. |
| `linux.flatpak.button` | Get the Flatpak | Ottieni il Flatpak |
| `linux.flatpak.terminal` | Or install it from a terminal | Oppure installalo da un terminale |
| `linux.deb.title` | Ubuntu, Linux Mint or Debian | Ubuntu, Linux Mint o Debian |
| `linux.package.text` | Open the file with your software centre, and Nocturne appears with your other apps. (Shared by .deb and .rpm.) | Apri il file con il tuo centro software, e Nocturne compare insieme alle altre app. |
| `linux.deb.intel` | .deb for Intel or AMD | .deb per Intel o AMD |
| `linux.deb.arm` | .deb for Arm | .deb per Arm |
| `linux.rpm.title` | Fedora, Red Hat or openSUSE | Fedora, Red Hat o openSUSE |
| `linux.rpm.intel` | .rpm for Intel or AMD | .rpm per Intel o AMD |
| `linux.rpm.arm` | .rpm for Arm | .rpm per Arm |
| `linux.appimage.title` | Try it without installing | Provalo senza installarlo |
| `linux.appimage.text` | One file that runs on most desktops. In its properties, allow it to run as a program, then open it. | Un unico file che funziona sulla maggior parte dei desktop. Nelle sue proprietà, consenti di eseguirlo come programma, poi aprilo. |
| `linux.appimage.intel` | AppImage for Intel or AMD | AppImage per Intel o AMD |
| `linux.appimage.arm` | AppImage for Arm | AppImage per Arm |
| `linux.note` | Most Linux computers have an Intel or AMD chip. To check, run `uname -m` in a terminal: `x86_64` means Intel or AMD, `aarch64` means Arm. The Flatpak picks the right one by itself. | La maggior parte dei computer Linux ha un chip Intel o AMD. Per controllare, esegui `uname -m` in un terminale: `x86_64` significa Intel o AMD, `aarch64` significa Arm. Il Flatpak sceglie da solo quello giusto. |
| `linux.why` | Linux shows no warning the first time you open Nocturne. | Linux non mostra alcun avviso la prima volta che apri Nocturne. |

## Install page, checks, needs and after

| Id | English | Italiano |
|---|---|---|
| `install.checks` | To make sure a download is the one we published, compare it with the fingerprints in [SHA256SUMS]. The [install guide] shows how, explains each package and how to take Nocturne off again. Earlier versions are on the [releases page]. | Per verificare che un download sia quello che abbiamo pubblicato, confrontalo con le impronte in [SHA256SUMS]. La [guida all’installazione] spiega come fare, descrive ogni pacchetto e come togliere Nocturne. Le versioni precedenti sono nella [pagina delle versioni]. |
| `needs.title` | What your computer needs. (The rest of this section is the Requirements rows above.) | Cosa serve al tuo computer. |
| `after.title` | After you install. | Dopo l’installazione. |
| `after.body` | Nocturne opens a short setup. You can keep things straight away. | Al primo avvio Nocturne ti guida in pochi passaggi. Puoi conservare cose da subito. |
| `after.1.when` | When it opens | All’apertura |
| `after.1.title` | It looks at the computer | Esamina il computer |
| `after.1.text` | It tells you which setup it will use. You can always pick a smaller one, which uses less memory, space and battery. | Ti dice quale configurazione userà. Puoi sempre sceglierne una più piccola, che usa meno memoria, spazio e batteria. |
| `after.2.when` | Once, when you agree | Una volta, se sei d’accordo |
| `after.2.title` | It downloads what it needs | Scarica ciò che gli serve |
| `after.2.text` | It asks first, then downloads what this computer can run: between about 3 GB and 26 GB, and about 9 GB more where the AI chip reads what you keep. This is the only time Nocturne needs the internet, and it carries on by itself if the connection drops. | Prima chiede, poi scarica ciò che questo computer può eseguire: tra circa 3 GB e 26 GB, e circa 9 GB in più dove il chip IA legge ciò che conservi. È l’unica volta in cui Nocturne ha bisogno di internet, e riprende da solo se la connessione cade. |
| `after.3.when` | At the time you choose | All’ora che scegli |
| `after.3.title` | Your first page | La tua prima pagina |
| `after.3.text` | From then on, Nocturne works with no internet at all. | Da quel momento Nocturne funziona senza alcuna connessione a internet. |

## Install page, messages from install.js

| Id | English | Italiano |
|---|---|---|
| `js.mac` | This looks like a Mac, so its download comes first. Nocturne needs a Mac with an Apple chip. | Sembra un Mac, quindi il suo download è in cima. Nocturne richiede un Mac con chip Apple. |
| `js.macArm` | This looks like a Mac with an Apple chip, so its download comes first. | Sembra un Mac con chip Apple, quindi il suo download è in cima. |
| `js.macIntel` ◊ | This looks like a Mac with an Intel chip. Nocturne needs a Mac with an Apple chip (M1 or later), so it will not run on this one. About This Mac, in the Apple menu, shows which chip it has. | Sembra un Mac con chip Intel. Nocturne richiede un Mac con chip Apple (M1 o successivo), quindi su questo non funzionerà. Informazioni su questo Mac, nel menu Apple, mostra quale chip ha. |
| `js.windows` | This looks like a Windows computer, so its download comes first. | Sembra un computer Windows, quindi il suo download è in cima. |
| `js.linux` | This looks like a Linux computer, so its choices come first. | Sembra un computer Linux, quindi le sue opzioni sono in cima. |
| `js.linuxArm` | This looks like a Linux computer with an Arm chip, so its choices come first, Arm downloads first. | Sembra un computer Linux con chip Arm, quindi le sue opzioni sono in cima, con i download per Arm per primi. |
| `js.other` | Nocturne is a desktop app, for Mac, Windows and Linux computers. Open this page on the computer you want to use it on, or choose one below. | Nocturne è un’app per computer desktop Mac, Windows e Linux. Apri questa pagina sul computer su cui vuoi usarla, oppure scegline uno qui sotto. |

## Support page

Drafted with DeepL on 2026-11-24 (with the glossary's form of address) and reviewed against the glossary and the neighbouring strings the same day. The `req.chip.detail`, `req.downloads`, `linux.flatpak.text`, `after.2.text`, `after.3.when` and `foot.support` rows below and above went through the same pass. Words the app shows are bold, with the English in brackets while the app is English.

| Id | English | Italiano |
|---|---|---|
| `support.title` | Support · Nocturne | Assistenza · Nocturne |
| `support.description` | Help with Nocturne on a computer or a phone: answers from its own guide, what to try first, and how to reach the people who make it. | Aiuto con Nocturne su computer o telefono: risposte dalla sua guida, cosa provare per prima cosa e come raggiungere le persone che lo fanno. |
| `support.heading` | Help with Nocturne. | Aiuto con Nocturne. |
| `support.lede` | Nocturne keeps everything on your own computer or phone, so no help desk can see your memory. Help starts with Nocturne itself and its guide, and the people who make it are a message away. | Nocturne conserva tutto sul tuo computer o telefono, quindi nessun servizio di assistenza può vedere i tuoi ricordi. L’aiuto parte da Nocturne stesso e dalla sua guida, e le persone che lo fanno sono a un messaggio di distanza. |
| `support.first.title` | Start here. | Parti da qui. |
| `support.ask.title` | Ask Nocturne | Chiedi a Nocturne |
| `support.ask.text` | On a computer, type your question in the ask box, like “how do I add a phone”. Nocturne answers from the guide that comes with it and shows the part it used. | Su un computer, scrivi la tua domanda nella casella delle domande, per esempio «come aggiungo un telefono». Nocturne risponde dalla guida che ha con sé e mostra la parte che ha usato. |
| `support.phone.title` | On your phone | Sul tuo telefono |
| `support.phone.text` | Open **This phone**, then **Help**: what this phone can do, its permissions with a way to change them, and the privacy policy. | Apri **Questo telefono** (This phone), poi **Aiuto** (Help): cosa può fare questo telefono, i suoi permessi con un modo per cambiarli e l’informativa sulla privacy. |
| `support.guide.title` | Read the guide | Leggi la guida |
| `support.guide.text` | [Installing], [your computer’s desktop], [Away from home], [Online answers] and [your data]. | [Installazione], [il desktop del tuo computer], [Fuori casa], [Risposte online] e [i tuoi dati]. |
| `support.qa.title` | Common questions. | Domande frequenti. |
| `support.qa.page.term` | My page didn’t come | La mia pagina non è arrivata |
| `support.qa.page.detail` | Nocturne writes it in the hours before your page time, while the computer is on, plugged in and not in use. If it was off or asleep, Nocturne catches up when it wakes. [The guide] explains each desktop. | Nocturne la scrive nelle ore prima dell’ora della tua pagina, mentre il computer è acceso, collegato alla corrente e non in uso. Se era spento o in sospensione, Nocturne recupera al risveglio. [La guida] spiega ogni desktop. |
| `support.qa.lost.term` | I lost my phone | Ho perso il telefono |
| `support.qa.lost.detail` | If it was paired with your computer, your memory is on the computer: open **Away from home** there, choose **See phones**, then **Remove** beside it. If the phone kept your memory on its own, install Nocturne on the new phone and choose **I have a backup**. | Se era associato al tuo computer, i tuoi ricordi sono sul computer: apri lì **Fuori casa** (Away from home), scegli **Vedi i telefoni** (See phones), poi **Rimuovi** (Remove) accanto al suo. Se il telefono conservava i tuoi ricordi da solo, installa Nocturne sul telefono nuovo e scegli **Ho un backup** (I have a backup). |
| `support.qa.passphrase.term` | I forgot my passphrase | Ho dimenticato la password |
| `support.qa.passphrase.detail` | No one can open a locked backup without its passphrase or recovery key, not even the people who make Nocturne. Make a new backup with a passphrase you can keep. | Nessuno può aprire un backup bloccato senza la sua password o la chiave di recupero, nemmeno le persone che fanno Nocturne. Fai un nuovo backup con una password che puoi conservare. |
| `support.qa.delete.term` | How do I delete everything? | Come cancello tutto? |
| `support.qa.delete.detail` | Choose **Delete everything…** in **Privacy** on a computer, or in **This phone** on a phone that keeps your memory itself. Nocturne’s makers hold nothing of yours, so there is nothing else to ask for. A service you chose for answers keeps what it received under its own rules. | Scegli **Elimina tutto…** (Delete everything…) in **Privacy** su un computer, oppure in **Questo telefono** (This phone) su un telefono che conserva da solo i tuoi ricordi. Chi fa Nocturne non conserva nulla di tuo, quindi non c’è altro da chiedere. Un servizio che hai scelto per le risposte conserva ciò che ha ricevuto secondo le proprie regole. |
| `support.qa.collect.term` | What does Nocturne collect? | Cosa raccoglie Nocturne? |
| `support.qa.collect.detail` | Nothing. No account, no analytics, no crash reports. Only what you turn on can leave your computer or phone. | Niente. Nessun account, nessuna statistica, nessun rapporto sugli arresti anomali. Solo ciò che attivi tu può lasciare il tuo computer o telefono. |
| `support.contact.title` | Write to us. | Scrivici. |
| `support.contact.email` | Something not working, or a word that reads wrong? Email [hello@nocturne.is]. A person reads every message. Say which computer or phone you use and what you saw; never send what you kept, your passphrase or your online service key. | Qualcosa non funziona, o una parola suona male? Scrivi a [hello@nocturne.is]. Ogni messaggio lo legge una persona. Dì quale computer o telefono usi e cosa hai visto; non mandare mai ciò che hai conservato, la tua password o la chiave del tuo servizio online. |
| `support.contact.github` | Or open an issue on [GitHub]. Issues are public, so leave out anything personal. | Oppure apri una segnalazione su [GitHub]. Le segnalazioni sono pubbliche, quindi lascia fuori tutto ciò che è personale. |
| `support.contact.security` | To report a security problem, use [GitHub’s private reporting], not a public issue. | Per segnalare un problema di sicurezza usa [la segnalazione privata di GitHub], non una segnalazione pubblica. |

## Decisions

- «Conserva» for keep: salva sounds like saving a file.
- ◊ rows follow the system's wording and were accepted as written.

## Approval

The owner approved these words on 2026-11-16, with the answers in [the site in five languages](../site-localisation.md#approval).

The journal band, the two reworded Privacy details, the phone picture's description and the meta description were
translated on 2026-11-19 from the English in [the journal on the site](../site-capabilities.md),
with the glossary rows above for the journal.
