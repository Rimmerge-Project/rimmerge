import { descriptor, type MessageDescriptor } from "@/i18n/messageDescriptor";
import type { ScanStageDto } from "@/types/generated/ScanStageDto";
import { assertNever } from "@/utils/assertNever";

/**
 * The caption for one scan phase under the load/rescan progress bar.
 * Exhaustive via {@link assertNever}; returns a {@link MessageDescriptor},
 * rendered through `t()`/`useTranslateMessage()`, never as text.
 */
export function scanStageLabel(stage: ScanStageDto): MessageDescriptor {
  switch (stage) {
    case "discovering":
      return descriptor("setup.stage.discovering");
    case "scanning":
      return descriptor("setup.stage.scanning");
    case "analyzing":
      return descriptor("setup.stage.analyzing");
    case "collectingTagEvidence":
      return descriptor("setup.stage.collectingTagEvidence");
    case "done":
      return descriptor("setup.stage.done");
    default:
      return assertNever(stage);
  }
}
