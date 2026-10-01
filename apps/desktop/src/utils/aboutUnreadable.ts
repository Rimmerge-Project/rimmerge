import { descriptor, type MessageDescriptor } from "@/i18n/messageDescriptor";
import type { AboutUnreadableCauseDto } from "@/types/generated/AboutUnreadableCauseDto";
import { assertNever } from "@/utils/assertNever";

/**
 * Why a mod's `About.xml` couldn't be shown, by closed cause. The underlying
 * error's English text is a technical-details line the panel renders
 * separately, never part of this headline. Exhaustive via {@link assertNever}.
 */
export function describeAboutUnreadable(cause: AboutUnreadableCauseDto): MessageDescriptor {
  switch (cause) {
    case "io":
      return descriptor("modInfo.panel.aboutUnreadableIoHeadline");
    case "xml":
      return descriptor("modInfo.panel.aboutUnreadableXmlHeadline");
    default:
      return assertNever(cause);
  }
}
