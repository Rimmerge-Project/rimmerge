import { afterEach, describe, expect, it, vi } from "vitest";

const { openMock, saveMock } = vi.hoisted(() => ({ openMock: vi.fn(), saveMock: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: openMock, save: saveMock }));

import { pickFile, pickModListFile, pickModListSavePath } from "@/services/dialogs";

const FILTER_NAMES = { logFiles: "Log files (translated)", allFiles: "Everything (translated)" };

describe("pickFile", () => {
  afterEach(() => {
    openMock.mockReset();
  });

  it("offers .log and .txt files first, and every file, since a snapshot is saved under any name", async () => {
    openMock.mockResolvedValue("C:/logs/midgame1.txt");

    const path = await pickFile(FILTER_NAMES);

    expect(path).toBe("C:/logs/midgame1.txt");
    expect(openMock).toHaveBeenCalledWith(
      expect.objectContaining({
        directory: false,
        multiple: false,
        filters: [
          { name: "Log files (translated)", extensions: ["log", "txt"] },
          { name: "Everything (translated)", extensions: ["*"] },
        ],
      }),
    );
  });

  it("returns null when the user cancels", async () => {
    openMock.mockResolvedValue(null);

    expect(await pickFile(FILTER_NAMES)).toBeNull();
  });
});

describe("pickModListFile", () => {
  afterEach(() => {
    openMock.mockReset();
  });

  it("offers .rml, .xml and .txt first, and every file, since the import detects the format", async () => {
    openMock.mockResolvedValue("C:/lists/shared.rml");

    const path = await pickModListFile({ modLists: "Mod lists", allFiles: "Everything" });

    expect(path).toBe("C:/lists/shared.rml");
    expect(openMock).toHaveBeenCalledWith({
      directory: false,
      multiple: false,
      filters: [
        { name: "Mod lists", extensions: ["rml", "xml", "txt"] },
        { name: "Everything", extensions: ["*"] },
      ],
    });
  });

  it("returns null when the user cancels", async () => {
    openMock.mockResolvedValue(null);

    expect(await pickModListFile({ modLists: "Mod lists", allFiles: "Everything" })).toBeNull();
  });
});

describe("pickModListSavePath", () => {
  afterEach(() => {
    saveMock.mockReset();
  });

  it("starts at the suggested path and offers only .rml", async () => {
    saveMock.mockResolvedValue("C:/Saves/ModLists/mine.rml");

    const path = await pickModListSavePath({
      filterName: "RimWorld mod list",
      defaultPath: "C:/Saves/ModLists/rimmerge-load-order.rml",
    });

    expect(path).toBe("C:/Saves/ModLists/mine.rml");
    expect(saveMock).toHaveBeenCalledWith({
      filters: [{ name: "RimWorld mod list", extensions: ["rml"] }],
      defaultPath: "C:/Saves/ModLists/rimmerge-load-order.rml",
    });
  });

  it("leaves the default path out when the ModLists folder does not exist", async () => {
    saveMock.mockResolvedValue(null);

    const path = await pickModListSavePath({ filterName: "RimWorld mod list", defaultPath: null });

    expect(path).toBeNull();
    expect(saveMock).toHaveBeenCalledWith({
      filters: [{ name: "RimWorld mod list", extensions: ["rml"] }],
    });
  });
});
