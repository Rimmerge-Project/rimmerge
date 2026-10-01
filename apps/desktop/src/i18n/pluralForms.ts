import type { SupportedLocale } from "@/i18n/locales";

/**
 * The CLDR plural categories each locale's catalogue writes a
 * pipe-separated form for, in the order those forms appear in the JSON
 * (`"{count} mod | {count} mods"` is `["one", "other"]`). This is the one
 * authority for a locale's form count and order: `format.ts` builds the
 * runtime selectors from it, `locales.test.ts` and
 * `scripts/i18n-status.mjs` enforce its length, and `docs/translating.md`
 * documents it for translators. Keep the four in step.
 *
 * Dependency-free on purpose (a type-only import) so the Node status
 * script can load this file directly.
 */
export const PLURAL_FORM_ORDER: Readonly<Record<SupportedLocale, readonly Intl.LDMLPluralRule[]>> =
  {
    en: ["one", "other"],
    // pt-BR: CLDR's `pt` rule treats 0 *and* 1 as `one` ("0 mod", "1 mod"),
    // unlike English — `Intl.PluralRules` gets this right where a
    // hand-written `n === 1` check would not (see locales.test.ts).
    "pt-BR": ["one", "other"],
    de: ["one", "other"],
    fr: ["one", "other"],
    "es-ES": ["one", "other"],
    tr: ["one", "other"],
    // Slavic locales: `one` (21, 31, 101, ...), `few` (2-4, 22-24, ...),
    // `many` (0, 5-20, 25-30, ...). CLDR's `other` (fractions only) never
    // reaches a count in this app; see `pluralFormIndex` for where it lands.
    ru: ["one", "few", "many"],
    uk: ["one", "few", "many"],
    pl: ["one", "few", "many"],
    // No plural distinction at all: one form, no pipe.
    "zh-CN": ["other"],
    "zh-TW": ["other"],
    ja: ["other"],
    ko: ["other"],
  };

/** How many pipe-separated plural forms a locale's catalogue is expected to carry. */
export function expectedPluralFormCount(locale: SupportedLocale): number {
  return PLURAL_FORM_ORDER[locale].length;
}
