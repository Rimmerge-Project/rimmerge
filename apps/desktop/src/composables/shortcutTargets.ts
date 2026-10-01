const TEXT_ENTRY_TAGS = new Set(["INPUT", "TEXTAREA", "SELECT"]);
const NATIVE_ENTER_TAGS = new Set(["BUTTON", "A"]);

/**
 * Whether `target` is a place a keystroke should be typed or clicked, not
 * interpreted as a keyboard shortcut: a text-entry control, a
 * `role="combobox"` (native `<select>`-like widgets included), inside a
 * modal `role="dialog"`, or `contentEditable`. Shared by every `useXKeys`
 * composable (`useInboxKeys`, `useMergeKeys`) so the inert rules stay in
 * one place. Deliberately does *not* include `BUTTON`/`A` — those have no
 * letter-key behavior of their own to protect; see {@link ownsEnterNatively}
 * for the one case (Enter/Space) that does need its own button/link check.
 */
export function isInert(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) {
    return false;
  }
  if (target.closest('[role="dialog"]')) {
    return true;
  }
  if (TEXT_ENTRY_TAGS.has(target.tagName)) {
    return true;
  }
  if (target.isContentEditable) {
    return true;
  }
  return target.getAttribute("role") === "combobox";
}

/**
 * Whether `target` sits inside a `role="radiogroup"`, which moves its own selection on the
 * arrow keys. Only the arrow-key shortcuts defer to it: `j`/`k` and the rest stay live after
 * a click on a radio (the shell's order and names/ids switches), so this is deliberately not
 * part of {@link isInert}.
 */
export function ownsArrowKeys(target: EventTarget | null): boolean {
  return target instanceof HTMLElement && target.closest('[role="radiogroup"]') !== null;
}

/**
 * Whether `target` already has its own native Enter/Space activation (a
 * button or a link) — firing a shortcut's own "activate this" action on
 * top of that would double-fire it. Callers gate only their
 * activation-shaped shortcut(s) on this, never every shortcut: `j`/`k`/
 * digits/etc. have no button/link behavior to conflict with, and
 * blocking them here would leave every shortcut dead the moment focus
 * lands on, say, a just-clicked nav link or a row's own action button.
 */
export function ownsEnterNatively(target: EventTarget | null): boolean {
  return target instanceof HTMLElement && NATIVE_ENTER_TAGS.has(target.tagName);
}
