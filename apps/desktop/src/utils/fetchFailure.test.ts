import { describe, expect, it } from "vitest";

import type { FetchFailureCauseDto } from "@/types/generated/FetchFailureCauseDto";
import { describeFetchFailure, fetchFailureTechnicalDetail } from "@/utils/fetchFailure";

function failure(cause: FetchFailureCauseDto) {
  return { cause, detail: "english detail" };
}

describe("describeFetchFailure", () => {
  it.each<[FetchFailureCauseDto, string]>([
    [{ kind: "transport" }, "fetchFailure.transportHeadline"],
    [{ kind: "httpStatus", status: 503 }, "fetchFailure.httpStatus"],
    [{ kind: "rateLimited", until: "2026-01-01T00:00:00Z" }, "fetchFailure.rateLimited"],
    [{ kind: "notPublished" }, "fetchFailure.notPublished"],
    [{ kind: "tooLarge" }, "fetchFailure.tooLarge"],
    [{ kind: "invalidContent" }, "fetchFailure.invalidContent"],
    [{ kind: "notAStableVersion" }, "fetchFailure.notAStableVersion"],
    [{ kind: "notModifiedWithoutCache" }, "fetchFailure.notModifiedWithoutCache"],
    [{ kind: "readFailed" }, "fetchFailure.readFailedHeadline"],
    [{ kind: "cacheWriteFailed" }, "fetchFailure.cacheWriteFailedHeadline"],
    [{ kind: "unclassified" }, "fetchFailure.unclassifiedHeadline"],
  ])("maps %j to its own key", (cause, key) => {
    expect(describeFetchFailure(failure(cause), "en").key).toBe(key);
  });

  it("carries the HTTP status as a param", () => {
    const message = describeFetchFailure(failure({ kind: "httpStatus", status: 503 }), "en");
    expect(message.params).toEqual({ status: 503 });
  });

  it("formats the rate-limit instant in the app locale", () => {
    const until = "2026-01-01T00:00:00Z";
    const message = describeFetchFailure(failure({ kind: "rateLimited", until }), "pt-BR");
    expect(message.params).toEqual({ until: new Date(until).toLocaleString("pt-BR") });
  });

  it("keeps the English detail out of every headline", () => {
    for (const kind of ["transport", "readFailed", "cacheWriteFailed", "unclassified"] as const) {
      expect(describeFetchFailure(failure({ kind }), "en").params).toBeUndefined();
    }
  });
});

describe("fetchFailureTechnicalDetail", () => {
  it.each<FetchFailureCauseDto>([
    { kind: "transport" },
    { kind: "readFailed" },
    { kind: "cacheWriteFailed" },
    { kind: "unclassified" },
  ])("returns the English detail for %j", (cause) => {
    expect(fetchFailureTechnicalDetail(failure(cause))).toBe("english detail");
  });

  it.each<FetchFailureCauseDto>([
    { kind: "httpStatus", status: 503 },
    { kind: "rateLimited", until: "2026-01-01T00:00:00Z" },
    { kind: "notPublished" },
    { kind: "tooLarge" },
    { kind: "invalidContent" },
    { kind: "notAStableVersion" },
    { kind: "notModifiedWithoutCache" },
  ])("returns null when the headline already says it all (%j)", (cause) => {
    expect(fetchFailureTechnicalDetail(failure(cause))).toBeNull();
  });

  it("returns null for an empty detail", () => {
    expect(fetchFailureTechnicalDetail({ cause: { kind: "transport" }, detail: "" })).toBeNull();
  });
});
