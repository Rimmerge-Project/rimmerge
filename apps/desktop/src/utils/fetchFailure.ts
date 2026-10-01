import { descriptor, type MessageDescriptor } from "@/i18n/messageDescriptor";
import type { FetchFailureDto } from "@/types/generated/FetchFailureDto";
import { assertNever } from "@/utils/assertNever";

/**
 * The headline for one failed fetch from GitHub (an update check or a
 * rule-database refresh), keyed by its closed cause. The adapter's English
 * text (`failure.detail`) never appears in it: that is a technical-details
 * line, shown separately via {@link fetchFailureTechnicalDetail}. `locale`
 * formats a rate limit's retry instant in the app locale, never the host's.
 */
export function describeFetchFailure(failure: FetchFailureDto, locale: string): MessageDescriptor {
  const cause = failure.cause;
  switch (cause.kind) {
    case "transport":
      return descriptor("fetchFailure.transportHeadline");
    case "httpStatus":
      return descriptor("fetchFailure.httpStatus", { status: cause.status });
    case "rateLimited":
      return descriptor("fetchFailure.rateLimited", {
        until: new Date(cause.until).toLocaleString(locale),
      });
    case "notPublished":
      return descriptor("fetchFailure.notPublished");
    case "tooLarge":
      return descriptor("fetchFailure.tooLarge");
    case "invalidContent":
      return descriptor("fetchFailure.invalidContent");
    case "notAStableVersion":
      return descriptor("fetchFailure.notAStableVersion");
    case "notModifiedWithoutCache":
      return descriptor("fetchFailure.notModifiedWithoutCache");
    case "readFailed":
      return descriptor("fetchFailure.readFailedHeadline");
    case "cacheWriteFailed":
      return descriptor("fetchFailure.cacheWriteFailedHeadline");
    case "unclassified":
      return descriptor("fetchFailure.unclassifiedHeadline");
    default:
      return assertNever(cause);
  }
}

/**
 * The adapter's English text for the few causes where it adds information a
 * translation cannot (a transport or disk error message), `null` for every
 * cause whose headline already says it all. Render it as its own line next
 * to the headline, never inside it.
 */
export function fetchFailureTechnicalDetail(failure: FetchFailureDto): string | null {
  const cause = failure.cause;
  switch (cause.kind) {
    case "transport":
    case "readFailed":
    case "cacheWriteFailed":
    case "unclassified":
      return failure.detail === "" ? null : failure.detail;
    case "httpStatus":
    case "rateLimited":
    case "notPublished":
    case "tooLarge":
    case "invalidContent":
    case "notAStableVersion":
    case "notModifiedWithoutCache":
      return null;
    default:
      return assertNever(cause);
  }
}
