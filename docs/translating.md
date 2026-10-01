# Translating

The desktop app ships in English (`en`) and twelve translated locales:
Simplified Chinese (`zh-CN`), Traditional Chinese (`zh-TW`), Brazilian
Portuguese (`pt-BR`), Russian (`ru`), Ukrainian (`uk`), Polish (`pl`),
German (`de`), French (`fr`), Castilian Spanish (`es-ES`), Turkish
(`tr`), Japanese (`ja`) and Korean (`ko`). This page is for anyone who writes
or fixes a translation: the files, the message format, the words that
never change, and the checks that tell you you're done. The CLI stays
English, and nothing here applies to it.

Commands below run from `apps/desktop/`.

## Files

| Path (under `apps/desktop/`) | What it holds |
| --- | --- |
| `src/locales/en.json` | The source catalogue. Every key exists here first. |
| `src/locales/<locale>.json` (one per locale above) | The translations, keyed exactly like `en.json`. A key missing here falls back to English at runtime. A locale nobody has translated yet is the empty object `{}`. |
| [`src/locales/glossary.md`](../apps/desktop/src/locales/glossary.md) | The fixed term for every RimWorld and Rimmerge concept, per language. |
| `src/locales/translated-from/<locale>.json` | A short hash of the English text each key was translated from, written by `bun run i18n:status --record`. It is how staleness is detected (see [Checking your work](#checking-your-work)). |
| `src/i18n/locales.test.ts` | The gate: key parity, placeholders, plural forms, and that every message compiles. |
| `src/i18n/locales.ts` | The language picker's rows (native names, `isPreview` flag) and the system-language mapping. |
| `src/i18n/pluralForms.ts` | The plural form count and order per locale (the table under [Plurals](#plurals)). |

## Keys

Keys are nested JSON objects, one top-level object per feature
(`shell`, `settings`, `inbox`, …), with camelCase segments:

```json
{
  "shell": {
    "nav": {
      "inbox": "Findings",
      "order": "Load order"
    }
  }
}
```

The dotted path (`shell.nav.inbox`) is the key. Where a family of keys
maps an enum, the last segment is the enum's wire value verbatim
(`error.code.rimworld_running`), so it can look out of style; leave it.

A translation file never adds, removes or renames a key. It has exactly
the keys of `en.json`, with the values translated. Keys come from the
English catalogue; if one looks wrong, raise it rather than working
around it in a locale file.

`primevue.*` holds the strings PrimeVue's own components show (an empty
dropdown's message, a dialog's close button label). Translate it like
any other subtree.

## Writing a message

### Placeholders

A placeholder is a name in braces: `{count}`, `{mod}`, `{after}`. The
app fills it in at runtime, usually with a number, a mod name or a file
path. Every placeholder in the English message appears in the
translation, spelled identically, and no new one appears. The order
may change to suit the sentence:

```json
"en":    "{mod} must load after {after}"
"zh-CN": "{mod} 必须在 {after} 之后加载"
```

Never translate the name inside the braces. The parity test fails when
a key's placeholder set differs from English.

### Plurals

A message with a count carries one form per plural category, separated
by `|`. The number of forms and their order are fixed per locale (they
live in `src/i18n/pluralForms.ts`, the one place the app, the tests and
`bun run i18n:status` all read):

| Locale | Forms | Order, and which counts pick which form |
| --- | --- | --- |
| `en`, `de`, `es-ES`, `tr` | 2 | `one | other`: 1 → `one`; 0, 2, 3, … → `other` |
| `pt-BR`, `fr` | 2 | `one | other`: **0 and 1** → `one`; 2, 3, … → `other` |
| `ru`, `uk` | 3 | `one | few | many`: `one` = 1, 21, 31, 101, … (ends in 1, not 11); `few` = 2–4, 22–24, 102, … (ends in 2–4, not 12–14); `many` = 0, 5–20, 25–30, 100, 111, … |
| `pl` | 3 | `one | few | many`: `one` = exactly 1; `few` = 2–4, 22–24, 102, … (ends in 2–4, not 12–14); `many` = 0, 5–21, 25–31, 100, 111, … (21 is `many`, unlike ru/uk) |
| `zh-CN`, `zh-TW`, `ja`, `ko` | 1 | `other`: every count; no `|` at all |

```json
"en":    "{count} mod | {count} mods"
"pt-BR": "{count} mod | {count} mods"
"ru":    "{count} мод | {count} мода | {count} модов"
"zh-CN": "{count} 个 Mod"
```

Counts in the app are whole numbers, so CLDR's separate `other` category
(fractions) never occurs. Write each form so it reads correctly for every
number in its category: the `one` form of ru/uk also serves 21 and 101,
and the `many` form serves 0 and 11. pt-BR and fr follow the CLDR rule,
where zero is singular ("0 mod"): write the `one` form so it reads
correctly for both 0 and 1. A message whose English has no `|` has none
in the translation either, unless the English is a plural message. The
parity test and `bun run i18n:status` both fail a message with the wrong
number of forms for its locale, including one that dropped the `|`
entirely where the locale needs several.

### Special characters

The message format reserves `{`, `}`, `@` and `|`:

- `{` and `}` open and close a placeholder. An unbalanced brace fails to
  compile.
- `@` starts a link to another message. An `@` in plain text, such as
  an email address, fails to compile.
- `|` separates plural forms. A stray `|` silently turns a message into
  a plural message.

Use these characters only where the English message uses them. To show
one literally, write it as a quoted literal: `{'@'}`, `{'|'}`.

### Metadata keys

A key whose last segment starts with `_` is metadata about the file,
not a UI string, and is exempt from every parity check. `_pending` was
this catalogue's own marker while a locale was still incomplete; `zh-CN`
and `pt-BR` reached full translation on 2026-09-26 and no longer carry
it, and the locales added later start as `{}` without one. A future metadata key (a `_context` note, say) would
follow the same convention: add it when it's needed, remove it once
it no longer applies.

## What never gets translated

These stay exactly as written, in every locale:

- mod names, mod descriptions and author names: a mod's author wrote
  them;
- package ids and mod ids;
- def names, def types, `ParentName`s, xpaths, field paths and patch
  operation classes such as `PatchOperationReplace`;
- file names and paths: `ModsConfig.xml`, `About.xml`,
  `LoadFolders.xml`, `Player.log`;
- sha prefixes;
- keyboard-shortcut letters;
- game log text;
- anything the user wrote: decision notes, rule comments, patch and
  assignment descriptions.

Most of these arrive through a placeholder, so leaving the placeholder
alone is enough. When one appears literally in the English text, such
as `ModsConfig.xml`, copy it unchanged.

## Tone and length

- **Register:** concise, neutral software UI, the way RimWorld's own
  translation words its menus. zh-CN uses 你, not 您, and usually
  needs no pronoun at all in a label. pt-BR uses você.
- **Length:** pt-BR runs 20 to 35 % longer than English, and several
  places have little room: sidebar labels, table headers, segmented
  toggles, and dialog buttons. Keep labels as short as the English. A
  button stays a verb, not a sentence.
- **Case:** labels in sentence case, like the English catalogue.
- **Risky actions:** a message about writing `ModsConfig.xml`,
  activating or deactivating mods, overwriting a file, or fetching from
  the network says exactly what the English says, with nothing softened
  and nothing added. Keep "will", "cannot", "overwrite" and "not
  included" as strong as the original.

## The glossary

[`src/locales/glossary.md`](../apps/desktop/src/locales/glossary.md)
fixes one term per concept, per language. Use it every time the concept
appears, so that "active", "load order" or "patch" reads the same on
every page.

Each term is marked `game` or `proposed`:

- A `game` term is the one RimWorld's official translation uses. The
  glossary cites the language archive and key it came from.
- A `proposed` term is Rimmerge's own, for a concept the game doesn't
  have (load order, def, patch, finding). Its row says why that word
  was chosen.

The `game` terms are terminology taken from the game's official
translation files, which belong to Ludeon Studios and their volunteer
translators. Rimmerge uses single words and short labels from them,
with attribution in the glossary; it never copies their sentences.

## Checking your work

### The tests

```sh
bun run test
```

`src/i18n/locales.test.ts` checks every locale file:

- no key repeated twice in the same object — if you reopen a section
  you already translated (to add a key you missed, say) and your editor
  or a merge leaves the old block in place too, only your *new* block
  would otherwise survive silently, with the rest of that section's
  translations gone;
- no key that `en.json` lacks;
- the same placeholders as English, for every key present;
- the right number of plural forms for the locale;
- every message compiles and renders.

It also fails on a missing key. `STRICT_TRANSLATION_PARITY`, a constant
near the top of `locales.test.ts`, was flipped to `true` on 2026-09-26
and stays `true` for good — every run is strict for every locale, with
no environment variable needed. A locale that is still being translated
therefore fails its own "no missing keys" row until it is complete,
and only that row.

`I18N_STRICT_LOCALE=zh-CN bun run test` (or any other locale) still works — it
turns on the missing-key check for that one locale, for that one run —
but is now a redundant superset of the always-on check above. It
remains useful when a new English key lands and a translator wants to
confirm their own locale's row without waiting on the other locale's
translator to catch up in the same run. A metadata key (`_pending`, a
future `_context` note, …) is never required, in either mode.

Either way, the test reports each locale on its own row (`zh-CN has no
missing keys (strict mode only)`) — read your own locale's row.

### The status report

```sh
bun run i18n:status             # per locale: translated, missing, extra, stale, untracked, plural
bun run i18n:status --missing   # also list every missing key
```

The report always exits 0. It is a to-do list, not a gate.

- **missing**: in `en.json`, not yet in the locale.
- **extra**: in the locale, not in `en.json`. Delete it.
- **stale**: the English text changed after the translation was
  recorded. Re-read the English and fix the translation.
- **untracked**: translated but never recorded, so staleness can't be
  judged yet.
- **plural**: a message with the wrong number of plural forms for the
  locale (see [Plurals](#plurals)).

Staleness works through `src/locales/translated-from/<locale>.json`,
which maps each key to a short SHA-256 of the English text at the time
the translation was recorded. After translating, record it:

```sh
bun run i18n:status --record pt-BR
```

This adds an entry for every translated key that has none yet, and
drops entries for keys that no longer exist. It never refreshes an
existing entry, so a stale key stays stale until you have re-translated
it and name it explicitly:

```sh
bun run i18n:status --record pt-BR --key shell.nav.order
```

Commit the `translated-from` file together with the translation.

## Seeing a translation in the app

Run the app with `bun run tauri dev` and pick the language on the
Settings page, or on the Setup screen before a project is loaded. The
switch applies at once, with no reload. "System" follows the Windows
display language. A system language maps to one locale of ours:
Traditional-Chinese tags (`zh-TW`, `zh-HK`, `zh-MO`, `zh-Hant…`) to
`zh-TW`, other Chinese tags (`zh`, `zh-CN`, `zh-SG`, `zh-Hans…`) to
`zh-CN`, every Spanish tag (`es-MX`, `es-419`, …) to `es-ES`, every
Portuguese tag (including `pt-PT`) to `pt-BR`, and any other regional
variant (`de-AT`, `fr-CA`, …) to its language's locale. A language we
ship no catalogue for is skipped, falling through to the next language
the system lists, and finally to English.

A locale whose file is still `{}` runs in English: every key falls back to
`en` (plural messages included), and PrimeVue's own strings stay English.

## Checking the layout after a translation round

Longer words overflow labels the tests cannot see. After a round, a
maintainer runs the locale tour, which switches to each locale through
the real picker, visits every page and the Apply dialog, screenshots
each light and dark, and lists text that is clipped, ellipsized or past
the window edge:

```sh
RIMMERGE_SCREENSHOT_DIR=<a scratch directory> bun run e2e:screenshots
RIMMERGE_SCREENSHOT_DIR=<a scratch directory> RIMMERGE_SCREENSHOT_LOCALES=ru,ja bun run e2e:screenshots
```

The second form tours only the named locales. Each locale gets its own
folder of PNGs and an `overflow.json`; read the report next to the
screenshots, since a flagged element is not always a defect (a long mod
id is meant to ellipsize). The tour runs the mock-IPC app on port 5183,
so never run it together with `bun run e2e`, and it is not a gate.

## Preview locales and reporting a wrong translation

A locale that no native speaker has reviewed yet carries a "preview"
label in the language picker, with a note inviting users who spot a
wrong translation to open an issue. The label comes from the locale's
`isPreview` flag in `src/i18n/locales.ts`; a maintainer sets it to
`false` once a native speaker has reviewed the whole locale. Dropping
the label doesn't close the locale: improvements to a reviewed
translation are as welcome as fixes to a preview one.

To report or fix a translation, open an issue at
<https://github.com/Rimmerge-Project/rimmerge/issues> naming the
language, the screen, the text you saw and the text you expected, or
send a pull request that changes the locale file and passes
`bun run test`.
