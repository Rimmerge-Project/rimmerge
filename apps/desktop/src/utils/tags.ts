/** A tag is a non-empty lowercase slug of `a-z`, `0-9`, `_` and `-`; the backend enforces it. */
const TAG_SLUG = /^[a-z0-9_-]+$/;

/** Whether `text` is a valid tag as typed (no trimming or rewriting: the backend decides too). */
export function isTagSlug(text: string): boolean {
  return TAG_SLUG.test(text);
}
