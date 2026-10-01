import { detectLocale, LOCALE_OPTIONS, type LocaleOption } from "@/i18n/locales";

/**
 * The {@link LocaleOption} `detectLocale(navigator.languages)` resolves
 * to right now — what the language picker's "System" row names in its
 * own parenthetical (`"System (English)"`). A plain function, not a
 * `computed`: `navigator.languages` is not reactive, and this app never
 * needs to react to it changing mid-session (the picker recomputes it
 * on every render regardless, which is enough).
 */
export function detectSystemLocale(): LocaleOption {
  const detected = detectLocale(navigator.languages);
  // `LOCALE_OPTIONS` always has one row per `SupportedLocale`
  // (`detectLocale`'s own return type), so this lookup can't miss.
  const option = LOCALE_OPTIONS.find((candidate) => candidate.locale === detected);
  return option ?? { locale: detected, nativeName: detected, isPreview: false };
}
