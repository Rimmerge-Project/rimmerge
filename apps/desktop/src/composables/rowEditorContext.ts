// The row editor's own state, shared with its slot and match panels through provide/inject.

import { type ComputedRef, type InjectionKey, inject, type Ref } from "vue";
import type { useModLabel } from "@/composables/useModLabel";
import type { RowDescriptor } from "@/composables/useRowDescriptor";
import type { DroppedItemSlotValueDto } from "@/types/generated/DroppedItemSlotValueDto";
import type { FieldSpecDto } from "@/types/generated/FieldSpecDto";

export type RowEditorContext = {
  descriptor: ComputedRef<RowDescriptor>;
  modLabel: ReturnType<typeof useModLabel>;
  copyFromMatch: (sourceDefName: string, owner: string) => Promise<void>;
  isCopying: ComputedRef<boolean>;
  itemSlotFields: ComputedRef<[string, FieldSpecDto][]>;
  chanceFields: ComputedRef<[string, FieldSpecDto][]>;
  prefillSource: Ref<Record<string, string>>;
  clampedFrom: Ref<Record<string, string[]>>;
  droppedFromCopy: Ref<Record<string, DroppedItemSlotValueDto[]>>;
  namesFor: (path: string) => string[];
  setNames: (path: string, names: string[]) => void;
  chanceAt: (path: string, index: number) => number | null;
  setChanceAt: (path: string, index: number, value: number | null) => void;
};

export const ROW_EDITOR_KEY: InjectionKey<RowEditorContext> = Symbol("RowEditor");

export function useRowEditorContext(): RowEditorContext {
  const context = inject(ROW_EDITOR_KEY);
  if (!context) {
    throw new Error("useRowEditorContext: no RowEditor provided this context");
  }
  return context;
}
