import { afterEach, describe, expect, it, vi } from "vitest";

const { openMock } = vi.hoisted(() => ({ openMock: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: openMock }));

import { pickFile } from "@/services/dialogs";

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
