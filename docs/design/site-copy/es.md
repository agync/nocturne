---
type: Design
title: Site copy, Español
description: The approved Spanish words for the landing page, the install page and the not-found page, with the glossary they follow.
status: stable
tags: [approved, copy, site, localisation, es]
---

# Site copy, Español

The Spanish words for the pages described in [the site in five languages](../site-localisation.md).
The strings file `site/i18n/es.json` carries the same words; change both together.

## Register and conventions

- Register: Tú, Spanish as written in Spain (ordenador, móvil). Tú is the norm for consumer apps in Spain and Latin America and matches the warm English; usted would sound like a bank. Spain confirmed by the owner.
- Quotes «…», inner “…”. Opening ¿ and ¡ where needed.
- Numbers: 5,2 GB, with a no-break space between number and unit. Dates: viernes 13 de noviembre.
- Keys: `Ctrl Mayús Espacio`, and `Cmd Mayús Espacio` on a Mac.
- Nocturne takes no article; la app is feminine, Nocturne as a subject is left implicit where Spanish allows.
- A plain space before a unit, and in French before `: ; ? !` and inside « », stands for a no-break space; the build writes it.
- `code` marks a key or a command, **bold** a word the system shows, [brackets] the text of a link, *italic* the emphasised half of the hero title.
- ◊ quotes the operating system's own words. L appears only on translated pages.

## Glossary

Each term is rendered the same way on every page.

| English | Español | Why |
|---|---|---|
| keep (verb, button) | guardar; botón «Guardar» | The everyday word for putting something away to find later. |
| Kept. | Guardado. | Short confirmation. |
| ask (verb, button) | preguntar; botón «Preguntar» | Plain. |
| remember | recordar | Plain. |
| sort (as in without sorting) | clasificar | Keeps ordenar free for tidy. |
| the morning page | la página de la mañana | Reads as a name. |
| what needs you | lo que tienes pendiente; lo pendiente | Idiomatic; «lo que te necesita» sounds translated. |
| memories (things kept) | recuerdos | Kept apart from memoria. |
| memory (of a computer) | memoria | As the systems say it. |
| space | espacio | As the systems say it. |
| receipts; the original | pruebas; el original | Plain. |
| private; marked private | privado; marcado como privado | Plain. |
| fade; let go; bring back | difuminarse; apartar; recuperar | Apartar says it is set aside, not thrown away. |
| forget; forget for good | olvidar; olvidar para siempre | Para siempre marks the one final step. |
| tidy; the daily tidy | repasar; el repaso diario | Limpieza would suggest deleting, and ordenar is too close to sorting. |
| Away from home | Fuera de casa | Common phrase, fits a switch. |
| Online answers | Respuestas en línea | Plain. |
| setup (first run) | puesta en marcha | Keeps configuración free for the sizes. |
| setup (size): Large, Standard, Small, Tiny | configuración: Grande, Estándar, Pequeña, Mini | Feminine to agree with configuración. |
| voice note | nota de voz | As on phones. |
| phone | móvil | Spain; Latin America would say celular. |
| computer | ordenador | Spain; Latin America would say computadora. |
| its mind | su cerebro | Plain image. |
| AI chip | chip de IA | Only in Requisitos, as in English. |
| demo data | datos de ejemplo | Plain. |
| journal | diario | The everyday word. |
| entry | entrada | As diary apps say it. |
| check in (verb); a check-in | registrar cómo te sientes; registro de ánimo | Hacer check-in would read as a hotel or an airport. |
| Wound up, Lit up, Low, Settled (the four parts) | Tensión, Entusiasmo, Bajón, Calma | Nouns, so each fits a man or a woman alike. With the English in brackets while the app is English. |
| good thing | cosa buena | Plain. |
| reflection | reflexión | Kept apart from the página de la mañana and from resumen. |
| recap | resumen | Plain; repaso is kept for the daily tidy. |
| streak | racha | As streak counters in Spanish apps say it. |

## Landing page, head and link preview

| Id | English | Español |
|---|---|---|
| `head.title` | Nocturne · Keep everything, ask about it, get one page each morning | Nocturne · Guárdalo todo, pregunta y recibe una página cada mañana |
| `head.description` | Nocturne is a free, private app for your computer. Save notes, photos, voice notes and files without sorting them, now or later. Ask about any of it later, and each morning get one page of what needs you, with the original behind every line. Keep a journal too, if you like. | Nocturne es una app gratuita y privada para tu ordenador. Guarda notas, fotos, notas de voz y archivos sin clasificarlos, ni ahora ni después. Más tarde, pregunta por cualquier cosa, y cada mañana recibe una página con lo que tienes pendiente, con el original detrás de cada línea. Lleva también un diario, si te apetece. |
| `head.og.description` | Save notes, photos, voice notes and files without sorting them, now or later. Ask about any of it later, and each morning get one page of what needs you, all on your own computer. | Guarda notas, fotos, notas de voz y archivos sin clasificarlos, ni ahora ni después. Más tarde, pregunta por cualquier cosa, y cada mañana recibe una página con lo que tienes pendiente, todo en tu propio ordenador. |
| `head.og.image.alt` | Nocturne. Keep everything. Nocturne remembers it for you. Ask about any of it later, and get one page each morning of what needs you. Beside the words, the morning page from the app with demo data. | Nocturne. Guárdalo todo. Nocturne lo recuerda por ti. Más tarde, pregunta por cualquier cosa, y recibe cada mañana una página con lo que tienes pendiente. Junto al texto, la página de la mañana de la app con datos de ejemplo. |

## Header, footer and switches (every page)

| Id | English | Español |
|---|---|---|
| `header.skip` | Skip to content | Saltar al contenido |
| `header.brand.aria` | Nocturne, home | Nocturne, inicio |
| `nav.aria` | Main | Principal |
| `nav.how` | How it works | Cómo funciona |
| `nav.privacy` | Privacy | Privacidad |
| `nav.engine` | Engine | Motor |
| `nav.requirements` | Requirements | Requisitos |
| `nav.download` | Download | Descargar |
| `nav.docs` | Docs | Documentación |
| `theme.aria` | Colour theme | Tema de color |
| `theme.system` | System | Sistema |
| `theme.light` | Light | Claro |
| `theme.dark` | Dark | Oscuro |
| `lang.aria` L | Language | Idioma |
| `docs.inEnglish` L | in English (read by screen readers after a docs link; the visible mark is EN) | en inglés |
| `suggest.text` L | This page is also in German. (Shown on the English page, in the suggested language.) | Esta página también está en español. |
| `suggest.link` L | Read in German | Leer en español |
| `suggest.close` L | Close this note | Cerrar este aviso |
| `foot.brand` | Nocturne · engine by Lemonade · Apache 2.0 | Nocturne · motor de Lemonade · Apache 2.0 |
| `foot.aria` | Footer | Pie de página |
| `foot.docs` | Docs | Documentación |
| `foot.data` | Your data | Tus datos |
| `foot.download` | Download | Descargar |
| `foot.install` | Install guide | Guía de instalación |
| `foot.support` | Support | Ayuda |
| `foot.font` | Font licence | Licencia de las fuentes |

## Hero

| Id | English | Español |
|---|---|---|
| `hero.title` | Keep everything. *Nocturne remembers it for you.* | Guárdalo todo. *Nocturne lo recuerda por ti.* |
| `hero.lede` | Save notes, photos, voice notes and files without sorting them, now or later. Ask about any of it later, and each morning get one page of what needs you, all on your own computer. | Guarda notas, fotos, notas de voz y archivos sin clasificarlos, ni ahora ni después. Más tarde, pregunta por cualquier cosa, y cada mañana recibe una página con lo que tienes pendiente, todo en tu propio ordenador. |
| `hero.download` | Download free | Descargar gratis |
| `hero.how` | See how it works | Ver cómo funciona |
| `hero.facts.aria` | At a glance | De un vistazo |
| `hero.fact.platforms` | Mac, Windows, Linux | Mac, Windows, Linux |
| `hero.fact.offline` | Offline after setup | Sin conexión tras la puesta en marcha |
| `hero.fact.free` | Free and open source | Gratis y de código abierto |
| `hero.page.alt` | The morning page in Nocturne: Friday 13 November, two things need you today. The Year 4 trip form is due next Friday, with the letter it came from. The dentist appointment changed, with a choice between the card and the voice note. | La página de la mañana en Nocturne: viernes 13 de noviembre, hoy tienes dos cosas pendientes. El formulario de la excursión escolar hay que entregarlo el viernes que viene, con la carta de la que sale. La cita del dentista ha cambiado, con la opción de elegir entre la tarjeta de la cita y la nota de voz. |
| `hero.keep.alt` | The keep box with “Gran’s new number ends 4471” typed in, and Keep and Ask buttons. | El cuadro para guardar con un texto escrito: el nuevo número de la abuela termina en 4471, y los botones Guardar y Preguntar. |
| `note.english` L | The app is in English for now. | La app está en inglés por ahora. |
| `note.understanding` L | How well it understands notes and voice in this language depends on the mind it downloads, not on Nocturne itself. | Lo bien que entiende las notas y la voz en español depende del cerebro que descarga, no del propio Nocturne. |
| `hero.caption` | Keep it in one box, read it on the morning page. From the app, with demo data. | Guárdalo en un solo cuadro, léelo en la página de la mañana. De la app, con datos de ejemplo. |

## How it works

| Id | English | Español |
|---|---|---|
| `how.eyebrow` | How it works | Cómo funciona |
| `how.title` | Keep it now. Read it once. | Guárdalo ahora. Léelo una vez. |
| `keep.eyebrow` | Keep · any time | Guardar · cuando quieras |
| `keep.title` | Keep anything, file nothing. | Guárdalo todo, no archives nada. |
| `keep.body` | Press `Ctrl Shift Space`, or `Cmd Shift Space` on a Mac, then type, talk, paste or drop a photo or a PDF. Nocturne says “Kept.” and you get back to your day. No folders or tags to keep up: you ask, and it finds it. If it cannot read something, it tells you what to do. | Pulsa `Ctrl Mayús Espacio`, o `Cmd Mayús Espacio` en un Mac, y escribe, habla, pega o suelta una foto o un PDF. Nocturne dice «Guardado.» (Kept.) y sigues con tu día. Sin carpetas ni etiquetas que mantener: tú preguntas y Nocturne lo encuentra. Si no puede leer algo, te dice qué hacer. |
| `keep.box.alt` | The quick keep box with “Gran’s new number ends 4471” typed in, and Keep and Ask buttons. | El cuadro de guardado rápido con un texto escrito: el nuevo número de la abuela termina en 4471, y los botones Guardar y Preguntar. |
| `keep.recent.alt` | Recently kept: a photo still being read, voice notes and pasted messages marked understood, a password-locked PDF Nocturne could not read with what to do about it, and a voice note that needs you. | Guardado hace poco: una foto que aún se está leyendo, notas de voz y mensajes pegados marcados como entendidos, un PDF protegido con contraseña que Nocturne no pudo leer, con lo que hay que hacer, y una nota de voz pendiente. |
| `ask.eyebrow` | Ask · any time | Preguntar · cuando quieras |
| `ask.title` | Answers with receipts. | Respuestas con pruebas. |
| `ask.body` | Ask in plain words. Nocturne answers only from what it can show you, with the originals underneath so you can check. When it does not know, it says so. If it gets something wrong, tell it what is right and it remembers. | Pregunta con palabras sencillas. Nocturne responde solo a partir de lo que puede mostrarte, con los originales debajo para que puedas comprobarlo. Cuando no lo sabe, lo dice. Si se equivoca en algo, dile qué es lo correcto y lo recordará. |
| `ask.alt` | An answer to “What does Leo need for his trip?”: a packed lunch, and the trip form is due next Friday with a signature and £12. Below it, the two photographed letters it came from. | Una respuesta a «¿Qué necesita Leo para su excursión?»: un almuerzo para llevar, y el formulario de la excursión hay que entregarlo el viernes que viene con una firma y 12 libras. Debajo, las dos cartas fotografiadas de las que sale la respuesta. |
| `page.eyebrow` | The morning page · at your time | La página de la mañana · a tu hora |
| `page.title` | Wake to what needs you. | Despierta con lo pendiente. |
| `page.body` | Whenever the computer is plugged in and idle, by night or by day, Nocturne reads what you kept and notices what changed. At the time you choose, you get a short list to read, hear or print. When two things disagree, it asks instead of guessing. | Siempre que el ordenador está enchufado y sin uso, de noche o de día, Nocturne lee lo que guardaste y se fija en lo que ha cambiado. A la hora que elijas, recibes una lista breve para leer, escuchar o imprimir. Cuando dos cosas no coinciden, pregunta en lugar de adivinar. |
| `page.alt` | The morning page: two things need you today. The Year 4 trip form is due next Friday, with the letter it came from. The dentist appointment changed, with a choice between the appointment card and the voice note. | La página de la mañana: hoy tienes dos cosas pendientes. El formulario de la excursión escolar hay que entregarlo el viernes que viene, con la carta de la que sale. La cita del dentista ha cambiado, con la opción de elegir entre la tarjeta de la cita y la nota de voz. |

## The journal

| Id | English | Español |
|---|---|---|
| `journal.eyebrow` | The journal · when you want to | El diario · cuando te apetezca |
| `journal.title` | Your day, in your own words. | Tu día, con tus propias palabras. |
| `journal.body` | Everything you keep during the day is already in your journal, so it starts half written. Add a few lines, a voice note or a photo, and say how you felt. Nocturne looks back on each day, and on each week, month and year in a recap. Every line points to what it came from. | Todo lo que guardas durante el día ya está en tu diario, así que empieza medio escrito. Añade unas líneas, una nota de voz o una foto, y di cómo te has sentido. Nocturne echa la vista atrás a cada día, y a cada semana, mes y año en un resumen. Cada línea remite a aquello de lo que sale. |
| `journal.felt.term` | How you felt, never guessed | Cómo te sentiste, sin suposiciones |
| `journal.felt.detail` | Check in as Wound up, Lit up, Low or Settled, and choose the words that fit. A reflection quotes what you said you felt, and never scores the day or gives advice. | Registra cómo te sientes: Tensión (Wound up), Entusiasmo (Lit up), Bajón (Low) o Calma (Settled), y elige las palabras que encajen. Una reflexión cita lo que dijiste que sentías, y nunca puntúa el día ni da consejos. |
| `journal.good.term` | Good things | Cosas buenas |
| `journal.good.detail` | Up to three a day, with a photo if you like. Your recaps bring them back. | Hasta tres al día, con una foto si quieres. Tus resúmenes las traen de vuelta. |
| `journal.calm.term` | No streaks, no nagging | Sin rachas ni insistencias |
| `journal.calm.detail` | Nothing counts the days you write or says you missed one. A reminder comes only if you turn one on. When a reflection or recap is new, the morning page says so in one line. | Nada cuenta los días que escribes ni te dice que te has saltado uno. Solo recibes un recordatorio si lo activas. Cuando hay una reflexión o un resumen nuevo, la página de la mañana lo dice en una línea. |
| `journal.quiet.term` | It stays quiet | Se queda en silencio |
| `journal.quiet.detail` | Nothing from your journal is read aloud or shown in a notification or on a lock screen. If you turn on Online answers, a line from your journal or a check-in can go with a question it helps answer. Your reflections and recaps never do. | Nada de tu diario se lee en voz alta ni aparece en una notificación o en la pantalla de bloqueo. Si activas Respuestas en línea (Online answers), una línea de tu diario o un registro de ánimo puede acompañar a una pregunta si ayuda a responderla. Tus reflexiones y resúmenes, nunca. |
| `journal.day.alt` | Today in the journal, Thursday 12 November: an entry about lunch with Priya, with a voice note and a photo. Beside it, how today felt, with three check-ins (tired, pleased, relieved and content). | Hoy en el diario, jueves 12 de noviembre: una entrada sobre la comida con Priya, con una nota de voz y una foto. Al lado, cómo te has sentido hoy, con tres registros de ánimo (cansancio, alegría, alivio y satisfacción). |
| `journal.look.alt` | The day, looked back on: a short paragraph about Leo forgetting his PE kit, a tired morning and lunch with Priya at last, with the notes, check-ins, document and entry it came from listed underneath. | El día, visto en retrospectiva: un párrafo breve sobre Leo, que olvidó la ropa de deporte, una mañana de cansancio y por fin la comida con Priya, con debajo las notas, los registros de ánimo, el documento y la entrada de los que sale. |
| `journal.caption` | Today’s journal, and the day looked back on. From the app, with demo data. | El diario de hoy, y el día visto en retrospectiva. De la app, con datos de ejemplo. |

## What it promises

| Id | English | Español |
|---|---|---|
| `privacy.eyebrow` | What it promises | Lo que promete |
| `privacy.title` | Your life stays on your computer. | Tu vida se queda en tu ordenador. |
| `privacy.body` | Out of the box, Nocturne goes online only for downloads you agree to: its mind at setup and any update you allow. Privacy in the app lists each one with the day and the size. | De serie, Nocturne solo se conecta para las descargas que aceptas: su cerebro en la puesta en marcha y cada actualización que permitas. Privacidad (Privacy), en la app, las muestra una a una con el día y el tamaño. |
| `promise.signin.term` | Nothing to sign in to | Nada en lo que iniciar sesión |
| `promise.signin.detail` | No account, no analytics, no crash reports, no cloud copy. To keep a copy somewhere safe, back everything up to one file, locked with a passphrase you choose. | Sin cuenta, sin analíticas, sin informes de errores, sin copia en la nube. Para tenerlo todo a salvo en otro sitio, haz una copia de seguridad en un solo archivo, protegida con una contraseña que tú eliges. |
| `promise.checked.term` | Checked, not just claimed | Comprobado, no solo prometido |
| `promise.checked.detail` | Before a release, Nocturne is run with its network cut off from everything but this computer, and any attempt to reach further fails the check. | Antes de una versión, Nocturne se ejecuta con la red cortada para todo menos este ordenador, y cualquier intento de llegar más allá hace fallar la prueba. |
| `promise.edit.term` | It never edits what you kept | Nunca cambia lo que guardaste |
| `promise.edit.detail` | Your notes, photos and files are never changed. Every answer shows them underneath. If one is wrong, tell it what is right; the old version is kept, not overwritten. | Tus notas, fotos y archivos nunca se modifican. Cada respuesta los muestra debajo. Si una está mal, dile qué es lo correcto; la versión anterior se conserva, no se sobrescribe. |
| `promise.forget.term` | It forgets on purpose | Olvida a propósito |
| `promise.forget.detail` | Things you no longer use fade, and the page says when one is let go. Bring any back; forgetting for good always asks first. | Lo que ya no usas se difumina, y la página avisa cuando algo se aparta. Puedes recuperar cualquier cosa; para olvidar algo para siempre, siempre pregunta antes. |
| `promise.note.term` | One quiet note a day | Un solo aviso tranquilo al día |
| `promise.note.detail` | When the page is ready, one note says so and how many things need you. Never what you kept. | Cuando la página está lista, un aviso lo dice y cuántas cosas tienes pendientes. Nunca lo que guardaste. |
| `promise.extras.term` | Two extras, off until you ask | Dos extras, apagados hasta que los pidas |
| `promise.extras.detail` | Away from home lets your own phone ask, keep and read your page and journal, sealed so the company in between cannot read it. Online answers sends a question, and the few remembered sentences it needs, to one service you choose. They can include lines from your journal, your check-ins and things you marked private, but never your reflections or recaps. Marking something private keeps it out of notifications, reminders and the lock screen, but it still helps answer what you ask. | Fuera de casa (Away from home) deja que tu propio móvil pregunte, guarde y lea tu página y tu diario, sellado para que la empresa que hay en medio no pueda leerlo. Respuestas en línea (Online answers) envía una pregunta, y las pocas frases recordadas que necesita, a un único servicio que tú eliges. Pueden incluir líneas de tu diario, tus registros de ánimo y cosas marcadas como privadas, pero nunca tus reflexiones ni resúmenes. Marcar algo como privado lo mantiene fuera de las notificaciones, los recordatorios y la pantalla de bloqueo, pero sigue ayudando a responder lo que preguntas. |
| `phone.alt` | Nocturne on a paired phone: connected to the home computer, with Ask, Keep, today’s page and the journal, and a note that things marked private stay on the computer. | Nocturne en un móvil vinculado: conectado al ordenador de casa, con Preguntar, Guardar, la página de hoy y el diario, y una nota que dice que lo marcado como privado se queda en el ordenador. |
| `phone.caption` | Away from home on a paired phone, with demo data. | Fuera de casa en un móvil vinculado, con datos de ejemplo. |

## The engine

| Id | English | Español |
|---|---|---|
| `engine.eyebrow` | The engine | El motor |
| `engine.title` | Built on (followed by the Lemonade logo, alt text “Lemonade”) | Construido sobre (texto alternativo del logo «Lemonade») |
| `engine.body1` | Nocturne’s thinking runs on **Embeddable Lemonade**, the portable build of [Lemonade], the open-source local AI server. Nocturne ships its own copy, Lemonade {{LEMONADE_VERSION}}, that only this computer can reach. | El pensamiento de Nocturne funciona con **Embeddable Lemonade**, la versión portátil de [Lemonade], el servidor de IA local y de código abierto. Nocturne incluye su propia copia, Lemonade {{LEMONADE_VERSION}}, a la que solo puede llegar este ordenador. |
| `engine.body2` | Lemonade is free under the Apache 2.0 licence and built by its community, with optimisations by AMD engineers. Nocturne would not exist without it. | Lemonade es gratuito con la licencia Apache 2.0 y lo desarrolla su comunidad, con optimizaciones de ingenieros de AMD. Nocturne no existiría sin él. |

## Requirements (the install page repeats these)

| Id | English | Español |
|---|---|---|
| `req.eyebrow` | Requirements | Requisitos |
| `req.title` | Will it run on my computer? | ¿Funcionará en mi ordenador? |
| `req.body` | Almost certainly, if it has 8 GB of memory: an 8 GB MacBook Air, or a typical 8 GB Windows or Linux laptop. | Casi seguro, si tiene 8 GB de memoria: un MacBook Air de 8 GB, o un portátil normal con Windows o Linux y 8 GB. |
| `req.memory.term` | Memory | Memoria |
| `req.memory.detail` | 8 GB is enough. On an 8 GB computer Nocturne uses about 5 GB at its busiest and gives it back when it has nothing to do. With more memory it offers a larger setup with clearer answers. | Con 8 GB basta. En un ordenador de 8 GB, Nocturne usa unos 5 GB en sus momentos de más trabajo y los devuelve cuando no tiene nada que hacer. Con más memoria, ofrece una configuración mayor con respuestas más claras. |
| `req.graphics.term` | A graphics card | Una tarjeta gráfica |
| `req.graphics.detail` | No graphics card needed. Macs with Apple silicon, and computers with AMD or Intel graphics built in, run Nocturne on the chip and share the computer’s memory. An AMD or NVIDIA graphics card works too, best with 6 GB or more. Other computers run it on the processor. | No hace falta tarjeta gráfica. Los Mac con chip de Apple, y los ordenadores con gráficos AMD o Intel integrados, ejecutan Nocturne en el chip y comparten la memoria del ordenador. Una tarjeta gráfica AMD o NVIDIA también sirve, mejor con 6 GB o más. Los demás ordenadores lo ejecutan en el procesador. |
| `req.chip.term` | An AI chip | Un chip de IA |
| `req.chip.detail` | Helpful, never needed. On AMD Ryzen AI 300 series computers and newer, with 16 GB or more, the AI chip reads your voice notes. On many with 32 GB or more it also reads what you keep, and on some it answers your questions too, so the rest of the computer has less to do. | Útil, nunca necesario. En ordenadores AMD Ryzen AI serie 300 o posteriores, con 16 GB o más, el chip de IA lee tus notas de voz. En muchos con 32 GB o más también lee lo que guardas, y en algunos también responde a tus preguntas, así el resto del ordenador tiene menos trabajo. |
| `req.system.term` | The system | El sistema |
| `req.system.detail` | macOS 26 or later on an Apple chip, Windows 10 or 11 (64-bit), or 64-bit Linux on Intel, AMD or Arm. | macOS 26 o posterior en un chip de Apple, Windows 10 u 11 (64 bits), o Linux de 64 bits en Intel, AMD o Arm. |
| `tier.large.name` | Large | Grande |
| `tier.large.spec` | 60 GB+ memory · 28 GB space | 60 GB de memoria o más · 28 GB de espacio |
| `tier.large.text` | Clearer answers to harder questions. | Respuestas más claras a preguntas más difíciles. |
| `tier.standard.name` | Standard | Estándar |
| `tier.standard.spec` | 28 GB+ memory · 11 GB space | 28 GB de memoria o más · 11 GB de espacio |
| `tier.standard.text` | Many recent laptops with 32 GB. | Muchos portátiles recientes con 32 GB. |
| `tier.small.name` | Small | Pequeña |
| `tier.small.spec` | 14 GB+ memory · 7 GB space | 14 GB de memoria o más · 7 GB de espacio |
| `tier.small.text` | Same page. The daily tidy takes longer. | La misma página. El repaso diario tarda más. |
| `tier.tiny.name` | Tiny | Mini |
| `tier.tiny.spec` | 6 GB+ memory · 5.2 GB space | 6 GB de memoria o más · 5,2 GB de espacio |
| `tier.tiny.text` | Made for 8 GB computers. Simpler answers that may miss details. | Pensada para ordenadores de 8 GB. Respuestas más sencillas que pueden pasar por alto detalles. |
| `req.downloads` | Downloads at setup are between about 3 GB and 26 GB, depending on the setup, and about 9 GB more where the AI chip reads what you keep. After that, Nocturne works with no internet at all. | Las descargas de la puesta en marcha ocupan entre unos 3 GB y 26 GB, según la configuración, y unos 9 GB más donde el chip de IA lee lo que guardas. Después, Nocturne funciona sin conexión a internet. |

## Get Nocturne (end of the landing page)

| Id | English | Español |
|---|---|---|
| `get.title` | Get Nocturne. | Consigue Nocturne. |
| `get.body` | Free and open source, for Mac, Windows and Linux. Download it, open it, and it sets itself up for the computer it is on. | Gratis y de código abierto, para Mac, Windows y Linux. Descárgalo, ábrelo y se prepara solo para el ordenador en el que está. |
| `get.download` | Download | Descargar |
| `get.source` | Build it from source | Compilarlo desde el código fuente |

## Not-found page

| Id | English | Español |
|---|---|---|
| `missing.title` | Not here · Nocturne | No está aquí · Nocturne |
| `missing.eyebrow` | Not found | No encontrada |
| `missing.heading` | There is no page here. | Aquí no hay ninguna página. |
| `missing.body` | The link may be old, or the page may have moved. | Puede que el enlace sea antiguo o que la página se haya movido. |
| `missing.links` | [Go to the front page] or [read the docs]. | [Ve a la portada] o [lee la documentación]. |

## Install page, head and top

| Id | English | Español |
|---|---|---|
| `install.title` | Get Nocturne · Download for Mac, Windows and Linux | Consigue Nocturne · Descarga para Mac, Windows y Linux |
| `install.description` | Download Nocturne for a Mac with an Apple chip, a Windows 10 or 11 computer, or a Linux desktop. Free and open source, with no account. The first time it opens, it sets itself up for the computer it is on. | Descarga Nocturne para un Mac con chip de Apple, un ordenador con Windows 10 u 11, o un escritorio Linux. Gratis y de código abierto, sin cuenta. La primera vez que se abre, se prepara solo para el ordenador en el que está. |
| `install.og.description` | Free and open source, with no account. The first time it opens, Nocturne sets itself up for the computer it is on. | Gratis y de código abierto, sin cuenta. La primera vez que se abre, Nocturne se prepara solo para el ordenador en el que está. |
| `install.heading` | Get Nocturne. | Consigue Nocturne. |
| `install.lede` | Free and open source, for Mac, Windows and Linux. No account, nothing to sign in to. | Gratis y de código abierto, para Mac, Windows y Linux. Sin cuenta, sin iniciar sesión. |
| `install.here` | Choose your computer below. Each link gives the latest version. | Elige tu ordenador aquí abajo. Cada enlace lleva a la versión más reciente. |
| `install.hereTag` | For this computer | Para este ordenador |
| `install.firstOpen` | The first time you open it | La primera vez que lo abras |
| `install.others` | Other computers | Otros ordenadores |

## Install page, Mac

| Id | English | Español |
|---|---|---|
| `mac.spec` | Apple chip, M1 or later · macOS 26 or later | Chip de Apple, M1 o posterior · macOS 26 o posterior |
| `mac.download` | Download for Mac | Descargar para Mac |
| `mac.note` ◊ | Macs with an Intel chip are not supported. To see which chip yours has, open the Apple menu and choose About This Mac. | Los Mac con chip Intel no son compatibles. Para ver qué chip tiene el tuyo, abre el menú Apple y elige Acerca de este Mac. |
| `mac.step1` ◊ | Open the downloaded file, then drag Nocturne into Applications. | Abre el archivo descargado y arrastra Nocturne a Aplicaciones. |
| `mac.step2` ◊ | Open Nocturne from Applications. Your Mac says “Nocturne” Not Opened, because it could not check the app. Click **Done**. | Abre Nocturne desde Aplicaciones. Tu Mac dice que no se ha abierto «Nocturne», porque no ha podido comprobar la app. Haz clic en **OK**. |
| `mac.step3` ◊ | Open System Settings, then **Privacy & Security**. Scroll down to the line saying Nocturne was blocked and click **Open Anyway**. | Abre Ajustes del Sistema y luego **Privacidad y seguridad**. Baja hasta la línea que dice que Nocturne se ha bloqueado y haz clic en **Abrir igualmente**. |
| `mac.step4` ◊ | Enter your password, then click **Open Anyway** again. | Escribe tu contraseña y vuelve a hacer clic en **Abrir igualmente**. |
| `mac.why` | You only do this once. Apple charges a yearly fee to sign an app so it opens without asking; Nocturne is free and open source, and this release is not signed yet. | Solo lo haces una vez. Apple cobra una cuota anual por firmar una app para que se abra sin preguntar; Nocturne es gratis y de código abierto, y esta versión aún no está firmada. |

## Install page, Windows

| Id | English | Español |
|---|---|---|
| `win.spec` | Windows 10 or 11, 64-bit | Windows 10 u 11, 64 bits |
| `win.download` | Download for Windows | Descargar para Windows |
| `win.note` | Installs for you alone and needs no administrator password. Computers managed by an organisation can use the [.msi installer] instead. | Se instala solo para ti y no necesita contraseña de administrador. Los ordenadores gestionados por una organización pueden usar el [instalador .msi] en su lugar. |
| `win.step1` ◊ | Your browser may warn that the file is not commonly downloaded. In Edge, open the **…** menu beside the file, choose **Keep**, then **Show more**, then **Keep anyway**. | Puede que tu navegador avise de que el archivo no se descarga con frecuencia. En Edge, abre el menú **…** junto al archivo, elige **Conservar**, luego **Mostrar más** y luego **Conservar de todos modos**. |
| `win.step2` ◊ | Open the installer. Windows shows “Windows protected your PC”. Click **More info**, then **Run anyway**. | Abre el instalador. Windows muestra «Windows protegió su PC». Haz clic en **Más información** y luego en **Ejecutar de todas formas**. |
| `win.why` | You only do this once. Microsoft charges a yearly fee to sign an app so it opens without asking; Nocturne is free and open source, and this release is not signed yet. | Solo lo haces una vez. Microsoft cobra una cuota anual por firmar una app para que se abra sin preguntar; Nocturne es gratis y de código abierto, y esta versión aún no está firmada. |

## Install page, Linux

| Id | English | Español |
|---|---|---|
| `linux.spec` | 64-bit computer with an Intel, AMD or Arm chip | Ordenador de 64 bits con chip Intel, AMD o Arm |
| `linux.choices.aria` | Ways to install on Linux | Formas de instalarlo en Linux |
| `linux.flatpak.title` | From your software centre | Desde tu centro de software |
| `linux.flatpak.text` | The choice for most people, on any Linux desktop. Your software centre keeps Nocturne up to date. | La opción para la mayoría, en cualquier escritorio Linux. Tu centro de software mantiene Nocturne al día. |
| `linux.flatpak.button` | Get the Flatpak | Obtener el Flatpak |
| `linux.flatpak.terminal` | Or install it from a terminal | O instálalo desde un terminal |
| `linux.deb.title` | Ubuntu, Linux Mint or Debian | Ubuntu, Linux Mint o Debian |
| `linux.package.text` | Open the file with your software centre, and Nocturne appears with your other apps. (Shared by .deb and .rpm.) | Abre el archivo con tu centro de software y Nocturne aparecerá con tus otras apps. |
| `linux.deb.intel` | .deb for Intel or AMD | .deb para Intel o AMD |
| `linux.deb.arm` | .deb for Arm | .deb para Arm |
| `linux.rpm.title` | Fedora, Red Hat or openSUSE | Fedora, Red Hat u openSUSE |
| `linux.rpm.intel` | .rpm for Intel or AMD | .rpm para Intel o AMD |
| `linux.rpm.arm` | .rpm for Arm | .rpm para Arm |
| `linux.appimage.title` | Try it without installing | Pruébalo sin instalarlo |
| `linux.appimage.text` | One file that runs on most desktops. In its properties, allow it to run as a program, then open it. | Un solo archivo que funciona en la mayoría de los escritorios. En sus propiedades, permite que se ejecute como programa y luego ábrelo. |
| `linux.appimage.intel` | AppImage for Intel or AMD | AppImage para Intel o AMD |
| `linux.appimage.arm` | AppImage for Arm | AppImage para Arm |
| `linux.note` | Most Linux computers have an Intel or AMD chip. To check, run `uname -m` in a terminal: `x86_64` means Intel or AMD, `aarch64` means Arm. The Flatpak picks the right one by itself. | La mayoría de los ordenadores con Linux tienen un chip Intel o AMD. Para comprobarlo, ejecuta `uname -m` en un terminal: `x86_64` significa Intel o AMD, `aarch64` significa Arm. El Flatpak elige el correcto por sí solo. |
| `linux.why` | Linux shows no warning the first time you open Nocturne. | Linux no muestra ningún aviso la primera vez que abres Nocturne. |

## Install page, checks, needs and after

| Id | English | Español |
|---|---|---|
| `install.checks` | To make sure a download is the one we published, compare it with the fingerprints in [SHA256SUMS]. The [install guide] shows how, explains each package and how to take Nocturne off again. Earlier versions are on the [releases page]. | Para asegurarte de que una descarga es la que publicamos, compárala con las huellas de [SHA256SUMS]. La [guía de instalación] explica cómo hacerlo, describe cada paquete y cómo quitar Nocturne. Las versiones anteriores están en la [página de versiones]. |
| `needs.title` | What your computer needs. (The rest of this section is the Requirements rows above.) | Lo que necesita tu ordenador. |
| `after.title` | After you install. | Después de instalar. |
| `after.body` | Nocturne opens a short setup. You can keep things straight away. | Nocturne abre una breve puesta en marcha. Puedes guardar cosas desde el primer momento. |
| `after.1.when` | When it opens | Al abrirse |
| `after.1.title` | It looks at the computer | Revisa el ordenador |
| `after.1.text` | It tells you which setup it will use. You can always pick a smaller one, which uses less memory, space and battery. | Te dice qué configuración va a usar. Siempre puedes elegir una más pequeña, que usa menos memoria, espacio y batería. |
| `after.2.when` | Once, when you agree | Una vez, si aceptas |
| `after.2.title` | It downloads what it needs | Descarga lo que necesita |
| `after.2.text` | It asks first, then downloads what this computer can run: between about 3 GB and 26 GB, and about 9 GB more where the AI chip reads what you keep. This is the only time Nocturne needs the internet, and it carries on by itself if the connection drops. | Primero pregunta y luego descarga lo que este ordenador puede ejecutar: entre unos 3 GB y 26 GB, y unos 9 GB más donde el chip de IA lee lo que guardas. Es la única vez que Nocturne necesita internet, y sigue por sí solo si se corta la conexión. |
| `after.3.when` | At the time you choose | A la hora que elijas |
| `after.3.title` | Your first page | Tu primera página |
| `after.3.text` | From then on, Nocturne works with no internet at all. | A partir de ahí, Nocturne funciona sin conexión a internet. |

## Install page, messages from install.js

| Id | English | Español |
|---|---|---|
| `js.mac` | This looks like a Mac, so its download comes first. Nocturne needs a Mac with an Apple chip. | Parece un Mac, así que su descarga va primero. Nocturne necesita un Mac con chip de Apple. |
| `js.macArm` | This looks like a Mac with an Apple chip, so its download comes first. | Parece un Mac con chip de Apple, así que su descarga va primero. |
| `js.macIntel` ◊ | This looks like a Mac with an Intel chip. Nocturne needs a Mac with an Apple chip (M1 or later), so it will not run on this one. About This Mac, in the Apple menu, shows which chip it has. | Parece un Mac con chip Intel. Nocturne necesita un Mac con chip de Apple (M1 o posterior), así que no funcionará en este. Acerca de este Mac, en el menú Apple, muestra qué chip tiene. |
| `js.windows` | This looks like a Windows computer, so its download comes first. | Parece un ordenador con Windows, así que su descarga va primero. |
| `js.linux` | This looks like a Linux computer, so its choices come first. | Parece un ordenador con Linux, así que sus opciones van primero. |
| `js.linuxArm` | This looks like a Linux computer with an Arm chip, so its choices come first, Arm downloads first. | Parece un ordenador con Linux y chip Arm, así que sus opciones van primero, con las descargas para Arm delante. |
| `js.other` | Nocturne is a desktop app, for Mac, Windows and Linux computers. Open this page on the computer you want to use it on, or choose one below. | Nocturne es una app de escritorio para ordenadores Mac, Windows y Linux. Abre esta página en el ordenador donde quieras usarla, o elige uno aquí abajo. |

## Support page

Drafted with DeepL on 2026-11-24 (with the glossary's form of address) and reviewed against the glossary and the neighbouring strings the same day. The `req.chip.detail`, `req.downloads`, `linux.flatpak.text`, `after.2.text`, `after.3.when` and `foot.support` rows below and above went through the same pass. Words the app shows are bold, with the English in brackets while the app is English.

| Id | English | Español |
|---|---|---|
| `support.title` | Support · Nocturne | Ayuda · Nocturne |
| `support.description` | Help with Nocturne on a computer or a phone: answers from its own guide, what to try first, and how to reach the people who make it. | Ayuda con Nocturne en el ordenador o el móvil: respuestas de su propia guía, qué probar primero y cómo hablar con las personas que lo hacen. |
| `support.heading` | Help with Nocturne. | Ayuda con Nocturne. |
| `support.lede` | Nocturne keeps everything on your own computer or phone, so no help desk can see your memory. Help starts with Nocturne itself and its guide, and the people who make it are a message away. | Nocturne lo guarda todo en tu propio ordenador o móvil, así que ningún servicio de ayuda puede ver tus recuerdos. La ayuda empieza en el propio Nocturne y su guía, y las personas que lo hacen están a un mensaje de distancia. |
| `support.first.title` | Start here. | Empieza por aquí. |
| `support.ask.title` | Ask Nocturne | Pregunta a Nocturne |
| `support.ask.text` | On a computer, type your question in the ask box, like “how do I add a phone”. Nocturne answers from the guide that comes with it and shows the part it used. | En el ordenador, escribe tu pregunta en el cuadro de preguntas, por ejemplo «cómo añado un móvil». Nocturne responde con la guía que trae consigo y te enseña la parte que ha usado. |
| `support.phone.title` | On your phone | En tu móvil |
| `support.phone.text` | Open **This phone**, then **Help**: what this phone can do, its permissions with a way to change them, and the privacy policy. | Abre **Este móvil** (This phone) y luego **Ayuda** (Help): lo que puede hacer este móvil, sus permisos con una forma de cambiarlos y la política de privacidad. |
| `support.guide.title` | Read the guide | Lee la guía |
| `support.guide.text` | [Installing], [your computer’s desktop], [Away from home], [Online answers] and [your data]. | [Instalación], [el escritorio de tu ordenador], [Fuera de casa], [Respuestas en línea] y [tus datos]. |
| `support.qa.title` | Common questions. | Preguntas frecuentes. |
| `support.qa.page.term` | My page didn’t come | Mi página no ha llegado |
| `support.qa.page.detail` | Nocturne writes it in the hours before your page time, while the computer is on, plugged in and not in use. If it was off or asleep, Nocturne catches up when it wakes. [The guide] explains each desktop. | Nocturne la escribe en las horas anteriores a la hora de tu página, mientras el ordenador está encendido, enchufado y sin uso. Si estaba apagado o suspendido, Nocturne se pone al día al despertar. [La guía] explica cada escritorio. |
| `support.qa.lost.term` | I lost my phone | He perdido el móvil |
| `support.qa.lost.detail` | If it was paired with your computer, your memory is on the computer: open **Away from home** there, choose **See phones**, then **Remove** beside it. If the phone kept your memory on its own, install Nocturne on the new phone and choose **I have a backup**. | Si estaba emparejado con tu ordenador, tus recuerdos están en el ordenador: abre allí **Fuera de casa** (Away from home), elige **Ver móviles** (See phones) y luego **Quitar** (Remove) junto a él. Si el móvil guardaba tus recuerdos por su cuenta, instala Nocturne en el móvil nuevo y elige **Tengo una copia** (I have a backup). |
| `support.qa.passphrase.term` | I forgot my passphrase | He olvidado mi contraseña |
| `support.qa.passphrase.detail` | No one can open a locked backup without its passphrase or recovery key, not even the people who make Nocturne. Make a new backup with a passphrase you can keep. | Nadie puede abrir una copia de seguridad bloqueada sin su contraseña o clave de recuperación, ni siquiera las personas que hacen Nocturne. Haz una copia nueva con una contraseña que puedas guardar. |
| `support.qa.delete.term` | How do I delete everything? | ¿Cómo borro todo? |
| `support.qa.delete.detail` | Choose **Delete everything…** in **Privacy** on a computer, or in **This phone** on a phone that keeps your memory itself. Nocturne’s makers hold nothing of yours, so there is nothing else to ask for. A service you chose for answers keeps what it received under its own rules. | Elige **Borrar todo…** (Delete everything…) en **Privacidad** (Privacy) en un ordenador, o en **Este móvil** (This phone) en un móvil que guarda tus recuerdos por sí mismo. Quienes hacen Nocturne no conservan nada tuyo, así que no hay nada más que pedir. Un servicio que elegiste para las respuestas guarda lo que recibió según sus propias reglas. |
| `support.qa.collect.term` | What does Nocturne collect? | ¿Qué recoge Nocturne? |
| `support.qa.collect.detail` | Nothing. No account, no analytics, no crash reports. Only what you turn on can leave your computer or phone. | Nada. Ni cuenta, ni analíticas, ni informes de errores. Solo lo que tú activas puede salir de tu ordenador o móvil. |
| `support.contact.title` | Write to us. | Escríbenos. |
| `support.contact.email` | Something not working, or a word that reads wrong? Email [hello@nocturne.is]. A person reads every message. Say which computer or phone you use and what you saw; never send what you kept, your passphrase or your online service key. | ¿Algo no funciona, o hay una palabra que suena mal? Escribe a [hello@nocturne.is]. Cada mensaje lo lee una persona. Dinos qué ordenador o móvil usas y qué viste; nunca envíes lo que guardaste, tu contraseña ni la clave de tu servicio en línea. |
| `support.contact.github` | Or open an issue on [GitHub]. Issues are public, so leave out anything personal. | O abre una incidencia en [GitHub]. Las incidencias son públicas, así que no incluyas nada personal. |
| `support.contact.security` | To report a security problem, use [GitHub’s private reporting], not a public issue. | Para informar de un problema de seguridad, usa [el aviso privado de GitHub], no una incidencia pública. |

## Decisions

- Spanish as written in Spain (ordenador, móvil), confirmed by the owner.
- «Repaso diario» for the daily tidy: limpieza would suggest deleting, ordenar is too close to sorting.
- ◊ rows follow the system's wording and were accepted as written; the macOS dialog button is written as OK.

## Approval

The owner approved these words on 2026-11-16, with the answers in [the site in five languages](../site-localisation.md#approval).

The journal band, the two reworded Privacy details, the phone picture's description and the meta description were
translated on 2026-11-19 from the English in [the journal on the site](../site-capabilities.md),
with the glossary rows above for the journal.
