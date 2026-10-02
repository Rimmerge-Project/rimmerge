import { describe, expect, it } from "vitest";

import { RimmergeError } from "@/services/ipc";
import { describeCommandError, describeFailure } from "@/utils/errors";

describe("describeFailure", () => {
  it("renders a RimmergeError's own message, ignoring the fallback", () => {
    const error = new RimmergeError({ code: "profile_io_failed", message: "disk is full" });

    expect(describeFailure(error, "Something went wrong.")).toBe("disk is full");
  });

  it("renders the fallback for a rejection that never reached the IPC boundary", () => {
    expect(describeFailure(new Error("boom"), "Failed to save this row.")).toBe(
      "Failed to save this row.",
    );
    expect(describeFailure("not even an Error", "Failed to save this row.")).toBe(
      "Failed to save this row.",
    );
    expect(describeFailure(null, "Failed to save this row.")).toBe("Failed to save this row.");
  });
});

describe("describeCommandError", () => {
  it("maps a RimmergeError to its own code's title/detail keys, keeping the raw message as technical detail", () => {
    const error = new RimmergeError({
      code: "rimworld_running",
      message: "RimWorldWin64.exe (pid 1234)",
    });

    expect(describeCommandError(error)).toEqual({
      title: { key: "error.code.rimworld_running.title" },
      detail: { key: "error.code.rimworld_running.detail" },
      technicalDetail: "RimWorldWin64.exe (pid 1234)",
    });
  });

  it("maps every CommandErrorCode to a distinct key pair", () => {
    const codes = [
      "no_project_loaded",
      "session_lost",
      "internal",
      "invalid_input",
      "scan_failed",
      "mods_config_io_failed",
      "profile_io_failed",
      "rimsort_import_failed",
      "rimworld_running",
      "mod_not_found",
      "finding_not_found",
      "merge_source_failed",
      "merge_mod_io_failed",
      "patch_not_found",
      "patch_identity_invalid",
      "def_not_found",
      "assignment_not_found",
      "assignment_section_in_use",
      "stale_active_set",
      "texture_unsupported_format",
      "already_running",
      "recommended_rules_unavailable",
      "app_settings_damaged",
    ] as const;

    const keys = codes.map((code) => {
      const described = describeCommandError(new RimmergeError({ code, message: "x" }));
      return `${described.title.key}|${described.detail.key}`;
    });

    expect(new Set(keys).size).toBe(codes.length);
  });

  it("words a refused recommended-rules click by which network gate was closed", () => {
    const describeReason = (reason: "networkOff" | "awaitingFirstRun") =>
      describeCommandError(
        new RimmergeError({
          code: "recommended_rules_unavailable",
          message: "x",
          detail: { kind: "recommendedRulesUnavailable", reason },
        }),
      );

    expect(describeReason("networkOff").detail.key).toBe(
      "error.code.recommended_rules_unavailable.networkOff",
    );
    expect(describeReason("awaitingFirstRun").detail.key).toBe(
      "error.code.recommended_rules_unavailable.awaitingFirstRun",
    );
    expect(describeReason("networkOff").title.key).toBe(
      "error.code.recommended_rules_unavailable.title",
    );
  });

  it("keeps the code's own sentence when a refused click arrives without its detail", () => {
    const described = describeCommandError(
      new RimmergeError({ code: "recommended_rules_unavailable", message: "x" }),
    );

    expect(described.detail.key).toBe("error.code.recommended_rules_unavailable.detail");
  });

  it("falls back to the generic pair, with no technical detail, for a rejection that never reached the IPC boundary", () => {
    expect(describeCommandError(new Error("boom"))).toEqual({
      title: { key: "error.generic.title" },
      detail: { key: "error.generic.detail" },
      technicalDetail: null,
    });
    expect(describeCommandError("not even an Error").technicalDetail).toBeNull();
    expect(describeCommandError(null).technicalDetail).toBeNull();
  });
});
