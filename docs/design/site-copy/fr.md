---
type: Design
title: Site copy, Français
description: The approved French words for the landing page, the install page and the not-found page, with the glossary they follow.
status: stable
tags: [approved, copy, site, localisation, fr]
---

# Site copy, Français

The French words for the pages described in [the site in five languages](../site-localisation.md).
The strings file `site/i18n/fr.json` carries the same words; change both together.

## Register and conventions

- Register: Vous. French consumer software and banks, health and privacy services address people as vous; tu would read as a young brand and weaken the privacy promises. Warmth comes from short sentences, not from tu.
- Quotes « … » with no-break spaces inside; a no-break space before : ; ? !. Apostrophe ’.
- Units in French form: 8 Go, 5,2 Go (gigaoctet), with a no-break space. Dates: vendredi 13 novembre.
- Keys: `Ctrl Maj Espace`, and `Cmd Maj Espace` on a Mac.
- Nocturne takes no article and is masculine (il), as an app name usually is in speech.
- A plain space before a unit, and in French before `: ; ? !` and inside « », stands for a no-break space; the build writes it.
- `code` marks a key or a command, **bold** a word the system shows, [brackets] the text of a link, *italic* the emphasised half of the hero title.
- ◊ quotes the operating system's own words. L appears only on translated pages.

## Glossary

Each term is rendered the same way on every page.

| English | Français | Why |
|---|---|---|
| keep (verb, button) | garder; bouton « Garder » | Everyday, warm; enregistrer would sound like saving a file. |
| Kept. | Gardé. | Short confirmation, same rhythm. |
| ask (verb, button) | demander, poser une question; bouton « Demander » | Plain. |
| remember | se souvenir, retenir | Se souvenir for Nocturne; retenir for the few remembered sentences. |
| the morning page | la page du matin | Reads as a name. |
| what needs you | ce qui vous attend | Idiomatic; « ce qui a besoin de vous » sounds translated. |
| memories (things kept) | souvenirs | Kept apart from mémoire. |
| memory (of a computer) | mémoire | As macOS and Windows say it. |
| space | espace de stockage, espace | As the systems say it. |
| receipts; the original | preuves à l’appui; l’original | A natural idiom for answers that show their source. |
| private; marked private | privé; marqué comme privé | Plain. |
| fade; let go; bring back | s’estomper; mettre de côté; récupérer | Mettre de côté says it can be taken back. |
| forget; forget for good | oublier; oublier pour de bon | Pour de bon marks the one final step. |
| tidy; the daily tidy | ranger; le rangement quotidien | Household word. |
| Away from home | En déplacement | Common phrase, fits a switch. |
| Online answers | Réponses en ligne | Literal and plain. |
| setup (first run) | mise en route | Keeps configuration free for the sizes. |
| setup (size): Large, Standard, Small, Tiny | configuration: Grande, Standard, Petite, Mini | Feminine to agree with configuration. |
| Requirements | Prérequis | Configuration requise would clash with the sizes. |
| voice note | note vocale | As on phones. |
| its mind | son cerveau | Plain image, no jargon. |
| AI chip | puce d’IA | Only in Prérequis, as in English. |
| demo data | données d’exemple | Plain. |

## Landing page, head and link preview

| Id | English | Français |
|---|---|---|
| `head.title` | Nocturne · Keep everything, ask about it, get one page each morning | Nocturne · Gardez tout, posez vos questions, recevez une page chaque matin |
| `head.description` | Nocturne is a free, private app for your computer. Save notes, photos, voice notes and files without sorting them, now or later. Ask about any of it later, and each morning get one page of what needs you, with the original behind every line. | Nocturne est une application gratuite et privée pour votre ordinateur. Gardez notes, photos, notes vocales et fichiers sans les trier, ni maintenant ni plus tard. Plus tard, posez vos questions sur tout cela, et recevez chaque matin une page de ce qui vous attend, avec l’original derrière chaque ligne. |
| `head.og.description` | Save notes, photos, voice notes and files without sorting them, now or later. Ask about any of it later, and each morning get one page of what needs you, all on your own computer. | Gardez notes, photos, notes vocales et fichiers sans les trier, ni maintenant ni plus tard. Plus tard, posez vos questions sur tout cela, et recevez chaque matin une page de ce qui vous attend, le tout sur votre propre ordinateur. |
| `head.og.image.alt` | Nocturne. Keep everything. Nocturne remembers it for you. Ask about any of it later, and get one page each morning of what needs you. Beside the words, the morning page from the app with demo data. | Nocturne. Gardez tout. Nocturne s’en souvient pour vous. Plus tard, posez vos questions sur tout cela, et recevez chaque matin une page de ce qui vous attend. À côté du texte, la page du matin de l’application avec des données d’exemple. |

## Header, footer and switches (every page)

| Id | English | Français |
|---|---|---|
| `header.skip` | Skip to content | Aller au contenu |
| `header.brand.aria` | Nocturne, home | Nocturne, accueil |
| `nav.aria` | Main | Principal |
| `nav.how` | How it works | Comment ça marche |
| `nav.privacy` | Privacy | Confidentialité |
| `nav.engine` | Engine | Moteur |
| `nav.requirements` | Requirements | Prérequis |
| `nav.download` | Download | Télécharger |
| `nav.docs` | Docs | Doc |
| `theme.aria` | Colour theme | Thème de couleur |
| `theme.system` | System | Système |
| `theme.light` | Light | Clair |
| `theme.dark` | Dark | Sombre |
| `lang.aria` L | Language | Langue |
| `docs.inEnglish` L | in English (read by screen readers after a docs link; the visible mark is EN) | en anglais |
| `suggest.text` L | This page is also in German. (Shown on the English page, in the suggested language.) | Cette page existe aussi en français. |
| `suggest.link` L | Read in German | Lire en français |
| `suggest.close` L | Close this note | Fermer ce message |
| `foot.brand` | Nocturne · engine by Lemonade · Apache 2.0 | Nocturne · moteur Lemonade · Apache 2.0 |
| `foot.aria` | Footer | Pied de page |
| `foot.docs` | Docs | Documentation |
| `foot.data` | Your data | Vos données |
| `foot.download` | Download | Télécharger |
| `foot.install` | Install guide | Guide d’installation |
| `foot.font` | Font licence | Licence des polices |

## Hero

| Id | English | Français |
|---|---|---|
| `hero.title` | Keep everything. *Nocturne remembers it for you.* | Gardez tout. *Nocturne s’en souvient pour vous.* |
| `hero.lede` | Save notes, photos, voice notes and files without sorting them, now or later. Ask about any of it later, and each morning get one page of what needs you, all on your own computer. | Gardez notes, photos, notes vocales et fichiers sans les trier, ni maintenant ni plus tard. Plus tard, posez vos questions sur tout cela, et recevez chaque matin une page de ce qui vous attend, le tout sur votre propre ordinateur. |
| `hero.download` | Download free | Télécharger gratuitement |
| `hero.how` | See how it works | Voir comment ça marche |
| `hero.facts.aria` | At a glance | En bref |
| `hero.fact.platforms` | Mac, Windows, Linux | Mac, Windows, Linux |
| `hero.fact.offline` | Offline after setup | Hors ligne après la mise en route |
| `hero.fact.free` | Free and open source | Gratuit et open source |
| `hero.page.alt` | The morning page in Nocturne: Friday 13 November, two things need you today. The Year 4 trip form is due next Friday, with the letter it came from. The dentist appointment changed, with a choice between the card and the voice note. | La page du matin dans Nocturne : vendredi 13 novembre, deux choses vous attendent aujourd’hui. Le formulaire de la sortie scolaire est à rendre vendredi prochain, avec la lettre d’où il vient. Le rendez-vous chez le dentiste a changé, avec un choix entre la carte de rendez-vous et la note vocale. |
| `hero.keep.alt` | The keep box with “Gran’s new number ends 4471” typed in, and Keep and Ask buttons. | La zone pour garder, avec un texte tapé : le nouveau numéro de mamie finit par 4471, et les boutons Garder et Demander. |
| `note.english` L | The app is in English for now. | L’application est en anglais pour l’instant. |
| `note.understanding` L | How well it understands notes and voice in this language depends on the mind it downloads, not on Nocturne itself. | Sa compréhension des notes et de la voix en français dépend du cerveau qu’il télécharge, pas de Nocturne lui-même. |
| `hero.caption` | Keep it in one box, read it on the morning page. From the app, with demo data. | Gardez dans une seule zone, lisez sur la page du matin. Tiré de l’application, avec des données d’exemple. |

## How it works

| Id | English | Français |
|---|---|---|
| `how.eyebrow` | How it works | Comment ça marche |
| `how.title` | Keep it now. Read it once. | Gardez maintenant. Lisez une fois. |
| `keep.eyebrow` | Keep · any time | Garder · à tout moment |
| `keep.title` | Keep anything, file nothing. | Gardez tout, ne classez rien. |
| `keep.body` | Press `Ctrl Shift Space`, or `Cmd Shift Space` on a Mac, then type, talk, paste or drop a photo or a PDF. Nocturne says “Kept.” and you get back to your day. No folders or tags to keep up: you ask, and it finds it. If it cannot read something, it tells you what to do. | Appuyez sur `Ctrl Maj Espace`, ou `Cmd Maj Espace` sur un Mac, puis tapez, parlez, collez ou déposez une photo ou un PDF. Nocturne dit « Gardé. » (Kept.) et vous reprenez votre journée. Pas de dossiers ni d’étiquettes à entretenir : vous demandez, et il le retrouve. S’il ne peut pas lire quelque chose, il vous dit quoi faire. |
| `keep.box.alt` | The quick keep box with “Gran’s new number ends 4471” typed in, and Keep and Ask buttons. | La zone pour garder rapidement, avec un texte tapé : le nouveau numéro de mamie finit par 4471, et les boutons Garder et Demander. |
| `keep.recent.alt` | Recently kept: a photo still being read, voice notes and pasted messages marked understood, a password-locked PDF Nocturne could not read with what to do about it, and a voice note that needs you. | Gardés récemment : une photo en cours de lecture, des notes vocales et des messages collés marqués comme compris, un PDF protégé par mot de passe que Nocturne n’a pas pu lire, avec ce qu’il faut faire, et une note vocale qui vous attend. |
| `ask.eyebrow` | Ask · any time | Demander · à tout moment |
| `ask.title` | Answers with receipts. | Des réponses, preuves à l’appui. |
| `ask.body` | Ask in plain words. Nocturne answers only from what it can show you, with the originals underneath so you can check. When it does not know, it says so. If it gets something wrong, tell it what is right and it remembers. | Demandez avec des mots simples. Nocturne répond uniquement à partir de ce qu’il peut vous montrer, avec les originaux en dessous pour vérifier. Quand il ne sait pas, il le dit. S’il se trompe, dites-lui ce qui est juste et il s’en souviendra. |
| `ask.alt` | An answer to “What does Leo need for his trip?”: a packed lunch, and the trip form is due next Friday with a signature and £12. Below it, the two photographed letters it came from. | Une réponse à la question « De quoi Leo a-t-il besoin pour sa sortie ? » : un panier-repas, et le formulaire de sortie est à rendre vendredi prochain avec une signature et 12 livres. En dessous, les deux lettres photographiées d’où vient la réponse. |
| `page.eyebrow` | The morning page · at your time | La page du matin · à l’heure de votre choix |
| `page.title` | Wake to what needs you. | Au réveil, ce qui vous attend. |
| `page.body` | Whenever the computer is plugged in and idle, by night or by day, Nocturne reads what you kept and notices what changed. At the time you choose, you get a short list to read, hear or print. When two things disagree, it asks instead of guessing. | Dès que l’ordinateur est branché et inactif, de nuit comme de jour, Nocturne lit ce que vous avez gardé et remarque ce qui a changé. À l’heure de votre choix, vous recevez une courte liste à lire, à écouter ou à imprimer. Quand deux choses se contredisent, il demande au lieu de deviner. |
| `page.alt` | The morning page: two things need you today. The Year 4 trip form is due next Friday, with the letter it came from. The dentist appointment changed, with a choice between the appointment card and the voice note. | La page du matin : deux choses vous attendent aujourd’hui. Le formulaire de la sortie scolaire est à rendre vendredi prochain, avec la lettre d’où il vient. Le rendez-vous chez le dentiste a changé, avec un choix entre la carte de rendez-vous et la note vocale. |

## What it promises

| Id | English | Français |
|---|---|---|
| `privacy.eyebrow` | What it promises | Ce qu’il promet |
| `privacy.title` | Your life stays on your computer. | Votre vie reste sur votre ordinateur. |
| `privacy.body` | Out of the box, Nocturne goes online only for downloads you agree to: its mind at setup and any update you allow. Privacy in the app lists each one with the day and the size. | D’origine, Nocturne ne se connecte que pour les téléchargements que vous acceptez : son cerveau à la mise en route et chaque mise à jour que vous autorisez. Confidentialité (Privacy), dans l’application, les liste un par un avec le jour et la taille. |
| `promise.signin.term` | Nothing to sign in to | Pas de compte à créer |
| `promise.signin.detail` | No account, no analytics, no crash reports, no cloud copy. | Pas de compte, pas de statistiques d’usage, pas de rapports de plantage, pas de copie dans le cloud. |
| `promise.checked.term` | Checked, not just claimed | Vérifié, pas seulement promis |
| `promise.checked.detail` | Before a release, Nocturne is run with its network cut off from everything but this computer, and any attempt to reach further fails the check. | Avant une version, Nocturne est lancé avec son réseau coupé de tout sauf de cet ordinateur, et toute tentative d’aller plus loin fait échouer la vérification. |
| `promise.edit.term` | It never edits what you kept | Il ne modifie jamais ce que vous avez gardé |
| `promise.edit.detail` | Your notes, photos and files are never changed. Every answer shows them underneath. If one is wrong, tell it what is right; the old version is kept, not overwritten. | Vos notes, photos et fichiers ne sont jamais modifiés. Chaque réponse les montre en dessous. Si l’une est fausse, dites-lui ce qui est juste ; l’ancienne version est conservée, pas écrasée. |
| `promise.forget.term` | It forgets on purpose | Il oublie exprès |
| `promise.forget.detail` | Things you no longer use fade, and the page says when one is let go. Bring any back; forgetting for good always asks first. | Ce que vous n’utilisez plus s’estompe, et la page dit quand une chose est mise de côté. Vous pouvez tout récupérer ; pour oublier pour de bon, il demande toujours d’abord. |
| `promise.note.term` | One quiet note a day | Une seule notification discrète par jour |
| `promise.note.detail` | When the page is ready, one note says so and how many things need you. Never what you kept. | Quand la page est prête, une notification le dit, avec le nombre de choses qui vous attendent. Jamais ce que vous avez gardé. |
| `promise.extras.term` | Two extras, off until you ask | Deux options, désactivées jusqu’à ce que vous les demandiez |
| `promise.extras.detail` | Away from home lets your own phone ask, keep and read your page, sealed so the company in between cannot read it. Online answers sends a question, and the few remembered sentences it needs, to one service you choose. Never anything marked private. | En déplacement (Away from home) permet à votre propre téléphone de demander, garder et lire votre page, sous scellé, pour que l’entreprise entre les deux ne puisse pas le lire. Réponses en ligne (Online answers) envoie une question, avec les quelques phrases retenues dont elle a besoin, à un seul service de votre choix. Jamais rien de marqué comme privé. |
| `phone.alt` | Nocturne on a paired phone: connected to the home computer, with Ask, Keep and today’s page, and a note that things marked private stay on the computer. | Nocturne sur un téléphone associé : connecté à l’ordinateur de la maison, avec Demander, Garder et la page du jour, et une note indiquant que ce qui est marqué comme privé reste sur l’ordinateur. |
| `phone.caption` | Away from home on a paired phone, with demo data. | En déplacement sur un téléphone associé, avec des données d’exemple. |

## The engine

| Id | English | Français |
|---|---|---|
| `engine.eyebrow` | The engine | Le moteur |
| `engine.title` | Built on (followed by the Lemonade logo, alt text “Lemonade”) | Construit sur (texte alternatif du logo « Lemonade ») |
| `engine.body1` | Nocturne’s thinking runs on **Embeddable Lemonade**, the portable build of [Lemonade], the open-source local AI server. Nocturne ships its own copy, Lemonade {{LEMONADE_VERSION}}, that only this computer can reach. | La réflexion de Nocturne tourne sur **Embeddable Lemonade**, la version portable de [Lemonade], le serveur d’IA local et open source. Nocturne embarque sa propre copie, Lemonade {{LEMONADE_VERSION}}, que seul cet ordinateur peut joindre. |
| `engine.body2` | Lemonade is free under the Apache 2.0 licence and built by its community, with optimisations by AMD engineers. Nocturne would not exist without it. | Lemonade est gratuit sous licence Apache 2.0 et développé par sa communauté, avec des optimisations d’ingénieurs d’AMD. Nocturne n’existerait pas sans lui. |

## Requirements (the install page repeats these)

| Id | English | Français |
|---|---|---|
| `req.eyebrow` | Requirements | Prérequis |
| `req.title` | Will it run on my computer? | Est-ce que ça marchera sur mon ordinateur ? |
| `req.body` | Almost certainly, if it has 8 GB of memory: an 8 GB MacBook Air, or a typical 8 GB Windows or Linux laptop. | Presque certainement, s’il a 8 Go de mémoire : un MacBook Air de 8 Go, ou un portable Windows ou Linux classique de 8 Go. |
| `req.memory.term` | Memory | Mémoire |
| `req.memory.detail` | 8 GB is enough. On an 8 GB computer Nocturne uses about 5 GB at its busiest and gives it back when it has nothing to do. With more memory it offers a larger setup with clearer answers. | 8 Go suffisent. Sur un ordinateur de 8 Go, Nocturne utilise environ 5 Go au plus fort de son activité et les rend quand il n’a rien à faire. Avec plus de mémoire, il propose une configuration plus grande aux réponses plus claires. |
| `req.graphics.term` | A graphics card | Une carte graphique |
| `req.graphics.detail` | No graphics card needed. Macs with Apple silicon, and computers with AMD or Intel graphics built in, run Nocturne on the chip and share the computer’s memory. An AMD or NVIDIA graphics card works too, best with 6 GB or more. Other computers run it on the processor. | Pas besoin de carte graphique. Les Mac avec puce Apple, et les ordinateurs avec graphismes AMD ou Intel intégrés, font tourner Nocturne sur la puce et partagent la mémoire de l’ordinateur. Une carte graphique AMD ou NVIDIA fonctionne aussi, idéalement avec 6 Go ou plus. Les autres ordinateurs le font tourner sur le processeur. |
| `req.chip.term` | An AI chip | Une puce d’IA |
| `req.chip.detail` | Helpful, never needed. On some AMD computers with an AI chip and 16 GB or more, the AI chip reads your voice notes, so answers and the daily tidy do not wait for them. | Utile, jamais nécessaire. Sur certains ordinateurs AMD avec une puce d’IA et 16 Go ou plus, la puce d’IA lit vos notes vocales, pour que les réponses et le rangement quotidien ne les attendent pas. |
| `req.system.term` | The system | Le système |
| `req.system.detail` | macOS 26 or later on an Apple chip, Windows 10 or 11 (64-bit), or 64-bit Linux on Intel, AMD or Arm. | macOS 26 ou ultérieur sur une puce Apple, Windows 10 ou 11 (64 bits), ou Linux 64 bits sur Intel, AMD ou Arm. |
| `tier.large.name` | Large | Grande |
| `tier.large.spec` | 60 GB+ memory · 28 GB space | 60 Go de mémoire ou plus · 28 Go d’espace |
| `tier.large.text` | Clearer answers to harder questions. | Des réponses plus claires aux questions plus difficiles. |
| `tier.standard.name` | Standard | Standard |
| `tier.standard.spec` | 28 GB+ memory · 11 GB space | 28 Go de mémoire ou plus · 11 Go d’espace |
| `tier.standard.text` | Many recent laptops with 32 GB. | Beaucoup de portables récents avec 32 Go. |
| `tier.small.name` | Small | Petite |
| `tier.small.spec` | 14 GB+ memory · 7 GB space | 14 Go de mémoire ou plus · 7 Go d’espace |
| `tier.small.text` | Same page. The daily tidy takes longer. | La même page. Le rangement quotidien prend plus de temps. |
| `tier.tiny.name` | Tiny | Mini |
| `tier.tiny.spec` | 6 GB+ memory · 5.2 GB space | 6 Go de mémoire ou plus · 5,2 Go d’espace |
| `tier.tiny.text` | Made for 8 GB computers. Simpler answers that may miss details. | Faite pour les ordinateurs de 8 Go. Des réponses plus simples, qui peuvent manquer des détails. |
| `req.downloads` | Downloads at setup are between about 3 GB and 26 GB, depending on the setup. After that, Nocturne works with no internet at all. | Les téléchargements de la mise en route vont d’environ 3 Go à 26 Go, selon la configuration. Ensuite, Nocturne fonctionne sans aucune connexion à internet. |

## Get Nocturne (end of the landing page)

| Id | English | Français |
|---|---|---|
| `get.title` | Get Nocturne. | Téléchargez Nocturne. |
| `get.body` | Free and open source, for Mac, Windows and Linux. Download it, open it, and it sets itself up for the computer it is on. | Gratuit et open source, pour Mac, Windows et Linux. Téléchargez-le, ouvrez-le, et il se règle tout seul pour l’ordinateur sur lequel il se trouve. |
| `get.download` | Download | Télécharger |
| `get.source` | Build it from source | Le compiler à partir des sources |

## Not-found page

| Id | English | Français |
|---|---|---|
| `missing.title` | Not here · Nocturne | Introuvable · Nocturne |
| `missing.eyebrow` | Not found | Page introuvable |
| `missing.heading` | There is no page here. | Il n’y a pas de page ici. |
| `missing.body` | The link may be old, or the page may have moved. | Le lien est peut-être ancien, ou la page a été déplacée. |
| `missing.links` | [Go to the front page] or [read the docs]. | [Aller à l’accueil] ou [lire la documentation]. |

## Install page, head and top

| Id | English | Français |
|---|---|---|
| `install.title` | Get Nocturne · Download for Mac, Windows and Linux | Téléchargez Nocturne · pour Mac, Windows et Linux |
| `install.description` | Download Nocturne for a Mac with an Apple chip, a Windows 10 or 11 computer, or a Linux desktop. Free and open source, with no account. The first time it opens, it sets itself up for the computer it is on. | Téléchargez Nocturne pour un Mac avec puce Apple, un ordinateur sous Windows 10 ou 11, ou un bureau Linux. Gratuit et open source, sans compte. À la première ouverture, il se règle tout seul pour l’ordinateur sur lequel il se trouve. |
| `install.og.description` | Free and open source, with no account. The first time it opens, Nocturne sets itself up for the computer it is on. | Gratuit et open source, sans compte. À la première ouverture, Nocturne se règle tout seul pour l’ordinateur sur lequel il se trouve. |
| `install.heading` | Get Nocturne. | Téléchargez Nocturne. |
| `install.lede` | Free and open source, for Mac, Windows and Linux. No account, nothing to sign in to. | Gratuit et open source, pour Mac, Windows et Linux. Pas de compte, rien à quoi se connecter. |
| `install.here` | Choose your computer below. Each link gives the latest version. | Choisissez votre ordinateur ci-dessous. Chaque lien donne la dernière version. |
| `install.hereTag` | For this computer | Pour cet ordinateur |
| `install.firstOpen` | The first time you open it | À la première ouverture |
| `install.others` | Other computers | Autres ordinateurs |

## Install page, Mac

| Id | English | Français |
|---|---|---|
| `mac.spec` | Apple chip, M1 or later · macOS 26 or later | Puce Apple, M1 ou ultérieure · macOS 26 ou ultérieur |
| `mac.download` | Download for Mac | Télécharger pour Mac |
| `mac.note` ◊ | Macs with an Intel chip are not supported. To see which chip yours has, open the Apple menu and choose About This Mac. | Les Mac à puce Intel ne sont pas pris en charge. Pour savoir quelle puce a le vôtre, ouvrez le menu Pomme et choisissez À propos de ce Mac. |
| `mac.step1` ◊ | Open the downloaded file, then drag Nocturne into Applications. | Ouvrez le fichier téléchargé, puis faites glisser Nocturne dans Applications. |
| `mac.step2` ◊ | Open Nocturne from Applications. Your Mac says “Nocturne” Not Opened, because it could not check the app. Click **Done**. | Ouvrez Nocturne depuis Applications. Votre Mac affiche « Nocturne » n’a pas été ouvert, car il n’a pas pu vérifier l’app. Cliquez sur **Terminé**. |
| `mac.step3` ◊ | Open System Settings, then **Privacy & Security**. Scroll down to the line saying Nocturne was blocked and click **Open Anyway**. | Ouvrez Réglages Système, puis **Confidentialité et sécurité**. Faites défiler jusqu’à la ligne indiquant que Nocturne a été bloqué et cliquez sur **Ouvrir quand même**. |
| `mac.step4` ◊ | Enter your password, then click **Open Anyway** again. | Saisissez votre mot de passe, puis cliquez à nouveau sur **Ouvrir quand même**. |
| `mac.why` | You only do this once. Apple charges a yearly fee to sign an app so it opens without asking; Nocturne is free and open source, and this release is not signed yet. | Vous ne le faites qu’une fois. Apple facture des frais annuels pour signer une app afin qu’elle s’ouvre sans demander ; Nocturne est gratuit et open source, et cette version n’est pas encore signée. |

## Install page, Windows

| Id | English | Français |
|---|---|---|
| `win.spec` | Windows 10 or 11, 64-bit | Windows 10 ou 11, 64 bits |
| `win.download` | Download for Windows | Télécharger pour Windows |
| `win.note` | Installs for you alone and needs no administrator password. Computers managed by an organisation can use the [.msi installer] instead. | S’installe pour vous seul et ne demande aucun mot de passe administrateur. Les ordinateurs gérés par une organisation peuvent utiliser le [programme d’installation .msi] à la place. |
| `win.step1` ◊ | Your browser may warn that the file is not commonly downloaded. In Edge, open the **…** menu beside the file, choose **Keep**, then **Show more**, then **Keep anyway**. | Votre navigateur peut signaler que ce fichier n’est pas téléchargé couramment. Dans Edge, ouvrez le menu **…** à côté du fichier, choisissez **Conserver**, puis **Afficher plus**, puis **Conserver quand même**. |
| `win.step2` ◊ | Open the installer. Windows shows “Windows protected your PC”. Click **More info**, then **Run anyway**. | Ouvrez le programme d’installation. Windows affiche « Windows a protégé votre ordinateur ». Cliquez sur **Informations complémentaires**, puis sur **Exécuter quand même**. |
| `win.why` | You only do this once. Microsoft charges a yearly fee to sign an app so it opens without asking; Nocturne is free and open source, and this release is not signed yet. | Vous ne le faites qu’une fois. Microsoft facture des frais annuels pour signer une app afin qu’elle s’ouvre sans demander ; Nocturne est gratuit et open source, et cette version n’est pas encore signée. |

## Install page, Linux

| Id | English | Français |
|---|---|---|
| `linux.spec` | 64-bit computer with an Intel, AMD or Arm chip | Ordinateur 64 bits avec puce Intel, AMD ou Arm |
| `linux.choices.aria` | Ways to install on Linux | Façons d’installer sous Linux |
| `linux.flatpak.title` | From your software centre | Depuis votre logithèque |
| `linux.flatpak.text` | The choice for most people, on any Linux desktop. Your software centre keeps Nocturne up to date. Starting with the computer is not available this way yet. | Le bon choix pour la plupart des gens, sur n’importe quel bureau Linux. Votre logithèque garde Nocturne à jour. Le lancement au démarrage de l’ordinateur n’est pas encore possible de cette façon. |
| `linux.flatpak.button` | Get the Flatpak | Obtenir le Flatpak |
| `linux.flatpak.terminal` | Or install it from a terminal | Ou l’installer depuis un terminal |
| `linux.deb.title` | Ubuntu, Linux Mint or Debian | Ubuntu, Linux Mint ou Debian |
| `linux.package.text` | Open the file with your software centre, and Nocturne appears with your other apps. (Shared by .deb and .rpm.) | Ouvrez le fichier avec votre logithèque, et Nocturne apparaît avec vos autres applications. |
| `linux.deb.intel` | .deb for Intel or AMD | .deb pour Intel ou AMD |
| `linux.deb.arm` | .deb for Arm | .deb pour Arm |
| `linux.rpm.title` | Fedora, Red Hat or openSUSE | Fedora, Red Hat ou openSUSE |
| `linux.rpm.intel` | .rpm for Intel or AMD | .rpm pour Intel ou AMD |
| `linux.rpm.arm` | .rpm for Arm | .rpm pour Arm |
| `linux.appimage.title` | Try it without installing | Essayer sans installer |
| `linux.appimage.text` | One file that runs on most desktops. In its properties, allow it to run as a program, then open it. | Un seul fichier qui fonctionne sur la plupart des bureaux. Dans ses propriétés, autorisez-le à s’exécuter comme un programme, puis ouvrez-le. |
| `linux.appimage.intel` | AppImage for Intel or AMD | AppImage pour Intel ou AMD |
| `linux.appimage.arm` | AppImage for Arm | AppImage pour Arm |
| `linux.note` | Most Linux computers have an Intel or AMD chip. To check, run `uname -m` in a terminal: `x86_64` means Intel or AMD, `aarch64` means Arm. The Flatpak picks the right one by itself. | La plupart des ordinateurs Linux ont une puce Intel ou AMD. Pour vérifier, lancez `uname -m` dans un terminal : `x86_64` signifie Intel ou AMD, `aarch64` signifie Arm. Le Flatpak choisit tout seul le bon. |
| `linux.why` | Linux shows no warning the first time you open Nocturne. | Linux n’affiche aucun avertissement la première fois que vous ouvrez Nocturne. |

## Install page, checks, needs and after

| Id | English | Français |
|---|---|---|
| `install.checks` | To make sure a download is the one we published, compare it with the fingerprints in [SHA256SUMS]. The [install guide] shows how, explains each package and how to take Nocturne off again. Earlier versions are on the [releases page]. | Pour vous assurer qu’un téléchargement est bien celui que nous avons publié, comparez-le aux empreintes de [SHA256SUMS]. Le [guide d’installation] explique comment faire, décrit chaque paquet et comment retirer Nocturne. Les versions précédentes sont sur la [page des versions]. |
| `needs.title` | What your computer needs. (The rest of this section is the Requirements rows above.) | Ce qu’il faut à votre ordinateur. |
| `after.title` | After you install. | Après l’installation. |
| `after.body` | Nocturne opens a short setup. You can keep things straight away. | Nocturne ouvre une courte mise en route. Vous pouvez garder des choses tout de suite. |
| `after.1.when` | When it opens | À l’ouverture |
| `after.1.title` | It looks at the computer | Il examine l’ordinateur |
| `after.1.text` | It tells you which setup it will use. You can always pick a smaller one, which uses less memory, space and battery. | Il vous dit quelle configuration il va utiliser. Vous pouvez toujours en choisir une plus petite, qui utilise moins de mémoire, d’espace et de batterie. |
| `after.2.when` | Once, when you agree | Une fois, si vous acceptez |
| `after.2.title` | It downloads what it needs | Il télécharge ce dont il a besoin |
| `after.2.text` | It asks first, then downloads what this computer can run: between about 3 GB and 26 GB. This is the only time Nocturne needs the internet, and it carries on by itself if the connection drops. | Il demande d’abord, puis télécharge ce que cet ordinateur peut faire tourner : entre 3 Go et 26 Go environ. C’est le seul moment où Nocturne a besoin d’internet, et il reprend tout seul si la connexion coupe. |
| `after.3.when` | The next morning | Le lendemain matin |
| `after.3.title` | Your first page | Votre première page |
| `after.3.text` | From then on, Nocturne works with no internet at all. | Ensuite, Nocturne fonctionne sans aucune connexion à internet. |

## Install page, messages from install.js

| Id | English | Français |
|---|---|---|
| `js.mac` | This looks like a Mac, so its download comes first. Nocturne needs a Mac with an Apple chip. | On dirait un Mac, son téléchargement apparaît donc en premier. Nocturne a besoin d’un Mac avec puce Apple. |
| `js.macArm` | This looks like a Mac with an Apple chip, so its download comes first. | On dirait un Mac avec puce Apple, son téléchargement apparaît donc en premier. |
| `js.macIntel` ◊ | This looks like a Mac with an Intel chip. Nocturne needs a Mac with an Apple chip (M1 or later), so it will not run on this one. About This Mac, in the Apple menu, shows which chip it has. | On dirait un Mac avec puce Intel. Nocturne a besoin d’un Mac avec puce Apple (M1 ou ultérieure), il ne fonctionnera donc pas sur celui-ci. À propos de ce Mac, dans le menu Pomme, indique quelle puce il a. |
| `js.windows` | This looks like a Windows computer, so its download comes first. | On dirait un ordinateur Windows, son téléchargement apparaît donc en premier. |
| `js.linux` | This looks like a Linux computer, so its choices come first. | On dirait un ordinateur Linux, ses options apparaissent donc en premier. |
| `js.linuxArm` | This looks like a Linux computer with an Arm chip, so its choices come first, Arm downloads first. | On dirait un ordinateur Linux avec puce Arm, ses options apparaissent donc en premier, les téléchargements Arm d’abord. |
| `js.other` | Nocturne is a desktop app, for Mac, Windows and Linux computers. Open this page on the computer you want to use it on, or choose one below. | Nocturne est une application de bureau, pour les ordinateurs Mac, Windows et Linux. Ouvrez cette page sur l’ordinateur où vous voulez l’utiliser, ou choisissez-en un ci-dessous. |

## Decisions

- « Garder » for keep: warmer and shorter than « Conserver ».
- ◊ rows follow the system's wording and were accepted as written.
- « Doc » for Docs in the header: the everyday short form, so the header keeps one line at 1024 px. The footer and the not-found page say « documentation » in full.

## Approval

The owner approved these words on 2026-11-16, with the answers in [the site in five languages](../site-localisation.md#approval).
