import { readdirSync, readFileSync } from "node:fs";
import { afterEach, describe, expect, it, vi } from "vitest";
import { createI18n, type DefineLocaleMessage } from "vue-i18n";
import { buildPluralRules, pluralFormIndex } from "@/i18n/format";
import {
  areSetsEqual,
  findDuplicateKeys,
  flattenMessages,
  isMetadataKey,
  placeholdersOf,
} from "@/i18n/localeMessages";
import { detectLocale, SUPPORTED_LOCALES, type SupportedLocale } from "@/i18n/locales";
import { expectedPluralFormCount, PLURAL_FORM_ORDER } from "@/i18n/pluralForms";
import de from "@/locales/de.json";
import en from "@/locales/en.json";
import esES from "@/locales/es-ES.json";
import fr from "@/locales/fr.json";
import ja from "@/locales/ja.json";
import ko from "@/locales/ko.json";
import pl from "@/locales/pl.json";
import ptBR from "@/locales/pt-BR.json";
import ru from "@/locales/ru.json";
import tr from "@/locales/tr.json";
import uk from "@/locales/uk.json";
import zhCN from "@/locales/zh-CN.json";
import zhTW from "@/locales/zh-TW.json";

/**
 * This catalogue's parity check, strict since both `zh-CN` and `pt-BR`
 * reached full translation (2026-09-26) — a missing key now fails the
 * gate for every locale, every run. Extra keys, malformed values,
 * placeholder mismatches and wrong plural form counts already failed in
 * both modes; only "missing" was ever gated by this constant. See
 * `docs/translating.md`.
 *
 * {@link isStrictForLocale} still honors `I18N_STRICT_LOCALE=<locale>
 * bun run test` as a no-op superset of this constant now that it's
 * `true` for both locales — kept so the documented command stays valid
 * rather than becoming a silent no-op with no deprecation path.
 */
const STRICT_TRANSLATION_PARITY = true;

/** Whether the missing-key check is strict for `locale` this run — see {@link STRICT_TRANSLATION_PARITY}. */
function isStrictForLocale(locale: SupportedLocale): boolean {
  return STRICT_TRANSLATION_PARITY || process.env["I18N_STRICT_LOCALE"] === locale;
}

const LOCALE_MODULES: Record<SupportedLocale, unknown> = {
  en,
  "zh-CN": zhCN,
  "pt-BR": ptBR,
  ru,
  uk,
  pl,
  de,
  fr,
  "es-ES": esES,
  tr,
  ja,
  ko,
  "zh-TW": zhTW,
};
const NON_ENGLISH_LOCALES: readonly SupportedLocale[] = SUPPORTED_LOCALES.filter(
  (locale) => locale !== "en",
);

const enMessages = flattenMessages(en);

function localeFilePath(locale: SupportedLocale): string {
  return `${import.meta.dirname}/../locales/${locale}.json`;
}

describe("locale catalogue duplicate-key check", () => {
  // Reads the *raw* file text, never the already-`import`-ed module:
  // `JSON.parse` (which every `import`/`require` of a `.json` file goes
  // through under the hood) silently keeps only the last occurrence of
  // a repeated key, so `en`/`zhCN`/`ptBR` above can never reveal a
  // duplicate that already happened — only the source text still shows
  // it. A real instance of this shipped once already (two separate
  // edits reopened the same namespace object ~140 lines apart); nothing
  // else in this file's other checks would have caught it, since the
  // *parsed* object is already missing the discarded keys by the time
  // any other assertion runs.
  it.each(SUPPORTED_LOCALES)("%s has no key repeated within the same object", (locale) => {
    const text = readFileSync(localeFilePath(locale), "utf8");
    expect(findDuplicateKeys(text)).toEqual([]);
  });

  it("the detector itself actually catches a repeated key (not a silent no-op)", () => {
    const withDuplicate = `{
      "a": { "x": "1" },
      "b": "2",
      "a": { "y": "3" }
    }`;
    expect(findDuplicateKeys(withDuplicate)).toEqual(["a"]);
  });

  it("catches a duplicate nested inside an otherwise-unique object, with its full dotted path", () => {
    const withDuplicate = `{
      "shell": {
        "nav": { "dashboard": "Dashboard" },
        "nav": { "dashboard": "Dashboard (dup)" }
      }
    }`;
    expect(findDuplicateKeys(withDuplicate)).toEqual(["shell.nav"]);
  });

  it("does not flag the same key name used at different nesting levels", () => {
    const noDuplicate = `{
      "a": { "label": "1" },
      "b": { "label": "2" }
    }`;
    expect(findDuplicateKeys(noDuplicate)).toEqual([]);
  });
});

describe("English plural messages", () => {
  const pluralMessages = [...enMessages].filter(
    ([path, message]) => !isMetadataKey(path) && message.includes("|"),
  );

  // A form that drops the count (or any other placeholder) its siblings
  // carry reads wrongly once a translator copies its shape — and a form
  // the count never reaches can't be inflected for that count.
  it.each(pluralMessages)("%s carries the same placeholders in every form", (_path, message) => {
    const forms = message.split("|").map((form) => placeholdersOf(form));
    const [first, ...rest] = forms;
    expect(first).toBeDefined();
    for (const form of rest) {
      expect([...form].sort()).toEqual([...(first ?? [])].sort());
    }
  });
});

describe("locale catalogue parity", () => {
  it.each(NON_ENGLISH_LOCALES)("%s has no extra or malformed keys relative to en", (locale) => {
    const localeMessages = flattenMessages(LOCALE_MODULES[locale]);
    const extraKeys = [...localeMessages.keys()].filter(
      (key) => !isMetadataKey(key) && !enMessages.has(key),
    );
    expect(extraKeys).toEqual([]);
  });

  it.each(NON_ENGLISH_LOCALES)("%s has no missing keys (strict mode only)", (locale) => {
    const localeMessages = flattenMessages(LOCALE_MODULES[locale]);
    // Metadata keys (`_pending`, any future `_context` note, …) are
    // never translatable content — `en` carrying one must never force
    // every locale to also carry it, in either mode. Consistent with
    // `i18n:status`'s own skip.
    const missingKeys = [...enMessages.keys()].filter(
      (key) => !isMetadataKey(key) && !localeMessages.has(key),
    );
    if (isStrictForLocale(locale)) {
      expect(missingKeys).toEqual([]);
    } else {
      // Tolerant mode: missing keys are expected and fall back to `en`
      // at runtime — nothing to assert beyond the extra/malformed check
      // above, which still ran unconditionally.
      expect(true).toBe(true);
    }
  });

  it.each(NON_ENGLISH_LOCALES)(
    "%s uses the same placeholders as en for every key it has",
    (locale) => {
      const localeMessages = flattenMessages(LOCALE_MODULES[locale]);
      const mismatches: string[] = [];
      for (const [key, value] of localeMessages) {
        if (isMetadataKey(key)) {
          continue;
        }
        const enValue = enMessages.get(key);
        if (enValue === undefined) {
          continue; // already reported as an extra key above
        }
        if (!areSetsEqual(placeholdersOf(value), placeholdersOf(enValue))) {
          mismatches.push(key);
        }
      }
      expect(mismatches).toEqual([]);
    },
  );

  it.each(SUPPORTED_LOCALES)(
    "%s's plural messages have the locale's expected form count",
    (locale) => {
      const localeMessages = flattenMessages(LOCALE_MODULES[locale]);
      const expected = expectedPluralFormCount(locale);
      const wrongCounts: string[] = [];
      for (const [key, value] of localeMessages) {
        if (isMetadataKey(key)) {
          continue;
        }
        // A message is plural when English's is: a translation that
        // dropped the pipes (one form where the locale needs three) is as
        // wrong as one with too many.
        const isPlural = value.includes("|") || (enMessages.get(key)?.includes("|") ?? false);
        if (isPlural && value.split("|").length !== expected) {
          wrongCounts.push(key);
        }
      }
      expect(wrongCounts).toEqual([]);
    },
  );

  it.each(SUPPORTED_LOCALES)("%s's messages all compile and render through t()", (locale) => {
    // Cast: `locale` is dynamic here (parametrized over every supported
    // locale), so the computed `messages` key can't be checked against
    // `schema.d.ts`'s `en`-derived schema the way `i18n.ts`'s own fixed
    // `{ en }` literal can — each locale's own structural validity is
    // already covered by `flattenMessages` (called just below) throwing
    // on a malformed shape.
    const i18n = createI18n({
      legacy: false,
      locale,
      fallbackLocale: false,
      messages: { [locale]: LOCALE_MODULES[locale] } as Record<string, DefineLocaleMessage>,
      pluralRules: buildPluralRules(),
    });
    const localeMessages = flattenMessages(LOCALE_MODULES[locale]);
    for (const [key, value] of localeMessages) {
      if (isMetadataKey(key)) {
        continue;
      }
      const params = Object.fromEntries([...placeholdersOf(value)].map((name) => [name, "x"]));
      expect(() => i18n.global.t(key, params)).not.toThrow();
    }
  });
});

describe("isStrictForLocale", () => {
  afterEach(() => {
    vi.unstubAllEnvs();
  });

  // STRICT_TRANSLATION_PARITY is now permanently `true` (see its own doc
  // comment), so `isStrictForLocale` is strict for every locale
  // unconditionally — `I18N_STRICT_LOCALE` can no longer relax it, only
  // (redundantly) request the same strictness it already has. These
  // cases guard the `||`: a bug that flipped it to `&&`, or a future
  // revert of the constant without updating this suite, would surface
  // here as a locale unexpectedly reading lenient.
  it("is strict for every locale regardless of I18N_STRICT_LOCALE being unset", () => {
    vi.stubEnv("I18N_STRICT_LOCALE", "");
    expect(isStrictForLocale("zh-CN")).toBe(true);
    expect(isStrictForLocale("pt-BR")).toBe(true);
  });

  it("stays strict for a locale I18N_STRICT_LOCALE does not name", () => {
    vi.stubEnv("I18N_STRICT_LOCALE", "pt-BR");
    expect(isStrictForLocale("zh-CN")).toBe(true);
  });
});

describe("detectLocale", () => {
  it.each<[readonly string[], SupportedLocale]>([
    [["zh-CN"], "zh-CN"],
    [["zh"], "zh-CN"],
    [["zh-Hans"], "zh-CN"],
    [["zh-Hans-CN"], "zh-CN"],
    [["zh-SG"], "zh-CN"],
    [["pt-BR"], "pt-BR"],
    [["pt"], "pt-BR"],
    [["pt-PT"], "pt-BR"],
    [["en-US"], "en"],
    [["en"], "en"],
    [["zh-TW"], "zh-TW"],
    [["zh-HK"], "zh-TW"],
    [["zh-MO"], "zh-TW"],
    [["zh-Hant"], "zh-TW"],
    [["zh-Hant-HK"], "zh-TW"],
    [["zh_TW"], "zh-TW"],
    [["zh-Hans-TW"], "zh-CN"],
    [["zh-TW", "en-US"], "zh-TW"],
    [["ru-RU"], "ru"],
    [["ru"], "ru"],
    [["uk-UA"], "uk"],
    [["pl-PL"], "pl"],
    [["de-DE"], "de"],
    [["de-AT"], "de"],
    [["de-CH"], "de"],
    [["fr-FR"], "fr"],
    [["fr-CA"], "fr"],
    [["es-ES"], "es-ES"],
    [["es-MX"], "es-ES"],
    [["es-419"], "es-ES"],
    [["es"], "es-ES"],
    [["tr-TR"], "tr"],
    [["ja-JP"], "ja"],
    [["ko-KR"], "ko"],
    [["EN-us"], "en"],
    [["sv-SE", "de-DE"], "de"],
    [["sv-SE"], "en"],
    [["zh-Latn"], "en"],
    [["", "fr"], "fr"],
    [[], "en"],
  ])("detectLocale(%j) -> %s", (languages, expected) => {
    expect(detectLocale(languages)).toBe(expected);
  });
});

describe("plural form table", () => {
  it("declares exactly the supported locales", () => {
    expect(Object.keys(PLURAL_FORM_ORDER).sort()).toEqual([...SUPPORTED_LOCALES].sort());
  });

  it.each<[SupportedLocale, number]>([
    ["en", 2],
    ["pt-BR", 2],
    ["de", 2],
    ["fr", 2],
    ["es-ES", 2],
    ["tr", 2],
    ["ru", 3],
    ["uk", 3],
    ["pl", 3],
    ["zh-CN", 1],
    ["zh-TW", 1],
    ["ja", 1],
    ["ko", 1],
  ])("%s expects %i plural forms", (locale, count) => {
    expect(expectedPluralFormCount(locale)).toBe(count);
  });

  it("rejects a catalogue message with the wrong form count (the check can fail)", () => {
    // Mirrors the form-count check above on a synthetic message, so a
    // regression that made it vacuous would show here.
    const isRightCount = (message: string, locale: SupportedLocale): boolean =>
      message.split("|").length === expectedPluralFormCount(locale);
    expect(isRightCount("{count} мод | {count} мода | {count} модов", "ru")).toBe(true);
    expect(isRightCount("{count} мод | {count} модов", "ru")).toBe(false);
    expect(isRightCount("{count} 個 Mod", "zh-TW")).toBe(true);
  });
});

describe("pluralFormIndex", () => {
  // CLDR: ru/uk one = n%10==1 && n%100!=11, few = n%10 in 2..4 && n%100 not in 12..14, many = the rest.
  // pl: one = exactly 1, few = n%10 in 2..4 && n%100 not in 12..14, many = the rest (incl. 0, 5-21, 25...).
  const COUNTS = [0, 1, 2, 5, 11, 21, 22, 25, 101, 111] as const;
  const SLAVIC_CASES: ReadonlyArray<[SupportedLocale, readonly number[]]> = [
    ["ru", [2, 0, 1, 2, 2, 0, 1, 2, 0, 2]],
    ["uk", [2, 0, 1, 2, 2, 0, 1, 2, 0, 2]],
    ["pl", [2, 0, 1, 2, 2, 2, 1, 2, 2, 2]],
  ];

  it.each(SLAVIC_CASES)(
    "%s picks one/few/many for 0, 1, 2, 5, 11, 21, 22, 25, 101, 111",
    (locale, expected) => {
      // 0 = one, 1 = few, 2 = many (the order in PLURAL_FORM_ORDER).
      expect(COUNTS.map((count) => pluralFormIndex(locale, count, 3))).toEqual(expected);
    },
  );

  it("renders the right Russian form through t() for each count", () => {
    const i18n = createI18n({
      legacy: false,
      locale: "ru",
      fallbackLocale: false,
      messages: { ru: { test: { mods: "{count} мод | {count} мода | {count} модов" } } },
      pluralRules: buildPluralRules(),
    });
    const render = (count: number): string => i18n.global.t("test.mods", { count }, count);
    expect(COUNTS.map(render)).toEqual([
      "0 модов",
      "1 мод",
      "2 мода",
      "5 модов",
      "11 модов",
      "21 мод",
      "22 мода",
      "25 модов",
      "101 мод",
      "111 модов",
    ]);
  });

  it("reads a Polish fraction (CLDR other) as the few form, not an out-of-range index", () => {
    expect(pluralFormIndex("pl", 1.5, 3)).toBe(1);
  });

  it.each<SupportedLocale>(["de", "fr", "es-ES", "tr", "en"])(
    "%s picks one for 1 and other for 2 and 5",
    (locale) => {
      expect([1, 2, 5].map((count) => pluralFormIndex(locale, count, 2))).toEqual([0, 1, 1]);
    },
  );

  it("fr treats 0 as one (CLDR), like pt-BR", () => {
    expect(pluralFormIndex("fr", 0, 2)).toBe(0);
    expect(pluralFormIndex("pt-BR", 0, 2)).toBe(0);
  });

  it("fr and es-ES stay within a 2-form message for 1,000,000 (CLDR many)", () => {
    expect(pluralFormIndex("fr", 1_000_000, 2)).toBe(1);
    expect(pluralFormIndex("es-ES", 1_000_000, 2)).toBe(1);
  });

  it.each<SupportedLocale>(["ja", "ko", "zh-CN", "zh-TW"])(
    "%s always picks its only form",
    (locale) => {
      expect([0, 1, 2, 5, 101].map((count) => pluralFormIndex(locale, count, 1))).toEqual([
        0, 0, 0, 0, 0,
      ]);
    },
  );

  it.each<SupportedLocale>(["ru", "uk", "pl", "ja", "zh-TW"])(
    "%s reads an English two-form fallback message with English one/other",
    (locale) => {
      expect([1, 2, 5, 21].map((count) => pluralFormIndex(locale, count, 2))).toEqual([0, 1, 1, 1]);
    },
  );
});

describe("plural forms", () => {
  it("pt-BR renders the `one` form for a count of 0 (CLDR: 0 and 1 are both `one`)", () => {
    const i18n = createI18n({
      legacy: false,
      locale: "pt-BR",
      fallbackLocale: false,
      messages: {
        "pt-BR": { test: { mods: "{count} mod | {count} mods" } },
      },
      pluralRules: buildPluralRules(),
    });
    expect(i18n.global.t("test.mods", { count: 0 }, 0)).toBe("0 mod");
    expect(i18n.global.t("test.mods", { count: 1 }, 1)).toBe("1 mod");
    expect(i18n.global.t("test.mods", { count: 5 }, 5)).toBe("5 mods");
  });

  it("en renders the `other` form for a count of 0", () => {
    const i18n = createI18n({
      legacy: false,
      locale: "en",
      fallbackLocale: false,
      messages: {
        en: { test: { mods: "{count} mod | {count} mods" } },
      },
      pluralRules: buildPluralRules(),
    });
    expect(i18n.global.t("test.mods", { count: 0 }, 0)).toBe("0 mods");
  });
});

describe("locale catalogue unused-key check", () => {
  const SRC_DIR = `${import.meta.dirname}/../`;
  const EXCLUDED_DIR_SEGMENTS = ["/locales/", "/types/generated/"];

  function collectSourceFiles(dir: string): string[] {
    const files: string[] = [];
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const full = `${dir}${entry.name}`;
      if (entry.isDirectory()) {
        files.push(...collectSourceFiles(`${full}/`));
        continue;
      }
      if (
        (entry.name.endsWith(".ts") || entry.name.endsWith(".vue")) &&
        !entry.name.endsWith(".test.ts") &&
        !EXCLUDED_DIR_SEGMENTS.some((segment) => full.includes(segment))
      ) {
        files.push(full);
      }
    }
    return files;
  }

  it("every en key is referenced as a literal somewhere in src", () => {
    const sourceText = collectSourceFiles(SRC_DIR)
      .map((path) => readFileSync(path, "utf8"))
      .join("\n");

    const unused = [...enMessages.keys()].filter((key) => {
      // `primevue.*` is never looked up key-by-key through `t()` — its
      // whole subtree is bulk-copied onto PrimeVue's own reactive
      // `config.locale` by `i18n.ts`'s `setAppLocale` (see
      // `messages.primevue`, a property access, not a per-key string
      // literal), so this check does not apply to it.
      if (isMetadataKey(key) || key.startsWith("primevue.")) {
        return false;
      }
      return !sourceText.includes(`"${key}"`) && !sourceText.includes(`'${key}'`);
    });

    expect(unused).toEqual([]);
  });
});
