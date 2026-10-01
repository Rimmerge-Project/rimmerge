import { createI18n, type DefineLocaleMessage, type I18n } from "vue-i18n";
import { buildPluralRules } from "@/i18n/format";
import { detectLocale, type StoredLocale, type SupportedLocale } from "@/i18n/locales";
import type { MessageDescriptor } from "@/i18n/messageDescriptor";
import en from "@/locales/en.json";
import { usePreferencesStore } from "@/stores/preferences";
import { assertNever } from "@/utils/assertNever";

/**
 * Thrown by the `missing` handler (see {@link createAppI18n}) whenever
 * `import.meta.env.MODE === "test"` — Vitest's default mode — and the
 * key is absent from `en`, the source catalogue, so a component test
 * that renders an unknown key turns into a failing assertion instead of
 * a silently-rendered raw dotted key. A key an untranslated locale lacks
 * but `en` has resolves through the fallback, as it does in the app.
 * Outside tests the handler returns `undefined` and lets vue-i18n's own
 * `fallbackLocale`/`missingWarn` behavior take over.
 */
export class MissingMessageError extends Error {
  constructor(locale: string, key: string) {
    super(`missing i18n key "${key}" for locale "${locale}"`);
    this.name = "MissingMessageError";
  }
}

/**
 * The catalogue shape `AppI18n` is typed against: `en` fully (the
 * schema `schema.d.ts` checks every `t()` call site against),
 * every other locale as an open record — they load lazily and are
 * deliberately partial until their own translator agent finishes them.
 */
type LocaleCatalogue = { en: typeof en } & Record<
  Exclude<SupportedLocale, "en">,
  Record<string, unknown>
>;

/**
 * The instance `createAppI18n` returns. Named explicitly (rather than
 * `ReturnType<typeof createAppI18n>`) because `createI18n`'s own
 * generic inference, left to the plain call in {@link createAppI18n},
 * narrows both the message schema and the locale type down to exactly
 * `{ en }`/`"en"` — the one locale actually passed at construction —
 * which then rejects `setAppLocale`'s `appI18n.global.locale.value =
 * resolved` (a `SupportedLocale`) and `loadLocale`'s `setLocaleMessage`
 * for `zh-CN`/`pt-BR`. Naming the type here, against
 * {@link LocaleCatalogue}, keeps every consumer typed for every locale
 * this app ships, not just the one bundled eagerly.
 */
export type AppI18n = I18n<
  LocaleCatalogue,
  Record<string, never>,
  Record<string, never>,
  SupportedLocale,
  false
>;

/**
 * Builds the app's vue-i18n instance. `en` is bundled eagerly (it's
 * always needed, as the fallback locale for both other locales and for
 * a key not yet translated in them); `zh-CN`/`pt-BR` load lazily
 * through {@link loadLocale}. Composition API only (`legacy: false`) —
 * every consumer calls `useI18n()`, never a legacy instance's `$t`
 * template global.
 */
export function createAppI18n(): AppI18n {
  // Cast: see {@link AppI18n}'s own doc comment for why the plain
  // inferred return type is too narrow to use directly.
  return createI18n({
    legacy: false,
    locale: "en",
    fallbackLocale: "en",
    messages: { en },
    pluralRules: buildPluralRules(),
    missingWarn: import.meta.env.DEV,
    fallbackWarn: import.meta.env.DEV,
    missing: (locale, key) => {
      // Called once per locale in the fallback chain that lacks the key
      // (`zh-TW`, then `zh`, then `en`). Only `en` lacking it means the
      // key exists nowhere; an untranslated locale falling back is normal.
      if (import.meta.env.MODE === "test" && locale === "en") {
        throw new MissingMessageError(locale, key);
      }
      return undefined;
    },
  }) as unknown as AppI18n;
}

/**
 * Lazily imports and registers a locale's messages — a no-op once a
 * locale is already loaded (including `en`, bundled from the start).
 * Vite turns each dynamic `import()` into one local chunk per locale;
 * this never reaches the network (the workspace's one allowed outbound
 * host is the rule-database refresh, unrelated to this).
 */
export async function loadLocale(i18n: AppI18n, locale: SupportedLocale): Promise<void> {
  // Asked of the instance itself, not of module state: a second instance
  // (a test's, say) must still receive the messages.
  if (i18n.global.availableLocales.includes(locale)) {
    return;
  }
  // Cast: `zh-CN`/`pt-BR` are deliberately partial until their own
  // translator agent finishes them (see the parity test's
  // `STRICT_TRANSLATION_PARITY` switch) — this asserts the loaded JSON
  // against `en`'s own shape (`DefineLocaleMessage`, `schema.d.ts`'s
  // augmentation) purely to satisfy `setLocaleMessage`'s call signature.
  // vue-i18n itself handles a partial tree fine at runtime (missing
  // leaves resolve through `fallbackLocale`); only the static type is
  // too strict for an intentionally incomplete locale file.
  i18n.global.setLocaleMessage(locale, (await importLocaleMessages(locale)) as DefineLocaleMessage);
}

async function importLocaleMessages(locale: SupportedLocale): Promise<Record<string, unknown>> {
  switch (locale) {
    case "en":
      return en;
    case "zh-CN":
      return (await import("@/locales/zh-CN.json")).default;
    case "pt-BR":
      return (await import("@/locales/pt-BR.json")).default;
    case "ru":
      return (await import("@/locales/ru.json")).default;
    case "uk":
      return (await import("@/locales/uk.json")).default;
    case "pl":
      return (await import("@/locales/pl.json")).default;
    case "de":
      return (await import("@/locales/de.json")).default;
    case "fr":
      return (await import("@/locales/fr.json")).default;
    case "es-ES":
      return (await import("@/locales/es-ES.json")).default;
    case "tr":
      return (await import("@/locales/tr.json")).default;
    case "ja":
      return (await import("@/locales/ja.json")).default;
    case "ko":
      return (await import("@/locales/ko.json")).default;
    case "zh-TW":
      return (await import("@/locales/zh-TW.json")).default;
    default:
      return assertNever(locale);
  }
}

let appI18n: AppI18n | null = null;
let primeVueLocale: Record<string, unknown> | null = null;

/**
 * Registers the app's i18n instance for {@link setAppLocale} to drive —
 * called once from `main.ts`, mirroring `utils/toast.ts`'s
 * `setToastService` registration pattern (a module-level singleton is
 * the deliberate choice there too: this app mounts exactly one i18n
 * instance for its whole lifetime).
 */
export function registerAppI18n(i18n: AppI18n): void {
  appI18n = i18n;
}

/**
 * Registers PrimeVue's own reactive `config.locale` object for
 * {@link setAppLocale} to keep in step — obtained via
 * `app.runWithContext(() => usePrimeVue().config)`, the same pattern
 * `main.ts` already uses for `useToast()`, since `usePrimeVue()` needs
 * an active injection context `i18n.ts` itself never has.
 */
export function registerPrimeVueLocaleConfig(config: Record<string, unknown>): void {
  primeVueLocale = config;
}

/** Resolves a persisted `"system" | SupportedLocale` choice to an actual {@link SupportedLocale} for rendering. */
export function resolveStoredLocale(stored: StoredLocale): SupportedLocale {
  return stored === "system" ? detectLocale(navigator.languages) : stored;
}

function isPlainRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/**
 * `base` with every leaf `overlay` carries laid over it, recursively. A
 * locale's `primevue.*` subtree is copied onto one shared, long-lived
 * PrimeVue config object, so the English defaults must be re-applied
 * underneath it on every switch: a locale with no (or a partial)
 * `primevue.*` must show English, never the previous locale's strings.
 */
function overlayMessages(
  base: Record<string, unknown>,
  overlay: Record<string, unknown> | undefined,
): Record<string, unknown> {
  const merged: Record<string, unknown> = { ...base };
  for (const [key, value] of Object.entries(overlay ?? {})) {
    const baseValue = merged[key];
    merged[key] =
      isPlainRecord(baseValue) && isPlainRecord(value) ? overlayMessages(baseValue, value) : value;
  }
  return merged;
}

/**
 * The one entry point for switching language, called from `main.ts` at
 * boot and from the Settings/Setup language picker. In order: loads the
 * locale's messages, sets the active vue-i18n locale (reactive —
 * everything rendered through `t()` re-renders with no reload), copies
 * that locale's vendored `primevue.*` subtree onto PrimeVue's own
 * reactive config (a locale with no translated `primevue.*` yet, i.e.
 * a preview locale, shows the English strings — an acceptable
 * fallback, not a missing-key bug), syncs
 * `document.documentElement.lang` (screen readers, and the CJK font
 * fallback), and persists the *stored* choice (`"system"` included, not
 * the resolved locale) so a later OS language change is followed again.
 */
export async function setAppLocale(stored: StoredLocale): Promise<void> {
  if (!appI18n) {
    throw new Error("setAppLocale called before registerAppI18n");
  }
  const resolved = resolveStoredLocale(stored);
  await loadLocale(appI18n, resolved);
  appI18n.global.locale.value = resolved;

  if (primeVueLocale) {
    const messages = appI18n.global.getLocaleMessage(resolved) as {
      primevue?: Record<string, unknown>;
    };
    Object.assign(primeVueLocale, overlayMessages(en.primevue, messages.primevue));
  }

  document.documentElement.lang = resolved;
  usePreferencesStore().setLocale(stored);
}

/**
 * Renders a {@link MessageDescriptor} outside any component's
 * `setup()` — `utils/toast.ts`'s global error/info toasts run from
 * Pinia Colada's own `mutationOptions.onError` and from `main.ts`'s
 * session-lost handler, neither of which has the injection context
 * `useI18n()` needs. Reads the *current* locale at call time (a toast
 * already on screen keeps its language and expires in a few seconds
 * anyway) rather than re-rendering on a later switch.
 */
export function translateMessage(message: MessageDescriptor): string {
  if (!appI18n) {
    throw new Error("translateMessage called before registerAppI18n");
  }
  return appI18n.global.t(message.key, message.params ?? {});
}
