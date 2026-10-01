/**
 * Shared locale-file mechanics: flattening a nested JSON catalogue into
 * dotted-path leaves, and pulling the named placeholders out of a
 * message string. Used by `locales.test.ts` (parity/placeholder/plural
 * checks) and, later, `scripts/i18n-status.mjs` (L3) — kept here rather
 * than duplicated so both read the catalogue the identical way.
 */

/** A locale catalogue flattened to `"a.b.c" -> "message"`. */
export type FlatMessages = ReadonlyMap<string, string>;

/**
 * A leaf key whose last path segment starts with `_` — metadata about
 * the file (e.g. `_pending`, the "this locale is a preview" marker),
 * never a translatable UI string. Excluded from parity, placeholder,
 * plural-form and unused-key checks, but still required to be a plain
 * string (checked structurally the same as any other leaf).
 */
export function isMetadataKey(path: string): boolean {
  const lastSegment = path.split(".").pop();
  return lastSegment?.startsWith("_") ?? false;
}

/**
 * Flattens a locale JSON module into dotted-path leaves. Throws on a
 * malformed shape — an array, or a leaf that isn't a string — so a
 * structurally broken locale file fails immediately and specifically,
 * rather than surfacing as a confusing downstream parity mismatch.
 */
export function flattenMessages(source: unknown, prefix = ""): FlatMessages {
  const result = new Map<string, string>();
  if (typeof source !== "object" || source === null || Array.isArray(source)) {
    throw new Error(`malformed locale value at "${prefix || "<root>"}": expected an object`);
  }
  for (const [key, value] of Object.entries(source)) {
    const path = prefix ? `${prefix}.${key}` : key;
    if (typeof value === "string") {
      result.set(path, value);
    } else if (typeof value === "object" && value !== null && !Array.isArray(value)) {
      for (const [nestedPath, nestedValue] of flattenMessages(value, path)) {
        result.set(nestedPath, nestedValue);
      }
    } else {
      throw new Error(`malformed locale value at "${path}": expected a string or a nested object`);
    }
  }
  return result;
}

const PLACEHOLDER_PATTERN = /\{(\w+)\}/gu;

/** The set of named placeholders (`{after}`, `{count}`) a message string interpolates — positional placeholders are never used in this catalogue. */
export function placeholdersOf(message: string): ReadonlySet<string> {
  return new Set(Array.from(message.matchAll(PLACEHOLDER_PATTERN), (match) => match[1] as string));
}

/** Structural equality for two placeholder sets. */
export function areSetsEqual(a: ReadonlySet<string>, b: ReadonlySet<string>): boolean {
  return a.size === b.size && [...a].every((value) => b.has(value));
}

/**
 * Finds an object key repeated at the same nesting level in raw JSON
 * `text` — `JSON.parse` silently keeps only the *last* occurrence, so a
 * locale file that reopens one top-level namespace object across two
 * separate edits (translators will do this too, not just this repo's
 * own agents) can lose an earlier batch's keys with no error anywhere.
 * A small bracket-aware scanner over the raw text, not `JSON.parse`,
 * since `JSON.parse` can't see what it already discarded. Returns each
 * duplicate's full dotted path (e.g. `"inbox.evidence.danglingDefReference"`),
 * one entry per repeat beyond the first.
 *
 * Scoped to what this catalogue actually contains — nested objects and
 * string leaves only, so a value's own scanner just skips to the next
 * `,`/`}`/`]` without needing to parse numbers/booleans/arrays
 * correctly; a value already outside that shape is caught instead, more
 * specifically, by {@link flattenMessages}'s own structural check.
 */
export function findDuplicateKeys(text: string): string[] {
  const duplicates: string[] = [];
  let i = 0;
  const n = text.length;

  function skipWhitespace(): void {
    while (i < n && /\s/.test(text[i] ?? "")) {
      i++;
    }
  }

  function parseStringLiteral(): string {
    // Assumes `text[i] === '"'`.
    i++;
    let out = "";
    while (i < n && text[i] !== '"') {
      if (text[i] === "\\") {
        out += text[i] + (text[i + 1] ?? "");
        i += 2;
      } else {
        out += text[i];
        i++;
      }
    }
    i++; // closing quote
    return out;
  }

  function skipValue(path: string): void {
    skipWhitespace();
    if (text[i] === "{") {
      parseObject(path);
      return;
    }
    if (text[i] === '"') {
      parseStringLiteral();
      return;
    }
    // A malformed value (array/number/bool/null) — not this function's
    // job to validate; just skip past it so scanning can continue.
    while (i < n && !",}]".includes(text[i] ?? "")) {
      i++;
    }
  }

  function parseObject(path: string): void {
    i++; // "{"
    const seenKeys = new Set<string>();
    skipWhitespace();
    if (text[i] === "}") {
      i++;
      return;
    }
    while (i < n) {
      skipWhitespace();
      const key = parseStringLiteral();
      const fullPath = path ? `${path}.${key}` : key;
      if (seenKeys.has(key)) {
        duplicates.push(fullPath);
      }
      seenKeys.add(key);
      skipWhitespace();
      i++; // ":"
      skipValue(fullPath);
      skipWhitespace();
      if (text[i] === ",") {
        i++;
        continue;
      }
      if (text[i] === "}") {
        i++;
        break;
      }
      break;
    }
  }

  skipWhitespace();
  if (text[i] === "{") {
    parseObject("");
  }
  return duplicates;
}
