/** The locales this app ships a catalogue for, in picker order. */
export const SUPPORTED_LOCALES = [
  "en",
  "zh-CN",
  "pt-BR",
  "ru",
  "uk",
  "pl",
  "de",
  "fr",
  "es-ES",
  "tr",
  "ja",
  "ko",
  "zh-TW",
] as const;

/** A locale this app can actually render — always one of {@link SUPPORTED_LOCALES}. */
export type SupportedLocale = (typeof SUPPORTED_LOCALES)[number];

/**
 * What `stores/preferences.ts` persists: a {@link SupportedLocale}, or
 * `"system"` — the default — meaning "re-detect from the OS at every
 * start" rather than a locale pinned by the user.
 */
export type StoredLocale = "system" | SupportedLocale;

/** One row of the language picker. Names are never translated — a locale always names itself. */
export type LocaleOption = {
  readonly locale: SupportedLocale;
  readonly nativeName: string;
  /** Machine-drafted and not yet reviewed by a native speaker — the picker shows a "preview" note for these. */
  readonly isPreview: boolean;
};

/** The language picker's own rows, in display order. */
export const LOCALE_OPTIONS: readonly LocaleOption[] = [
  { locale: "en", nativeName: "English", isPreview: false },
  { locale: "zh-CN", nativeName: "简体中文", isPreview: true },
  { locale: "pt-BR", nativeName: "Português (Brasil)", isPreview: false },
  { locale: "ru", nativeName: "Русский", isPreview: true },
  { locale: "uk", nativeName: "Українська", isPreview: true },
  { locale: "pl", nativeName: "Polski", isPreview: true },
  { locale: "de", nativeName: "Deutsch", isPreview: true },
  { locale: "fr", nativeName: "Français", isPreview: true },
  { locale: "es-ES", nativeName: "Español (España)", isPreview: true },
  { locale: "tr", nativeName: "Türkçe", isPreview: true },
  { locale: "ja", nativeName: "日本語", isPreview: true },
  { locale: "ko", nativeName: "한국어", isPreview: true },
  { locale: "zh-TW", nativeName: "繁體中文", isPreview: true },
];

/** Narrows an arbitrary string to a {@link SupportedLocale}. */
export function isSupportedLocale(value: string): value is SupportedLocale {
  return (SUPPORTED_LOCALES as readonly string[]).includes(value);
}

/**
 * Which of our locales serves each BCP 47 primary language subtag when
 * the tag has no more specific rule (see {@link detectLocale}). Every
 * regional variant of a language lands on the one catalogue we ship for
 * it: `es-MX`/`es-419` read the Castilian `es-ES` catalogue, `pt-PT` the
 * Brazilian one, `fr-CA`/`de-AT` the plain `fr`/`de`. `zh` is absent on
 * purpose: its script decides, handled by {@link detectChineseLocale}.
 */
const LOCALE_BY_PRIMARY_LANGUAGE: Readonly<Record<string, SupportedLocale>> = {
  en: "en",
  pt: "pt-BR",
  ru: "ru",
  uk: "uk",
  pl: "pl",
  de: "de",
  fr: "fr",
  es: "es-ES",
  tr: "tr",
  ja: "ja",
  ko: "ko",
};

/** Regions that write Traditional Chinese when a `zh` tag carries no script subtag. */
const TRADITIONAL_CHINESE_REGIONS: ReadonlySet<string> = new Set(["tw", "hk", "mo"]);

/** The locale for the subtags after `zh`, or `null` when they name neither a known script nor a known region. */
function detectChineseLocale(subtags: readonly string[]): SupportedLocale | null {
  if (subtags.length === 0 || subtags.includes("hans")) {
    return "zh-CN";
  }
  if (subtags.includes("hant")) {
    return "zh-TW";
  }
  if (subtags.some((subtag) => TRADITIONAL_CHINESE_REGIONS.has(subtag))) {
    return "zh-TW";
  }
  if (subtags.includes("cn") || subtags.includes("sg")) {
    return "zh-CN";
  }
  return null;
}

/**
 * Picks a {@link SupportedLocale} from an ordered list of language tags
 * (`navigator.languages`) — pure and synchronous, so it can run before
 * first paint. Takes the first entry that matches any rule; entries
 * that match nothing (a language we ship no catalogue for) are skipped
 * rather than treated as `en`, so an `sv-SE` reader ahead of an `en-US`
 * fallback still lands on `en`. A Chinese tag is decided by its script
 * (`Hant` is `zh-TW`, `Hans` is `zh-CN`), else by its region (`TW`/`HK`/
 * `MO` are `zh-TW`; `CN`/`SG` or no region are `zh-CN`); every other
 * language maps through {@link LOCALE_BY_PRIMARY_LANGUAGE}. See the
 * table in `locales.test.ts`'s own `detectLocale` cases for every
 * mapped case.
 */
export function detectLocale(languages: readonly string[]): SupportedLocale {
  for (const raw of languages) {
    const [primary = "", ...subtags] = raw.toLowerCase().replaceAll("_", "-").split("-");
    const matched =
      primary === "zh"
        ? detectChineseLocale(subtags)
        : (LOCALE_BY_PRIMARY_LANGUAGE[primary] ?? null);
    if (matched !== null) {
      return matched;
    }
  }
  return "en";
}
