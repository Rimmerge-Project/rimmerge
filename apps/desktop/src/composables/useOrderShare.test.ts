import { PiniaColada, type PiniaColadaOptions } from "@pinia/colada";
import { clearMocks } from "@tauri-apps/api/mocks";
import { mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import PrimeVue from "primevue/config";
import type { ToastServiceMethods } from "primevue/toastservice";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { defineComponent } from "vue";

import { flush } from "@/components/apply/ApplyDialog.test-support";
import { useOrderShare } from "@/composables/useOrderShare";
import { createAppI18n, registerAppI18n } from "@/i18n/i18n";
import { installMockIpc } from "@/services/ipc.mock";
import { useSessionStore } from "@/stores/session";
import type { OrderImportOutcomeDto } from "@/types/generated/OrderImportOutcomeDto";
import { setToastService, toastError } from "@/utils/toast";

const { toastAdd, pickSave, pickOpen } = vi.hoisted(() => ({
  toastAdd: vi.fn(),
  pickSave: vi.fn(),
  pickOpen: vi.fn(),
}));
vi.mock("primevue/usetoast", () => ({ useToast: () => ({ add: toastAdd }) }));
vi.mock("@/services/dialogs", () => ({
  pickModListSavePath: pickSave,
  pickModListFile: pickOpen,
}));

const READY: OrderImportOutcomeDto = {
  kind: "ready",
  preview: {
    listed: 1,
    order: ["ludeon.rimworld"],
    entries: [{ kind: "alreadyActive", id: "ludeon.rimworld", name: "Core" }],
    deactivated: [],
    moved: 0,
    core: "listed",
    version: { kind: "same" },
    replacesPendingChanges: false,
    skipped: [],
    omittedSkipped: 0,
    importBlocked: null,
  },
};

type Calls = {
  exportFile: unknown[];
  importOrder: unknown[];
  workshop: unknown[];
};

function mountComposable(handlers: Record<string, unknown> = {}, colada: PiniaColadaOptions = {}) {
  const calls: Calls = { exportFile: [], importOrder: [], workshop: [] };
  installMockIpc({
    suggested_mod_list_path: "C:/Saves/ModLists/rimmerge-load-order.rml",
    export_order_file: (payload: unknown) => {
      calls.exportFile.push((payload as { request: unknown }).request);
      return { count: 2, unrepresentable: [] };
    },
    export_order_text: { text: "1. Core [ludeon.rimworld]", count: 1, unrepresentable: [] },
    preview_order_import_text: READY,
    preview_order_import_file: READY,
    import_order: (payload: unknown) => {
      calls.importOrder.push((payload as { request: unknown }).request);
      return {
        modCount: 1,
        gameVersion: "1.6.4871",
        elapsedMs: 1,
        warnings: [],
        ruleWarnings: [],
        selected: "current",
      };
    },
    open_workshop_page: (payload: unknown) => {
      calls.workshop.push((payload as { request: unknown }).request);
      return null;
    },
    ...handlers,
  });
  const pinia = createPinia();
  let share!: ReturnType<typeof useOrderShare>;
  mount(
    defineComponent({
      setup() {
        share = useOrderShare();
        return () => null;
      },
    }),
    { global: { plugins: [pinia, [PiniaColada, colada], PrimeVue] } },
  );
  return { share, calls, session: useSessionStore(pinia) };
}

describe("useOrderShare", () => {
  beforeEach(() => {
    toastAdd.mockClear();
    pickSave.mockReset();
    pickOpen.mockReset();
    Object.defineProperty(navigator, "clipboard", {
      value: { writeText: vi.fn().mockResolvedValue(undefined) },
      configurable: true,
    });
  });
  afterEach(() => {
    clearMocks();
  });

  it("saves to the chosen path, starting the dialog at RimWorld's ModLists folder", async () => {
    const { share, calls } = mountComposable();
    pickSave.mockResolvedValue("C:/Saves/ModLists/mine.rml");

    await share.saveAsFile();

    expect(pickSave).toHaveBeenCalledWith({
      filterName: "RimWorld mod list",
      defaultPath: "C:/Saves/ModLists/rimmerge-load-order.rml",
    });
    expect(calls.exportFile).toEqual([{ path: "C:/Saves/ModLists/mine.rml" }]);
    expect(toastAdd.mock.calls.at(-1)?.[0].summary).toBe("Saved 2 mods to mine.rml.");
  });

  it("writes nothing when the save dialog is cancelled", async () => {
    const { share, calls } = mountComposable();
    pickSave.mockResolvedValue(null);

    await share.saveAsFile();

    expect(calls.exportFile).toEqual([]);
    expect(toastAdd).not.toHaveBeenCalled();
  });

  it("words a refused export by its action and its code", async () => {
    const { share } = mountComposable({
      export_order_file: () => {
        throw { code: "nothing_to_export", message: "no active mods" };
      },
    });
    pickSave.mockResolvedValue("C:/Saves/mine.rml");

    await share.saveAsFile();

    const toast = toastAdd.mock.calls.at(-1)?.[0];
    expect(toast.severity).toBe("error");
    expect(toast.summary).toBe("Couldn't save the mod list.");
    expect(toast.detail).toContain("Nothing to export");
  });

  it("copies the text to the clipboard and says how many mods", async () => {
    const { share } = mountComposable();

    await share.copyAsText();

    expect(navigator.clipboard.writeText).toHaveBeenCalledWith("1. Core [ludeon.rimworld]");
    expect(toastAdd.mock.calls.at(-1)?.[0].summary).toBe("Copied 1 mod.");
  });

  it("says so when the clipboard is blocked, and never claims a copy", async () => {
    const { share } = mountComposable();
    vi.mocked(navigator.clipboard.writeText).mockRejectedValue(new Error("blocked"));

    await share.copyAsText();

    const toast = toastAdd.mock.calls.at(-1)?.[0];
    expect(toast.severity).toBe("error");
    expect(toast.summary).toBe("Couldn't copy to the clipboard.");
  });

  it("opens the preview for a pasted list and closes the paste dialog", async () => {
    const { share } = mountComposable();
    share.openPaste();
    expect(share.isPasteOpen.value).toBe(true);

    await share.previewPasted("ludeon.rimworld");

    expect(share.isPasteOpen.value).toBe(false);
    expect(share.isPreviewOpen.value).toBe(true);
    expect(share.outcome.value).toEqual(READY);
  });

  it("opens no preview when the file dialog is cancelled", async () => {
    const { share } = mountComposable();
    pickOpen.mockResolvedValue(null);

    await share.openFile();

    expect(share.isPreviewOpen.value).toBe(false);
  });

  it("toasts an unreadable file with its code's sentence and opens nothing", async () => {
    const { share } = mountComposable({
      preview_order_import_file: () => {
        throw { code: "mod_list_io_failed", message: "C:/x.rml: not found" };
      },
    });
    pickOpen.mockResolvedValue("C:/x.rml");

    await share.openFile();

    expect(share.isPreviewOpen.value).toBe(false);
    expect(toastAdd.mock.calls.at(-1)?.[0].summary).toBe("Couldn't read or write the mod list");
  });

  it("imports the preview's order, adopts Current, closes the preview and says it is ready", async () => {
    const { share, calls, session } = mountComposable();
    session.setSelected("suggested");
    await share.previewPasted("ludeon.rimworld");

    await share.confirmImport(["ludeon.rimworld", "example.a"]);

    expect(calls.importOrder).toEqual([{ order: ["ludeon.rimworld", "example.a"] }]);
    expect(session.selected).toBe("current");
    expect(share.isPreviewOpen.value).toBe(false);
    expect(toastAdd.mock.calls.at(-1)?.[0].summary).toBe(
      "Imported order ready. Review it, then Apply to write ModsConfig.xml.",
    );
  });

  it("shows the scan's notes and rules warnings after an import, in their own words", async () => {
    const { share } = mountComposable({
      import_order: () => ({
        modCount: 1,
        gameVersion: "1.6.4871",
        elapsedMs: 1,
        warnings: [
          { modId: "example.a", message: "unreadable About.xml" },
          { modId: "example.b", message: "unreadable About.xml" },
        ],
        ruleWarnings: [],
        selected: "current",
      }),
    });
    await share.previewPasted("ludeon.rimworld");

    await share.confirmImport(["ludeon.rimworld"]);

    const summaries = toastAdd.mock.calls.map((call) => call[0].summary);
    expect(summaries).toContain("The scan reported 2 warnings.");
  });

  it("keeps the preview open when the scan fails so it can be retried", async () => {
    const { share } = mountComposable({
      import_order: () => {
        throw { code: "mods_config_io_failed", message: "ModsConfig.xml: access denied" };
      },
    });
    await share.previewPasted("ludeon.rimworld");

    await share.confirmImport(["ludeon.rimworld", "example.a"]);

    expect(share.isPreviewOpen.value).toBe(true);
    expect(share.isImporting.value).toBe(false);
  });

  it("closes the preview and asks for a fresh one when the order is refused as outdated", async () => {
    const { share } = mountComposable({
      import_order: () => {
        throw { code: "invalid_input", message: "example.a is not an installed mod" };
      },
    });
    await share.previewPasted("ludeon.rimworld");

    await share.confirmImport(["ludeon.rimworld", "example.a"]);

    expect(share.isPreviewOpen.value).toBe(false);
    expect(toastAdd.mock.calls.at(-1)?.[0].summary).toBe(
      "Your installed mods changed since the preview. Preview the list again.",
    );
  });

  describe("with the app's global mutation error handler installed", () => {
    function mountWithAppErrorHandler(code: string) {
      // `toastError` words the failure through the app-wide i18n instance, as `main.ts` wires it.
      registerAppI18n(createAppI18n());
      setToastService({ add: toastAdd } as unknown as ToastServiceMethods);
      return mountComposable(
        {
          import_order: () => {
            throw { code, message: "ModsConfig.xml: access denied" };
          },
        },
        { mutationOptions: { onError: (error: unknown) => toastError(error) } },
      );
    }

    it("toasts a failed import exactly once, in the order-share group, and keeps the preview", async () => {
      const { share } = mountWithAppErrorHandler("mods_config_io_failed");
      await share.previewPasted("ludeon.rimworld");

      await share.confirmImport(["ludeon.rimworld", "example.a"]);

      expect(toastAdd).toHaveBeenCalledTimes(1);
      const toast = toastAdd.mock.calls[0]?.[0];
      expect(toast.severity).toBe("error");
      expect(toast.group).toBe("order-share");
      expect(share.isPreviewOpen.value).toBe(true);
    });

    it("toasts an outdated preview exactly once, in the order-share group, and closes it", async () => {
      const { share } = mountWithAppErrorHandler("invalid_input");
      await share.previewPasted("ludeon.rimworld");

      await share.confirmImport(["ludeon.rimworld", "example.a"]);

      expect(toastAdd).toHaveBeenCalledTimes(1);
      const toast = toastAdd.mock.calls[0]?.[0];
      expect(toast.group).toBe("order-share");
      expect(toast.summary).toBe(
        "Your installed mods changed since the preview. Preview the list again.",
      );
      expect(share.isPreviewOpen.value).toBe(false);
    });
  });

  it("copies the missing list and says so", async () => {
    const { share } = mountComposable();

    await share.copyMissing("1. [example.gone]");

    expect(navigator.clipboard.writeText).toHaveBeenCalledWith("1. [example.gone]");
    expect(toastAdd.mock.calls.at(-1)?.[0].summary).toBe("Copied the missing list.");
  });

  it("says so when the clipboard is blocked for the missing list, in the same words as Copy as text", async () => {
    const { share } = mountComposable();
    vi.mocked(navigator.clipboard.writeText).mockRejectedValue(new Error("blocked"));

    await share.copyMissing("1. [example.gone]");

    expect(toastAdd).toHaveBeenCalledTimes(1);
    const toast = toastAdd.mock.calls[0]?.[0];
    expect(toast.severity).toBe("error");
    expect(toast.summary).toBe("Couldn't copy to the clipboard.");
  });

  it("sends the Workshop id as digits, never a URL", async () => {
    const { share, calls } = mountComposable();

    await share.openWorkshop(1234567890);

    expect(calls.workshop).toEqual([{ workshopId: "1234567890" }]);
  });

  it("ignores a second entry point while one is in flight", async () => {
    let release: () => void = () => undefined;
    const { share } = mountComposable({
      export_order_text: () =>
        new Promise((resolve) => {
          release = () => resolve({ text: "x", count: 1, unrepresentable: [] });
        }),
    });

    const first = share.copyAsText();
    await flush();
    await share.copyAsText();
    release();
    await first;

    expect(navigator.clipboard.writeText).toHaveBeenCalledTimes(1);
  });
});
