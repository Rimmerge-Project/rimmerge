// ApplyDialog: which paths the success toast lists.

import { clearMocks } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it, vi } from "vitest";
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

const toastAddSpy = vi.hoisted(() => vi.fn());
vi.mock("primevue/usetoast", () => ({ useToast: () => ({ add: toastAddSpy }) }));

function installApply(backupPath: string | null): void {
  installMockIpc({
    list_rules: EMPTY_RULE_SET,
    list_mod_names: {},
    get_pending_active_changes: EMPTY_PENDING_ACTIVE_CHANGES,
    get_dashboard: dashboardFixture(0),
    get_apply_preflight: preflightFixture(),
    get_merge_mod: emptyMergeModFixture(),
    get_def_cache_carrier: { carrierModId: null },
    list_order: [],
    apply: () => ({
      modsConfigPath: "C:/RimWorld/ModsConfig.xml",
      backupPath,
      decisionsPath: "C:/Profile/rimmerge/decisions.json",
      rulesPath: "C:/Profile/rimmerge/rules.json",
      mergeModPath: null,
      mergeModBackupPath: null,
      skippedMerges: [],
    }),
  });
}

describe("ApplyDialog success toast", () => {
  afterEach(() => {
    clearMocks();
    toastAddSpy.mockClear();
  });

  it("lists the ModsConfig and backup paths after writing ModsConfig.xml", async () => {
    installApply("C:/RimWorld/ModsConfig.xml.bak-x");
    const { wrapper } = mountDialog();
    await flush();

    await wrapper.get('[data-testid="apply-dialog-submit"]').trigger("click");
    await flush();

    const toast = toastAddSpy.mock.calls.at(-1)?.[0];
    expect(toast.summary).toBe("Applied");
    expect(toast.detail).toContain("ModsConfig: C:/RimWorld/ModsConfig.xml");
    expect(toast.detail).toContain("Backup: C:/RimWorld/ModsConfig.xml.bak-x");
  });

  it("does not mention ModsConfig.xml when only decisions and rules were saved", async () => {
    installApply(null);
    const { wrapper } = mountDialog();
    await flush();
    await wrapper
      .get('[data-testid="apply-dialog-write-modsconfig-checkbox"] input')
      .setValue(false);

    await wrapper.get('[data-testid="apply-dialog-submit"]').trigger("click");
    await flush();

    const toast = toastAddSpy.mock.calls.at(-1)?.[0];
    expect(toast.summary).toBe("Saved");
    expect(toast.detail).not.toContain("ModsConfig");
    expect(toast.detail).toContain("Decisions: C:/Profile/rimmerge/decisions.json");
  });
});
