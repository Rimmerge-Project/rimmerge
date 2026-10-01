/**
 * A finding's canonical, rerere-style key text (see
 * `rim_resolve::domain::FindingKey`'s `Display`/`FromStr` impls on the
 * Rust side). Crosses IPC as a plain string — this brand exists only so
 * TypeScript catches passing an arbitrary string where a real finding key
 * is expected; construct one only from a value the backend actually gave
 * you (a `ResolutionSummaryDto.key`/`ResolutionDetailDto.key`), never by
 * hand.
 */
export type FindingKey = string & { readonly __brand: "FindingKey" };

/**
 * Brands a raw string as a {@link FindingKey}. Only for wrapping a key
 * string that already came from the backend (a DTO's own `key` field) —
 * never call this on user input.
 */
export function asFindingKey(raw: string): FindingKey {
  return raw as FindingKey;
}

/**
 * One field or list item's canonical path text inside a contested def's
 * resolved XML tree (see `rim_resolve::domain::FieldPath`'s `Display`/
 * `FromStr` impls on the Rust side). Crosses IPC as a plain string, in a
 * `MergeFieldDto.path` or as a `SetMergeChoicesRequestDto.choices` key —
 * this brand exists only so TypeScript catches passing an arbitrary
 * string where a real field path is expected; construct one only from a
 * value the backend actually gave you, never by hand.
 */
export type FieldPath = string & { readonly __brand: "FieldPath" };

/**
 * Brands a raw string as a {@link FieldPath}. Only for wrapping a path
 * string that already came from the backend (a `MergeFieldDto.path`) —
 * never call this on user input.
 */
export function asFieldPath(raw: string): FieldPath {
  return raw as FieldPath;
}

/**
 * The address of one def or `Name`-attributed template (see
 * `rim_resolve::domain::DefRef`'s `Display`/`FromStr` impls on the Rust
 * side). Crosses IPC as a plain string, and as the `defs/:defRef` route's
 * own param — it can contain a literal `/` (e.g. `ThingDef/Wall`), which
 * `vue-router`'s own param encoder (`encodeParam`) already percent-encodes
 * and decodes for a dynamic segment, so building a link from one needs no
 * manual `encodeURIComponent`. This brand exists only so TypeScript
 * catches passing an arbitrary string where a real def ref is expected;
 * construct one only from a value the backend actually gave you, never by
 * hand.
 */
export type DefRef = string & { readonly __brand: "DefRef" };

/**
 * Brands a raw string as a {@link DefRef}. Only for wrapping a ref string
 * that already came from the backend (a DTO's own `defRef` field) — never
 * call this on user input.
 */
export function asDefRef(raw: string): DefRef {
  return raw as DefRef;
}

/**
 * The {@link DefRef} of a def addressed by its type and name (`ThingDef/Wall`) — the one
 * place the `type/name` text is built on this side, from a DTO's own `defType`/`defName`.
 */
export function defRefOf(defType: string, defName: string): DefRef {
  return asDefRef(`${defType}/${defName}`);
}
