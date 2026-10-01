import { ref } from "vue";

/** The inbox's `?` shortcut-help overlay: a simple shared visibility toggle. */
export function useShortcutHelp() {
  const visible = ref(false);

  function toggle(): void {
    visible.value = !visible.value;
  }

  function close(): void {
    visible.value = false;
  }

  return { visible, toggle, close };
}
