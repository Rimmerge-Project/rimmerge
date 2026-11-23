import { computed } from "vue";
import { useI18n } from "vue-i18n";

import { useTranslateMessage } from "@/composables/useTranslateMessage";
import { descriptor } from "@/i18n/messageDescriptor";
import type { ModListRowDto } from "@/types/generated/ModListRowDto";
import { updateUnavailableLabel } from "@/utils/modListSort";

/**
 * A mod row's last-updated cell text: its time as a date in the app language, or the reason it has
 * none. One formatter serves the whole table, not one per rendered row.
 */
export function useUpdatedText(): (mod: ModListRowDto) => string {
  const { locale } = useI18n();
  const tm = useTranslateMessage();
  const dateFormat = computed(() => new Intl.DateTimeFormat(locale.value, { dateStyle: "medium" }));

  return (mod) => {
    const updated = mod.updated;
    if (updated.kind === "known") {
      const date = new Date(updated.seconds * 1000);
      // A date past the JS range makes `format` throw during render, which would
      // blank the whole list; such a time reads as unknown instead.
      if (Number.isFinite(date.getTime())) {
        return dateFormat.value.format(date);
      }
      return tm(descriptor("mods.sort.updatedUnavailable.unknown"));
    }
    const reason = updateUnavailableLabel(updated);
    return reason === null ? "" : tm(reason);
  };
}
