/**
 * A translated string, described rather than rendered. Helpers in
 * `utils/`/`composables/` return this instead of English text so a
 * component renders it through `t()` at the point it actually knows the
 * active locale — the same value re-renders correctly after a language
 * switch, with no cached English baked into a query result or a store.
 *
 * `key` stays a plain `string` rather than a hand-rolled path-literal
 * type: every producer of a `MessageDescriptor` is an exhaustive
 * `switch`/lookup table ending in `assertNever`, so a key is always a
 * source-literal already checked against its DTO's variants, and
 * `i18n/locales.test.ts`'s unused-key check plus `vue-tsc` (via
 * `schema.d.ts`'s `DefineLocaleMessage` augmentation, which types the
 * `t()` call site itself) together catch a typo or a stale key —
 * duplicating that as a second, hand-maintained path-union type here
 * would be a parallel system for the same guarantee.
 *
 * `count`, when present, is the plural selector for vue-i18n's own
 * `t(key, named, count)` overload — but only as a **fallback**:
 * `@intlify/core-base`'s `getPluralIndex` reads `named.count` (then
 * `named.n`) first if either is set, and only falls back to this
 * explicit third argument when neither is. Since `params.count` is
 * almost always set alongside it anyway (for the message's own `{count}`
 * placeholder), the explicit `count` here is mostly redundant in
 * practice — set both regardless, so a descriptor still pluralizes
 * correctly on the rare message that carries a count with no matching
 * `{count}` placeholder in its own text.
 */
export type MessageDescriptor = {
  readonly key: string;
  readonly params?: Readonly<Record<string, unknown>>;
  readonly count?: number;
};

/** The `t()` shape {@link renderMessage} needs — a caller's own `useI18n().t`. */
export type Translate = (key: string, params?: Record<string, unknown>, count?: number) => string;

/**
 * Marks a value as built by {@link descriptor}. A plain `{ key: string }`
 * object can legitimately appear as a message parameter (any DTO with a
 * string `key` field), so shape alone must never decide that a param is a
 * phrase to translate. The tag is a non-enumerable symbol property: it
 * survives Vue's reactive proxies and is invisible to `toEqual`, spreads and
 * serialization.
 */
const DESCRIPTOR_TAG = Symbol("MessageDescriptor");

function isMessageDescriptor(value: unknown): value is MessageDescriptor {
  return typeof value === "object" && value !== null && DESCRIPTOR_TAG in value;
}

/**
 * Renders a {@link MessageDescriptor} with `t`. A param that is itself a
 * descriptor is rendered first and substituted as text, so a sentence can
 * embed another translated phrase (a failure sentence inside "Last check
 * failed: {reason}") while each stays one whole, reorderable translation.
 */
export function renderMessage(t: Translate, message: MessageDescriptor): string {
  const params: Record<string, unknown> = {};
  for (const [name, value] of Object.entries(message.params ?? {})) {
    params[name] = isMessageDescriptor(value) ? renderMessage(t, value) : value;
  }
  return message.count === undefined
    ? t(message.key, params)
    : t(message.key, params, message.count);
}

/** Builds a {@link MessageDescriptor} — reads as `descriptor(key, params, count)` at call sites instead of an inline object literal. */
export function descriptor(
  key: string,
  params?: Readonly<Record<string, unknown>>,
  count?: number,
): MessageDescriptor {
  const built: MessageDescriptor = {
    key,
    ...(params !== undefined ? { params } : {}),
    ...(count !== undefined ? { count } : {}),
  };
  Object.defineProperty(built, DESCRIPTOR_TAG, { value: true });
  return built;
}
