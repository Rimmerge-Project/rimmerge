import { formatList } from "@/i18n/format";
import type { CaveatDto } from "@/types/generated/CaveatDto";
import { assertNever } from "@/utils/assertNever";

/** The `t()` shape below takes — a caller's own `useI18n().t`. */
type Translate = (key: string, params?: Record<string, unknown>, count?: number) => string;

/**
 * A human-readable rendering of one {@link CaveatDto} — `MergeHeader.vue`'s
 * own caveats list and `DefPage.vue`'s own per-patcher caveats list both
 * render this. Ported from `src-tauri/src/dto/merge.rs`'s former
 * `caveat_text` (removed there): every mod id now goes through `label`
 * (`useModLabel().label`), a real bug the Rust version had, same as
 * {@link findingTitle}'s own port. A plain string, not a
 * `MessageDescriptor`: `duplicateTemplate`/
 * `outOfScopeOwners` each join more than one resolved label, which a
 * flat `params` object can't express.
 *
 * `t`/`label` are the caller's own `useI18n().t`/`useModLabel().label` —
 * this function can't call either composable itself (a plain `utils/`
 * helper, not a component); `locale` is likewise the caller's own
 * `useI18n().locale.value`, for the two mod-list caveats' own
 * `formatList` join.
 */
export function caveatLabel(
  caveat: CaveatDto,
  t: Translate,
  label: (id: string) => string,
  locale: string,
): string {
  switch (caveat.kind) {
    case "duplicateTemplate":
      return t("caveat.duplicateTemplate", {
        name: caveat.name,
        owners: formatList(locale, caveat.owners.map(label)),
      });
    case "modSettingDefault":
      return t("caveat.modSettingDefault", {
        modId: label(caveat.modId),
        class: caveat.class,
      });
    case "failedOp":
      return t("caveat.failedOp", { modId: label(caveat.modId), xpath: caveat.xpath });
    case "unscopedOps":
      return t(
        "caveat.unscopedOps",
        { modId: label(caveat.modId), count: caveat.count },
        caveat.count,
      );
    case "malformedOperation":
      return t("caveat.malformedOperation", { modId: label(caveat.modId) });
    case "defRemoved":
      return t("caveat.defRemoved", { modId: label(caveat.modId) });
    case "positionalItem":
      return t("caveat.positionalItem", { path: caveat.path });
    case "unsafeXpathValue":
      return t("caveat.unsafeXpathValue", { path: caveat.path });
    case "unknownOwnerChoice":
      return t("caveat.unknownOwnerChoice", {
        path: caveat.path,
        modId: label(caveat.modId),
      });
    case "invalidValueFragment":
      return t("caveat.invalidValueFragment", { path: caveat.path });
    case "unsupportedDrop":
      return t("caveat.unsupportedDrop", { path: caveat.path });
    case "unreconstructableChain":
      return t("caveat.unreconstructableChain", { path: caveat.path });
    case "unsettableLeaf":
      return t("caveat.unsettableLeaf", { path: caveat.path });
    case "outOfScopeOwners":
      // "any of {mods}" is a disjunction in every locale, so "or", not "and".
      return t("caveat.outOfScopeOwners", {
        mods: formatList(locale, caveat.mods.map(label), "disjunction"),
      });
    case "clobberedMapEntry":
      return t("caveat.clobberedMapEntry", { path: caveat.path, by: label(caveat.by) });
    default:
      return assertNever(caveat);
  }
}
