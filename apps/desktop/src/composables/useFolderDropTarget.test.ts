import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { effectScope } from "vue";

import { HOVER_OPEN_MS, useFolderDropTarget } from "@/composables/useFolderDropTarget";
import type { GroupedEntryDto } from "@/types/generated/GroupedEntryDto";
import { ROW_DRAG_TYPE } from "@/utils/groupDrop";
import { type TreeRow, treeRows } from "@/utils/groupedEntries";

type FolderRow = Extract<TreeRow, { kind: "folder" }>;

function folderRow(id: number | "ungrouped", isCollapsed = false): FolderRow {
  const entry: GroupedEntryDto =
    id === "ungrouped"
      ? { kind: "folder", folder: { kind: "ungrouped" }, depth: 1, matched: 1, isCollapsed }
      : {
          kind: "folder",
          folder: { kind: "group", id, name: "G", color: null },
          depth: 1,
          matched: 1,
          isCollapsed,
        };
  const [row] = treeRows([entry]);
  if (row === undefined || row.kind !== "folder") {
    throw new Error("no folder row");
  }
  return row;
}

function dragEvent(overrides: Partial<DragEvent> = {}) {
  const dataTransfer = { dropEffect: "move", types: [ROW_DRAG_TYPE] } as unknown as DataTransfer;
  const event = {
    preventDefault: vi.fn(),
    stopPropagation: vi.fn(),
    dataTransfer,
    relatedTarget: null,
    currentTarget: null,
    ...overrides,
  };
  return event as unknown as DragEvent & {
    preventDefault: ReturnType<typeof vi.fn>;
    stopPropagation: ReturnType<typeof vi.fn>;
  };
}

function setup({ isDragActive = true }: { isDragActive?: boolean } = {}) {
  const openFolder = vi.fn();
  const dropOnGroup = vi.fn();
  const clearPaneHover = vi.fn();
  const scope = effectScope();
  const target = scope.run(() =>
    useFolderDropTarget({
      isDragActive: () => isDragActive,
      openFolder,
      dropOnGroup,
      clearPaneHover,
    }),
  );
  if (target === undefined) {
    throw new Error("the composable did not run");
  }
  return { target, openFolder, dropOnGroup, clearPaneHover, scope };
}

describe("useFolderDropTarget", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("accepts a drag over a group header as a copy, and keeps the pane out of it", () => {
    const { target, clearPaneHover } = setup();
    const event = dragEvent();

    target.onHeaderDragOver(event, folderRow(3));

    expect(event.preventDefault).toHaveBeenCalled();
    expect(event.stopPropagation).toHaveBeenCalled();
    expect(event.dataTransfer?.dropEffect).toBe("copy");
    expect(clearPaneHover).toHaveBeenCalled();
    expect(target.hoverKey.value).toBe("f:3");
    expect(target.isRefusedHover.value).toBe(false);
  });

  it("refuses a drag over Not in a group: the drop is not allowed and the refusal shows", () => {
    const { target } = setup();
    const event = dragEvent();

    target.onHeaderDragOver(event, folderRow("ungrouped"));

    expect(event.preventDefault).not.toHaveBeenCalled();
    expect(event.dataTransfer?.dropEffect).toBe("none");
    expect(target.isRefusedHover.value).toBe(true);
  });

  it("leaves everything to the pane while no mod drag is in progress", () => {
    const { target } = setup({ isDragActive: false });
    const event = dragEvent();

    target.onHeaderDragOver(event, folderRow(3));

    expect(event.preventDefault).not.toHaveBeenCalled();
    expect(event.stopPropagation).not.toHaveBeenCalled();
    expect(target.hoverKey.value).toBeNull();
  });

  it("ignores a drag that is not a mod drag, even while the page holds a stale one", () => {
    const { target, dropOnGroup, clearPaneHover } = setup();
    const fileDrag = dragEvent({
      dataTransfer: { dropEffect: "move", types: ["Files"] } as unknown as DataTransfer,
    });

    target.onHeaderDragOver(fileDrag, folderRow(3));
    target.onHeaderDrop(fileDrag, folderRow(3));

    expect(fileDrag.preventDefault).not.toHaveBeenCalled();
    expect(fileDrag.stopPropagation).not.toHaveBeenCalled();
    expect(clearPaneHover).not.toHaveBeenCalled();
    expect(dropOnGroup).not.toHaveBeenCalled();
    expect(target.hoverKey.value).toBeNull();
  });

  it("reports a drop on a group header with its id, and stops the pane's own drop", () => {
    const { target, dropOnGroup } = setup();
    const event = dragEvent();

    target.onHeaderDrop(event, folderRow(7));

    expect(dropOnGroup).toHaveBeenCalledWith(7);
    expect(event.stopPropagation).toHaveBeenCalled();
    expect(target.hoverKey.value).toBeNull();
  });

  it("reports nothing for a drop on Not in a group", () => {
    const { target, dropOnGroup } = setup();

    target.onHeaderDrop(dragEvent(), folderRow("ungrouped"));

    expect(dropOnGroup).not.toHaveBeenCalled();
  });

  it("opens a folded header after the hover delay, and not before", () => {
    const { target, openFolder } = setup();
    const row = folderRow(4, true);

    target.onHeaderDragOver(dragEvent(), row);
    vi.advanceTimersByTime(HOVER_OPEN_MS - 1);
    expect(openFolder).not.toHaveBeenCalled();
    target.onHeaderDragOver(dragEvent(), row);
    vi.advanceTimersByTime(1);

    expect(openFolder).toHaveBeenCalledTimes(1);
    expect(openFolder).toHaveBeenCalledWith({ kind: "group", groupId: 4 });
  });

  it("does not open a header that is already open", () => {
    const { target, openFolder } = setup();

    target.onHeaderDragOver(dragEvent(), folderRow(4, false));
    vi.advanceTimersByTime(HOVER_OPEN_MS * 2);

    expect(openFolder).not.toHaveBeenCalled();
  });

  it("does not open a folder the drag left before the delay", () => {
    const { target, openFolder } = setup();
    const row = folderRow(4, true);
    target.onHeaderDragOver(dragEvent(), row);

    target.onHeaderDragLeave(dragEvent(), row);
    vi.advanceTimersByTime(HOVER_OPEN_MS * 2);

    expect(openFolder).not.toHaveBeenCalled();
    expect(target.hoverKey.value).toBeNull();
  });

  it("cancels a pending open when its scope ends", () => {
    const { target, openFolder, scope } = setup();
    target.onHeaderDragOver(dragEvent(), folderRow(4, true));

    scope.stop();
    vi.advanceTimersByTime(HOVER_OPEN_MS * 2);

    expect(openFolder).not.toHaveBeenCalled();
  });
});
