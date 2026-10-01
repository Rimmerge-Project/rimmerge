#!/usr/bin/env node
// Reports, per non-English locale, how far its catalogue is from `en`:
// translated, missing, extra, stale (the `en` text changed after the
// translation was recorded), untracked (translated, but never
// recorded, so staleness can't be judged) and plural (a message with the
// wrong number of `|` forms for the locale, per `pluralForms.ts`). A report, never a gate: the
// report always exits 0 (only a usage or file error exits 1), because a
// gate that fails whenever English copy changes pushes people into
// re-recording hashes without re-reading the translation.
// `locales.test.ts` is the gate (extra keys, placeholders, plural forms,
// and missing keys once strict).
//
// Staleness mechanism: `src/locales/translated-from/<locale>.json` maps
// each translated key to a short SHA-256 of the `en` text it was
// translated from. `--record` writes it; nothing else does. See
// docs/translating.md ("Checking your work").
//
// Usage:
//   node scripts/i18n-status.mjs                  report every locale
//   node scripts/i18n-status.mjs --missing        also list missing keys
//   node scripts/i18n-status.mjs --record zh-CN   record every translated
//       key that has no entry yet, and drop entries for keys that are
//       gone from en or from the locale
//   node scripts/i18n-status.mjs --record zh-CN --key a.b --key c.d
//       re-record these keys too (after re-translating a stale key)

import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { flattenMessages, isMetadataKey } from "../src/i18n/localeMessages.ts";
import { SUPPORTED_LOCALES } from "../src/i18n/locales.ts";
import { expectedPluralFormCount } from "../src/i18n/pluralForms.ts";

const TRANSLATED_LOCALES = SUPPORTED_LOCALES.filter((locale) => locale !== "en");
const HASH_LENGTH = 12;

const scriptsDir = path.dirname(fileURLToPath(import.meta.url));
const localesDir = path.resolve(scriptsDir, "../src/locales");
const recordsDir = path.join(localesDir, "translated-from");

function readJson(file) {
  return JSON.parse(readFileSync(file, "utf8"));
}

/** Flattened, metadata-free messages of one locale file. */
function readCatalogue(locale) {
  const flat = flattenMessages(readJson(path.join(localesDir, `${locale}.json`)));
  return new Map([...flat].filter(([key]) => !isMetadataKey(key)));
}

function recordsFile(locale) {
  return path.join(recordsDir, `${locale}.json`);
}

function readRecords(locale) {
  const file = recordsFile(locale);
  return existsSync(file) ? new Map(Object.entries(readJson(file))) : new Map();
}

function sourceHash(text) {
  return createHash("sha256").update(text, "utf8").digest("hex").slice(0, HASH_LENGTH);
}

/** Keys whose message has the wrong number of plural forms for `locale` (a message is plural when its English is). */
function wrongPluralKeys(english, translated, locale) {
  const expected = expectedPluralFormCount(locale);
  return [...translated]
    .filter(([key, value]) => {
      const isPlural = value.includes("|") || (english.get(key)?.includes("|") ?? false);
      return isPlural && value.split("|").length !== expected;
    })
    .map(([key]) => key);
}

function statusOf(english, locale) {
  const translated = readCatalogue(locale);
  const records = readRecords(locale);
  const present = [...translated.keys()].filter((key) => english.has(key));
  return {
    locale,
    translated: present,
    missing: [...english.keys()].filter((key) => !translated.has(key)),
    extra: [...translated.keys()].filter((key) => !english.has(key)),
    stale: present.filter(
      (key) => records.has(key) && records.get(key) !== sourceHash(english.get(key)),
    ),
    untracked: present.filter((key) => !records.has(key)),
    plural: wrongPluralKeys(english, translated, locale),
    orphaned: [...records.keys()].filter((key) => !present.includes(key)),
  };
}

function printKeyList(label, keys) {
  if (keys.length === 0) {
    return;
  }
  console.log(`  ${label} (${keys.length}):`);
  for (const key of keys) {
    console.log(`    ${key}`);
  }
}

function report(english, shouldListMissing) {
  const statuses = TRANSLATED_LOCALES.map((locale) => statusOf(english, locale));
  console.log(`i18n status: en has ${english.size} keys\n`);
  const columns = ["translated", "missing", "extra", "stale", "untracked", "plural"];
  console.log(["locale".padEnd(8), ...columns.map((column) => column.padStart(11))].join(""));
  for (const status of statuses) {
    const counts = columns.map((column) => String(status[column].length).padStart(11));
    console.log([status.locale.padEnd(8), ...counts].join(""));
  }
  for (const status of statuses) {
    const actionable = [
      status.stale,
      status.extra,
      status.untracked,
      status.orphaned,
      status.plural,
    ];
    const hasNothingToList = actionable.every((keys) => keys.length === 0);
    console.log(
      `\n${status.locale}:${hasNothingToList ? " no stale, extra, untracked or wrong-plural keys" : ""}`,
    );
    printKeyList("stale: en changed since translated, re-translate then re-record", status.stale);
    printKeyList("extra: not in en, remove", status.extra);
    printKeyList("untracked: translated but never recorded", status.untracked);
    printKeyList(
      `plural: needs exactly ${expectedPluralFormCount(status.locale)} form(s) separated by |`,
      status.plural,
    );
    printKeyList("orphaned records: pruned by the next --record", status.orphaned);
    if (shouldListMissing) {
      printKeyList("missing", status.missing);
    }
  }
}

function record(english, locale, forcedKeys) {
  if (!TRANSLATED_LOCALES.includes(locale)) {
    throw new Error(`--record needs one of: ${TRANSLATED_LOCALES.join(", ")}`);
  }
  const translated = readCatalogue(locale);
  const unknown = forcedKeys.filter((key) => !english.has(key) || !translated.has(key));
  if (unknown.length > 0) {
    throw new Error(`--key names a key missing from en or ${locale}: ${unknown.join(", ")}`);
  }
  const previous = readRecords(locale);
  const next = {};
  const keys = [...translated.keys()].filter((key) => english.has(key)).sort();
  for (const key of keys) {
    const isForced = forcedKeys.includes(key);
    next[key] = previous.has(key) && !isForced ? previous.get(key) : sourceHash(english.get(key));
  }
  mkdirSync(recordsDir, { recursive: true });
  writeFileSync(recordsFile(locale), `${JSON.stringify(next, null, 2)}\n`);
  console.log(
    `recorded ${keys.length} keys in ${path.relative(process.cwd(), recordsFile(locale))}`,
  );
}

/** Parses argv into `{ recordLocale, forcedKeys, shouldListMissing }`; throws on anything unknown. */
function parseArgs(argv) {
  const options = { recordLocale: undefined, forcedKeys: [], shouldListMissing: false };
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    const value = argv[index + 1];
    if (argument === "--missing") {
      options.shouldListMissing = true;
    } else if ((argument === "--record" || argument === "--key") && value !== undefined) {
      if (argument === "--record") {
        options.recordLocale = value;
      } else {
        options.forcedKeys.push(value);
      }
      index += 1;
    } else {
      throw new Error(`unknown or incomplete argument: ${argument}`);
    }
  }
  if (options.forcedKeys.length > 0 && options.recordLocale === undefined) {
    throw new Error("--key only works together with --record <locale>");
  }
  return options;
}

try {
  const options = parseArgs(process.argv.slice(2));
  const english = readCatalogue("en");
  if (options.recordLocale === undefined) {
    report(english, options.shouldListMissing);
  } else {
    record(english, options.recordLocale, options.forcedKeys);
  }
} catch (error) {
  // A usage or file error is not a report result, so it does exit non-zero.
  console.error(`i18n-status: ${error instanceof Error ? error.message : String(error)}`);
  process.exitCode = 1;
}
