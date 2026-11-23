/** A matched part of a text, in UTF-16 code units (what `String.slice` counts); `end` exclusive. */
export type TextRange = { readonly start: number; readonly end: number };

/** One run of a text: marked as a match, or plain. */
export type TextSegment = { readonly text: string; readonly isMarked: boolean };

/** Whether `range` is a non-empty slice of `text` that keeps every surrogate pair whole. */
function isUsableRange(text: string, range: TextRange): boolean {
  const { start, end } = range;
  if (!Number.isInteger(start) || !Number.isInteger(end)) return false;
  if (start < 0 || end > text.length || start >= end) return false;
  return !isInsideSurrogatePair(text, start) && !isInsideSurrogatePair(text, end);
}

/** Whether a cut at `index` would fall between the two halves of a surrogate pair. */
function isInsideSurrogatePair(text: string, index: number): boolean {
  const before = text.charCodeAt(index - 1);
  const after = text.charCodeAt(index);
  return before >= 0xd800 && before <= 0xdbff && after >= 0xdc00 && after <= 0xdfff;
}

/**
 * Splits `text` into marked and plain runs, from the backend's match ranges. The runs always
 * concatenate back to `text`. Ranges arrive ascending and disjoint, but this does not rely on it:
 * they are sorted, and ranges that overlap are joined.
 *
 * A range that cannot be a slice of `text` (out of bounds, empty, not whole numbers, or cutting a
 * surrogate pair) is dropped, never clamped: a mark that is not exactly what the backend matched
 * would mislead, and no mark is only a missing hint. In development each dropped range is
 * reported with `console.warn`, since it means the backend and the text it labels disagree.
 */
export function splitMarked(text: string, ranges: readonly TextRange[]): TextSegment[] {
  const usable: TextRange[] = [];
  for (const range of ranges) {
    if (isUsableRange(text, range)) {
      usable.push(range);
    } else if (import.meta.env.DEV) {
      console.warn(
        `dropped match range [${range.start}, ${range.end}) for a text of ${text.length} code units`,
      );
    }
  }
  const joined: { start: number; end: number }[] = [];
  for (const range of usable.toSorted((left, right) => left.start - right.start)) {
    const previous = joined.at(-1);
    if (previous !== undefined && range.start < previous.end) {
      previous.end = Math.max(previous.end, range.end);
    } else {
      joined.push({ start: range.start, end: range.end });
    }
  }

  const segments: TextSegment[] = [];
  let cursor = 0;
  for (const { start, end } of joined) {
    if (start > cursor) segments.push({ text: text.slice(cursor, start), isMarked: false });
    segments.push({ text: text.slice(start, end), isMarked: true });
    cursor = end;
  }
  if (cursor < text.length) segments.push({ text: text.slice(cursor), isMarked: false });
  return segments;
}
