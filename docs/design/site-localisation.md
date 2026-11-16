---
type: Design
title: The site in five languages
description: How the site offers the landing page, the install page and the not-found page in German, French, Spanish, Italian and Dutch, with English unchanged.
status: stable
tags: [approved, site, localisation, copy, i18n]
---

# The site in five languages

The landing page, the install page and the not-found page are offered in German, French, Spanish,
Italian and Dutch beside English. The words for each language are in [site copy](site-copy/index.md).
The owner approved this design on 2026-11-16; the answers are under [Approval](#approval).

## What a visitor gets

| Decision | Choice | Why |
|---|---|---|
| Addresses | English stays at `/`, `/install`. Each language gets a folder: `/de/`, `/fr/`, `/es/`, `/it/`, `/nl/`, and `/de/install` and so on. | Every existing link keeps working. A folder per language is a plain static copy, needs no Worker script, and reads clearly in a shared link. |
| First visit | No automatic redirect, ever. The address decides the language. | A redirect guesses wrong for travellers, shared computers and people who read English by choice, and a static site cannot redirect without a script or a cookie. |
| A suggestion on the English pages | One line under the header, in the visitor's language, when the browser's preferred languages put one of the five ahead of English. "Diese Seite gibt es auch auf Deutsch. Auf Deutsch lesen", with a close button. | It reads `navigator.languages`, the same list the browser sends as Accept-Language. It runs from a file in `assets/js/`, so the content security policy is unchanged, sends nothing and sets no cookie. Closing it is remembered in local storage, as the theme choice already is. It never shows on a translated page, so a choice once made is not questioned. |
| Language switch | In the header, in one group with the theme switch: a native `details` disclosure whose summary is a globe and the current language's own name (its two-letter code below 440 px). It opens a list of all six. The footer repeats the six as plain links. | It opens and closes without script. The theme script also closes it on a click outside it or on Escape, as a menu is expected to. The footer copy works with the disclosure closed and on the not-found page. |
| Labels | Each language named in itself, in its own `lang` and `hreflang`: Deutsch, English, Español, Français, Italiano, Nederlands, in that order. No flags. The group is labelled "Language" in the page's language. | A flag names a country, not a language. A reader looks for their language in their own words. |
| Where a switch link goes | The same page in the other language: `/de/install` to `/fr/install`. | The reader keeps their place in the site. |
| Not-found page | `/de/404.html` and so on. | The site's static assets already serve the nearest `404.html` up the path (`not_found_handling: "404-page"`), so a missing `/de/...` page answers in German. |

## The header

One header serves all six languages, so it is laid out for the longest of them, German and French,
and English loses nothing.

| Width | What shows |
|---|---|
| 1024 px and up | One row: the brand; the links to the page's sections, Download and Docs, set right; then the preferences group. |
| 640 to 1023 px | One row: the brand, Download and Docs, the preferences group. The section links leave the header; the page itself is one scroll. |
| Below 640 px | Two rows: the brand and the preferences group, then Download and Docs on their own line. The language shows as its code (DE, FR) below 440 px. |

- **One preferences group.** The theme switch and the language switch share one outlined control,
  split by a hairline, so they read as the two settings they are. The theme buttons are a screen,
  a sun and a moon; each keeps its word (System, Hell, Dunkel) as its accessible name and as a
  tooltip. Icons keep the group the same width in every language.
- **A real disclosure indicator.** The language summary has no browser triangle: a globe, the name,
  and a chevron that turns over when the list is open.
- **Docs in English.** A link to the English docs shows a small EN tag set apart from the label;
  the long form ("auf Englisch") is for screen readers only.
- **Short header labels.** A label that would push the header onto a second line is shortened in
  the header only, as German already did with Doku. French says Doc in the header and
  documentation elsewhere.
- Nothing wraps inside the header, nothing scrolls sideways at 320 px, and every control keeps the
  site's focus ring.

## What the page tells search engines and link previews

| Item | Value |
|---|---|
| `lang` on `html` | `en-GB` (unchanged), `de`, `fr`, `es`, `it`, `nl` |
| `rel="alternate" hreflang` | Every page lists all six versions of itself, plus `x-default` pointing at the English one |
| `rel="canonical"` | Each version points at itself, never at English |
| `og:locale` | `de_DE`, `fr_FR`, `es_ES`, `it_IT`, `nl_NL`, with the other five as `og:locale:alternate` |
| `og:title`, `og:description`, `og:image:alt` | Translated, from the copy files |
| `og:image` | The English card for now (see below) |

## What stays English

- The docs under `/docs/`. Links to them from a translated page keep their translated label and add
  a small "EN" mark, with "in English" (in the page's language) for screen readers, and
  `hreflang="en"`. This covers Docs, Your data, Install guide and Build it from source.
- Names: Nocturne, Lemonade, Embeddable Lemonade, Flatpak, AppImage, macOS, Windows, Linux, MacBook
  Air, AMD, Intel, NVIDIA, Arm, Apache 2.0, SHA256SUMS, file names and terminal commands.
- The app itself, which is English only for now. Each translated landing and install page says so
  in a short note under the hero facts and under the install page's lede, with one more sentence:
  how well Nocturne understands notes and voice in that language depends on the mind it downloads,
  not on Nocturne itself. The note uses "its mind", as the privacy section does, and no other word
  for it.
- While the app is English, running text that names a word the person will see in the app gives the
  English after it in brackets: „Festgehalten.“ (Kept.), Unterwegs (Away from home), Online-Antworten
  (Online answers), Datenschutz (Privacy). The brackets go when the app ships that language.

Key names follow the keyboard and the system in each language: `Strg Umschalt Leertaste` in German,
`Ctrl Maj Espace` in French, `Ctrl Mayús Espacio` in Spanish, `Ctrl Maiusc Spazio` in Italian,
`Ctrl Shift Spatie` in Dutch, and `Cmd` for the Mac key in all five, as it is printed on the key.

Words the operating system shows (About This Mac, Privacy & Security, Open Anyway, the Windows
warning, the Edge menu) are written in each language as that system shows them, not translated by
us. Each is marked ◊ in the copy files and is checked against macOS 26, Windows 11 and Edge in that
language before it ships.

## Pictures

- The screenshots stay as they are: the English app with English demo data, in both themes. The
  caption already says "from the app, with demo data", and the note under the hero facts says the app is English.
- Alt text is translated. It describes what the picture shows in the reader's language and
  translates the meaning of the words in it, without quoting English. The British school year
  ("Year 4") is left out, because the numbers do not match school years elsewhere; the amount stays
  in pounds, written the local way.
- The link preview card (`og.png`) stays English for now. A card per language can follow by
  rendering `site/brand/card.html` once per language.

## Keeping the languages in step

| Rule | How |
|---|---|
| One source per language | The strings move out of the HTML into one file per language under `site/`, keyed by the ids in the copy tables. English is a file like the others. `make site` writes each page once per language from one template. |
| No silent fallback | A language that lacks a string fails the build. A page never mixes languages, except the marked English names above. |
| Every language has every string | The site check fails when a language has a missing or unknown id, so a new English string cannot ship untranslated. |
| Stale translations show | Each translated string records the English it was translated from. When the English changes, the check names every string in every language that now needs a new translation. |
| Copy files stay the record | The tables in `site-copy/` are the approved words. A change to the English goes through design, and the same change updates the five tables. |

The strings live in `site/i18n/<language>.json`. English maps each id to its words; every other
language maps each id to its words and the English they were translated from, which is how a
changed English string shows up as stale. The words are written as they appear in the page, with
their markup; a link to another page of the site is written `{{p:/install}}` and becomes
`/de/install` on the German page. The copy tables in `site-copy/` are the readable record of the
same words.

The install page script (`assets/js/install.js`) keeps its seven English messages, so the English
page is unchanged. A translated install page carries its messages in `data-` attributes on the
line they replace, and the script uses those when they are there.

## Checks before it ships

- `make -C site check` runs on every language: axe in both themes, no sideways scrolling at 360 px
  (German runs about a third longer than English, most of all in buttons and the header), keyboard
  reach of the language switch, and one header row from 640 px up at every width named under
  [The header](#the-header).
- The self-hosted fonts' Latin subset covers „ “ « » ’ é è à ç ñ ß ë ï and the no-break spaces.
  The check renders one line of each to confirm.
- French puts a no-break space before `: ; ? !` and inside « ». Numbers and units are joined by a
  no-break space in every language. In the copy files a plain space stands for both; the build
  writes the right character.

## Approval

The owner approved this design and the five copy pages on 2026-11-16, with these answers:

1. **German register: du.** Formal Sie is not the clear convention for personal tools: Apple's
   German consumer pages say du, while Sie remains in terms of service and business software.
2. **Spanish: Spain** (ordenador, móvil).
3. **English names in brackets:** kept while the app is English.
4. **The suggestion line:** ships.
5. **Other languages:** each translated page says, in one sentence beside the English note, that
   understanding notes and voice in that language depends on the mind Nocturne downloads, not on
   Nocturne itself.
6. **System words** (◊) are accepted as written.

The landing page trust and timing copy was approved the same day, so the copy pages translate its
final wording.
