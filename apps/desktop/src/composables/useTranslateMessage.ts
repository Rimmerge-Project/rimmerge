import { useI18n } from "vue-i18n";

import { type MessageDescriptor, renderMessage } from "@/i18n/messageDescriptor";

/**
 * Renders a {@link MessageDescriptor} through this component's own
 * `useI18n()` composer — the template-friendly counterpart of calling
 * `t(key, params)` directly, for the common case where a helper already
 * handed back a descriptor (an exhaustive `switch`'s return value, a
 * `CommandErrorDescriptor` field, …) rather than a literal key. Reactive
 * to a locale switch exactly like `t()` itself, since it's built from
 * the same composer. A descriptor nested in another's `params` is rendered
 * too (see `renderMessage`).
 */
export function useTranslateMessage(): (message: MessageDescriptor) => string {
  const { t } = useI18n();
  return (message: MessageDescriptor): string => renderMessage(t, message);
}
