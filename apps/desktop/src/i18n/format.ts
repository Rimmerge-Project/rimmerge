import type { PluralizationRule, PluralizationRulesMap } from "vue-i18n";

import { SUPPORTED_LOCALES, type SupportedLocale } from "@/i18n/locales";
import { PLURAL_FORM_ORDER } from "@/i18n/pluralForms";

const pluralSelectors = new Map<string, Intl.PluralRules>();

/** One cached `Intl.PluralRules` per locale tag — plural selection runs on every rendered count. */
function pluralSelector(tag: string): Intl.PluralRules {
  const cached = pluralSelectors.get(tag);
  if (cached) {
    return cached;
  }
  const created = new Intl.PluralRules(tag);
  pluralSelectors.set(tag, created);
  return created;
}

/**
 * Which pipe form of a message with `formCount` forms a count selects
 * under `locale`. A message whose form count is not the locale's own
 * is an English message that reached this locale through
 * `fallbackLocale` (vue-i18n applies the *current* locale's rule to it,
 * not the rule of the catalogue the message came from), so it is read
 * with English's one/other rule rather than the locale's. The result is
 * always a valid index into the message: plural selection must never be
 * the reason a render fails or comes out empty.
 */
export function pluralFormIndex(
  locale: SupportedLocale,
  choice: number,
  formCount: number,
): number {
  const order = PLURAL_FORM_ORDER[locale];
  if (formCount !== order.length) {
    const englishIndex = pluralSelector("en").select(choice) === "one" ? 0 : 1;
    return Math.min(englishIndex, formCount - 1);
  }
  const category = pluralSelector(locale).select(choice);
  const index = order.indexOf(category);
  if (index !== -1) {
    return index;
  }
  // A category this locale writes no form for: `other` (fractions) reads
  // like `few` (genitive singular) in the Slavic locales; `many` (CLDR
  // adds it for 1,000,000 in fr/es/pt) reads like `other`.
  const few = order.indexOf("few");
  return category === "other" && few !== -1 ? few : order.length - 1;
}

/**
 * Builds vue-i18n's `pluralRules` option: one `Intl.PluralRules`-backed
 * selector per {@link SUPPORTED_LOCALES}, mapping a count onto the index
 * of that locale's own pipe form (see {@link pluralFormIndex}). vue-i18n
 * has no built-in `Intl.PluralRules` integration — its pipe forms are
 * picked by a plain `(choice, choicesLength) => index` function per
 * locale, so this is the seam between the two.
 */
export function buildPluralRules(): PluralizationRulesMap {
  const rules = {} as Record<SupportedLocale, PluralizationRule>;
  for (const locale of SUPPORTED_LOCALES) {
    rules[locale] = (choice: number, choicesLength: number): number =>
      pluralFormIndex(locale, choice, choicesLength);
  }
  return rules;
}

/**
 * Joins already-translated phrases into one locale-appropriate list —
 * `Intl.ListFormat`'s `"conjunction"` type, never a hard-coded English
 * `", "` (a translator can reorder or reword a phrase, but never the
 * literal separator between them, so it must never live in a template
 * literal). `"conjunction"` (not `"unit"`) is deliberate: `zh-CN`'s
 * `"unit"` style renders with no separator at all between narrow-style
 * items (an ICU quirk meant for adjacent measurements like `5 ft 2 in`,
 * not prose phrases), while `"conjunction"` gives the expected `、`
 * between items and `和` before the last one; `pt-BR` gets `"e"` in the
 * same slot, `en` gets `"and"`. Pass `"disjunction"` for alternatives
 * ("A, B, or C"): the same locale rules, `"or"` in the last slot.
 */
export function formatList(
  locale: string,
  parts: readonly string[],
  type: "conjunction" | "disjunction" = "conjunction",
): string {
  return new Intl.ListFormat(locale, { style: "long", type }).format(parts);
}
