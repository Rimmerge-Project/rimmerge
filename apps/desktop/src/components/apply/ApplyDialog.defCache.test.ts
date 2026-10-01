// ApplyDialog: the def-cache note.

import { clearMocks } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it } from "vitest";
import {
  dashboardFixture,
  EMPTY_PENDING_ACTIVE_CHANGES,
  EMPTY_RULE_SET,
  emptyMergeModFixture,
  flush,
  mountDialog,
  preflightFixture,
} from "@/components/apply/ApplyDialog.test-support";
import { installMockIpc } from "@/services/ipc.mock";
import { gameLogSummary, snapshotCoverage } from "@/utils/gameLogSummary.test-support";

describe("ApplyDialog", () => {
  afterEach(() => {
    clearMocks();
  });

  describe("def-cache note", () => {
    it("does not appear when no active mod is a def-cache carrier", async () => {
      installMockIpc({
        list_rules: EMPTY_RULE_SET,
        list_mod_names: {},
        get_pending_active_changes: EMPTY_PENDING_ACTIVE_CHANGES,
        get_dashboard: dashboardFixture(0),
        get_apply_preflight: preflightFixture(),
        get_merge_mod: emptyMergeModFixture(),
        get_def_cache_carrier: { carrierModId: null },
        list_order: [],
      });
      const { wrapper } = mountDialog();
      await flush();
      await wrapper.vm.$nextTick();

      expect(wrapper.find('[data-testid="apply-dialog-def-cache-note"]').exists()).toBe(false);
    });

    it("appears, with no duration parenthetical, when a carrier is active but no log has been imported", async () => {
      installMockIpc({
        list_rules: EMPTY_RULE_SET,
        list_mod_names: {},
        get_pending_active_changes: EMPTY_PENDING_ACTIVE_CHANGES,
        get_dashboard: dashboardFixture(0),
        get_apply_preflight: preflightFixture(),
        get_merge_mod: emptyMergeModFixture(),
        get_def_cache_carrier: { carrierModId: "fixture.carriermod" },
      });
      const { wrapper } = mountDialog();
      await flush();
      await wrapper.vm.$nextTick();

      const note = wrapper.get('[data-testid="apply-dialog-def-cache-note"]').text();
      expect(note).toContain("def-cache mod keys its cache on the load order");
      expect(note).not.toContain("seconds");
    });

    it("includes the observed duration when the last imported log's own def-cache timer has one", async () => {
      installMockIpc({
        list_rules: EMPTY_RULE_SET,
        list_mod_names: {},
        get_pending_active_changes: EMPTY_PENDING_ACTIVE_CHANGES,
        get_dashboard: dashboardFixture(0),
        get_apply_preflight: preflightFixture(),
        get_merge_mod: emptyMergeModFixture(),
        get_def_cache_carrier: { carrierModId: "fixture.carriermod" },
      });
      const { wrapper, session } = mountDialog();
      await flush();
      await wrapper.vm.$nextTick();
      session.setGameLogSummary({
        patchFailures: [],
        extraStackTraces: [],
        crossReferences: [],
        ddsFailures: [],
        dependencyWarnings: [],
        timers: [],
        defCacheLines: [
          "DEFCACHE: <color=white>Cache created!</color> creating cache took <color=green>8 seconds</color>",
        ],
        loadEvents: [],
        readStats: {
          linesRead: 0,
          linesTruncated: 0,
          linesWithInvalidUtf8: 0,
          stackBlockLinesDropped: 0,
          stackJoinsAbandoned: 0,
          passesFolded: 0,
          stackRefsDropped: 0,
          loggingGapsDropped: 0,
          crashReportPathsTruncated: 0,
        },
        coverage: {
          kind: "playerLog",
          passes: 2,
          patchPhase: "noneLogged",
          endState: { kind: "cleanExit" },
        },
        loggingGaps: [],
        lowerBound: null,
      });
      await wrapper.vm.$nextTick();

      expect(wrapper.get('[data-testid="apply-dialog-def-cache-note"]').text()).toContain(
        "about 8 seconds on your last log",
      );
    });

    it("leaves the duration out when the imported log is a console snapshot, even one that quotes a build timer", async () => {
      installMockIpc({
        list_rules: EMPTY_RULE_SET,
        list_mod_names: {},
        get_pending_active_changes: EMPTY_PENDING_ACTIVE_CHANGES,
        get_dashboard: dashboardFixture(0),
        get_apply_preflight: preflightFixture(),
        get_merge_mod: emptyMergeModFixture(),
        get_def_cache_carrier: { carrierModId: "fixture.carriermod" },
      });
      const { wrapper, session } = mountDialog();
      await flush();
      await wrapper.vm.$nextTick();
      session.setGameLogSummary(
        gameLogSummary({
          coverage: snapshotCoverage(1000),
          defCacheLines: [
            "DEFCACHE: <color=white>Cache created!</color> creating cache took <color=green>8 seconds</color>",
          ],
        }),
      );
      await wrapper.vm.$nextTick();

      const note = wrapper.get('[data-testid="apply-dialog-def-cache-note"]').text();
      expect(note).toContain("def-cache mod keys its cache on the load order");
      expect(note).not.toContain("seconds");
    });
  });
});
