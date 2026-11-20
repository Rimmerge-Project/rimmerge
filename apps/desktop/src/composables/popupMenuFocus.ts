/**
 * Keyboard help for a PrimeVue popup `Menu`.
 *
 * The popup moves focus to its list but marks no item, so Enter does nothing until an arrow key is
 * pressed. A menu opened from the keyboard asks for the first item to be marked.
 */

const MENU_SELECTOR = '[data-pc-name="menu"]';

/**
 * Whether `event` opened a menu from the keyboard: a key (Shift+F10, the ContextMenu key) or the
 * click a key press on a button synthesizes (`detail` 0, where a pointer click has 1 or more).
 */
export function isKeyboardOpen(event: Event): boolean {
  // A `contextmenu` event (a right click) also reports `detail` 0, so only a click counts.
  return (
    event instanceof KeyboardEvent ||
    (event instanceof MouseEvent && event.type === "click" && event.detail === 0)
  );
}

/**
 * Marks the first enabled item of the popup menu that now has focus, by sending the ArrowDown the
 * menu itself would act on (it reads `event.code`, so both `key` and `code` are set). Call it from
 * the menu's `show` event.
 */
export function markFirstMenuItem(): void {
  const focused = document.activeElement;
  // Focus may have moved on (the menu closed, a dialog opened): the key must not reach the page.
  if (focused === null || !isInsideMenu(focused)) {
    return;
  }
  const overlay = focused.closest(MENU_SELECTOR);
  const arrowDown = new KeyboardEvent("keydown", {
    key: "ArrowDown",
    code: "ArrowDown",
    bubbles: true,
    cancelable: true,
  });
  // The menu's own listener sits below the overlay and has run by the time the event reaches it;
  // stopping it there keeps a key nobody pressed from reaching row, tree and window listeners.
  const stop = (event: Event): void => event.stopPropagation();
  overlay?.addEventListener("keydown", stop);
  try {
    focused.dispatchEvent(arrowDown);
  } finally {
    overlay?.removeEventListener("keydown", stop);
  }
}

/** Whether `target` is inside a PrimeVue menu overlay, which owns its own Escape. */
export function isInsideMenu(target: EventTarget | null): boolean {
  return target instanceof Element && target.closest(MENU_SELECTOR) !== null;
}
