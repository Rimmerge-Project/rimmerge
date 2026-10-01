//! Resolving type references to assembly references, and decoding runtime-patch attributes.

use std::collections::{BTreeSet, HashMap};

use super::headers::{compressed_uint, read_blob, read_ser_string};
use super::tables::{
    CUSTOM_ATTRIBUTE_TYPE, CUSTOM_ATTRIBUTE_TYPE_TAG_MEMBER_REF, HAS_CUSTOM_ATTRIBUTE,
    HAS_CUSTOM_ATTRIBUTE_TAG_METHOD_DEF, HAS_CUSTOM_ATTRIBUTE_TAG_TYPE_DEF, MEMBER_REF_PARENT,
    MEMBER_REF_PARENT_TAG_TYPE_REF, RESOLUTION_SCOPE_TAG_BITS, TYPE_DEF_OR_REF_TAG_BITS,
    TYPE_DEF_OR_REF_TAG_TYPE_REF, decode_coded_index,
};
use super::tables::{
    RESOLUTION_SCOPE_TAG_ASSEMBLY_REF, RESOLUTION_SCOPE_TAG_TYPE_REF, TYPE_DEF_OR_REF_TAG_TYPE_SPEC,
};
use crate::domain::{RuntimePatchKind, RuntimePatchTarget};

/// The runtime-method-patching library's own IL metadata names — a
/// technical fact about how mods patch game methods at runtime, not a
/// naming choice this workspace makes. Detecting a runtime patch means
/// recognizing these exact `TypeRef` names in a DLL's own
/// `CustomAttribute` metadata, since that is literally how the library's
/// generated attributes are named in every DLL that uses it; there is no
/// synonym to detect instead. Kept as one named group rather than
/// scattered literals so every place this reader depends on the
/// library's own naming is visible at a glance.
mod attribute_names {
    /// `[HarmonyPatch(...)]` — names a patch target.
    pub(super) const PATCH_TARGET: &str = "HarmonyPatch";
    /// `[HarmonyPrefix]` — a bare kind-marker attribute taking no
    /// constructor arguments.
    pub(super) const PREFIX_MARKER: &str = "HarmonyPrefix";
    /// `[HarmonyPostfix]` — see [`PREFIX_MARKER`].
    pub(super) const POSTFIX_MARKER: &str = "HarmonyPostfix";
    /// `[HarmonyTranspiler]` — see [`PREFIX_MARKER`].
    pub(super) const TRANSPILER_MARKER: &str = "HarmonyTranspiler";
}

/// Resolves a `TypeDefOrRef` coded-index value (a `TypeDef.Extends` or
/// `InterfaceImpl.Interface` field) to the 0-based `AssemblyRef` row it
/// ultimately names, when it names one at all.
///
/// A `TypeDef` target is defined in this same assembly, so it never leads to
/// an `AssemblyRef`. A `TypeRef` target resolves via
/// [`resolve_type_ref_to_assembly_ref`]. A `TypeSpec` target — a class
/// deriving from `OtherMod.Base<T>` or implementing `OtherMod.IFace<T>` —
/// stores the base as a generic instantiation over a `TypeDefOrRef` (ECMA-335
/// II.23.2.14, `GENERICINST`); `Assembly.GetTypes()` resolves that base
/// exactly like a non-generic one at DLL-load time, so it's load-time too,
/// resolved by `type_spec_assembly_refs[row - 1]` (precomputed by
/// [`resolve_type_spec_generic_base`] over every TypeSpec row before this
/// function is ever called — nested generic arguments are not followed, only
/// the base's own definition). `None` is returned for a null reference, a
/// `TypeDef` target, a `TypeSpec` whose signature isn't a `GENERICINST` over
/// a `TypeRef`, or a `TypeRef` that ultimately resolves to a
/// `Module`/`ModuleRef` (i.e. still within this assembly) rather than an
/// `AssemblyRef`.
pub(super) fn resolve_type_def_or_ref_to_assembly_ref(
    coded: u32,
    type_ref_scopes: &[u32],
    type_spec_assembly_refs: &[Option<usize>],
) -> Option<usize> {
    let (tag, row) = decode_coded_index(coded, TYPE_DEF_OR_REF_TAG_BITS);
    match tag {
        TYPE_DEF_OR_REF_TAG_TYPE_REF if row != 0 => {
            resolve_type_ref_to_assembly_ref(row, type_ref_scopes)
        }
        TYPE_DEF_OR_REF_TAG_TYPE_SPEC if row != 0 => type_spec_assembly_refs
            .get(row.checked_sub(1)? as usize)
            .copied()
            .flatten(),
        _ => None,
    }
}

/// A `TypeSpec` row's own signature blob, when it's a `GENERICINST` (`0x15`)
/// over a `TypeRef`: resolves that `TypeRef` to its `AssemblyRef` the same
/// way a direct `TypeRef` base would. Every other shape — a `TypeDef` inner
/// definition (defined in this same assembly, so never
/// load-time-cross-assembly), an array/pointer/other signature kind, or a
/// malformed/truncated blob — returns `None`. Nested generic arguments (the
/// type parameters themselves) are never inspected: only the base's own
/// generic *definition* has to be loaded for `Assembly.GetTypes()` to resolve
/// it.
pub(super) fn resolve_type_spec_generic_base(
    bytes: &[u8],
    blob_offset: usize,
    blob_size: usize,
    signature_blob_index: u32,
    type_ref_scopes: &[u32],
) -> Option<usize> {
    const ELEMENT_TYPE_GENERICINST: u8 = 0x15;
    const ELEMENT_TYPE_VALUETYPE: u8 = 0x11;
    const ELEMENT_TYPE_CLASS: u8 = 0x12;

    let sig = read_blob(bytes, blob_offset, blob_size, signature_blob_index).ok()?;
    if *sig.first()? != ELEMENT_TYPE_GENERICINST {
        return None;
    }
    let inner_kind = *sig.get(1)?;
    if inner_kind != ELEMENT_TYPE_VALUETYPE && inner_kind != ELEMENT_TYPE_CLASS {
        return None;
    }
    let (coded, _consumed) = compressed_uint(sig, 2).ok()?;
    let (tag, row) = decode_coded_index(coded, TYPE_DEF_OR_REF_TAG_BITS);
    if tag != TYPE_DEF_OR_REF_TAG_TYPE_REF || row == 0 {
        return None;
    }
    resolve_type_ref_to_assembly_ref(row, type_ref_scopes)
}

/// Walks a `TypeRef` row's `ResolutionScope` up through nested-type
/// `TypeRef` links (ECMA-335: a nested type's `TypeRef` names its
/// enclosing type's `TypeRef` as its scope) until it reaches an
/// `AssemblyRef`, returning that `AssemblyRef`'s 0-based row index.
///
/// The walk is bounded by `type_ref_scopes.len()` iterations, so a
/// hostile or malformed cyclic `ResolutionScope` chain terminates with
/// `None` instead of looping forever.
pub(super) fn resolve_type_ref_to_assembly_ref(
    mut row: u32,
    type_ref_scopes: &[u32],
) -> Option<usize> {
    for _ in 0..=type_ref_scopes.len() {
        let scope = *type_ref_scopes.get(row.checked_sub(1)? as usize)?;
        let (tag, scope_row) = decode_coded_index(scope, RESOLUTION_SCOPE_TAG_BITS);
        match tag {
            RESOLUTION_SCOPE_TAG_ASSEMBLY_REF => {
                return scope_row.checked_sub(1).map(|r| r as usize);
            }
            RESOLUTION_SCOPE_TAG_TYPE_REF if scope_row != 0 => row = scope_row,
            _ => return None, // Module/ModuleRef (or a malformed null TypeRef scope).
        }
    }
    None
}

// ---------------------------------------------------------------------
// Blob heap and compressed integers (ECMA-335 II.23.2)
// ---------------------------------------------------------------------

/// Which kind of member a `HasCustomAttribute` `Parent` names, restricted to
/// the two kinds runtime-patch detection cares about — a class or a method —
/// with the 0-based `TypeDef` row / 1-based `MethodDef` row it names (1-based
/// for `MethodDef` because [`combine_runtime_patches`] compares it directly
/// against `TypeDef.MethodList`, which is itself 1-based per ECMA-335).
#[derive(Debug, Clone, Copy)]
pub(super) enum AttrOwner {
    TypeDef(u32),
    MethodDef(u32),
}

/// Which C# parameter type produced one decoded `HarmonyPatch`
/// constructor argument. Both kinds decode identically as a `SerString`
/// (ECMA-335 II.23.3: a `System.Type` fixed argument is serialized as its
/// assembly-qualified name, a `System.String` fixed argument as itself),
/// so the signature blob — not the value blob — is what tells them apart
/// (see [`member_ref_ctor_param_count`]). The distinction matters for a
/// class-level single-argument attribute: `Type` names the target type,
/// `Text` names a method (the library has no single-`string`-argument
/// constructor that names a type — see [`combine_runtime_patches`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ArgKind {
    Type,
    Text,
}

/// Strips an assembly-qualified `System.Type` name down to its bare type
/// name: `Assembly.GetTypes()`-style resolution never needs the
/// `, Assembly-CSharp, Version=..., Culture=..., PublicKeyToken=...` tail
/// ECMA-335 II.23.3 mandates for a `System.Type` fixed argument, and every
/// downstream consumer of [`RuntimePatchTarget::type_name`] compares
/// against a bare name (a plain `HarmonyPatch(string typeName)` argument
/// is never assembly-qualified to begin with, so this is a no-op there).
pub(super) fn normalize_type_name(raw: &str) -> String {
    raw.split(',').next().unwrap_or(raw).trim().to_string()
}

/// One decoded fixed argument of a `HarmonyPatch(...)` constructor call: a
/// [`ArgKind::Type`] argument's `value` is already normalized (see
/// [`normalize_type_name`]); `None` is a serialized null reference
/// (`SerString`'s `0xFF` marker), vanishingly rare here but handled rather
/// than misdecoded.
#[derive(Debug, Clone)]
pub(super) struct RuntimePatchArg {
    pub(super) kind: ArgKind,
    pub(super) value: Option<String>,
}

/// One `[HarmonyPatch(...)]` application, decoded but not yet combined
/// with its class's or method's counterpart attribute:
/// `args.len()` is 1 or 2 (never 0 — a bare `[HarmonyPatch]` marker with
/// no constructor arguments carries no target information and is never
/// collected in the first place, and a constructor whose recognized
/// `Type`/`Text` prefix is empty — an unsupported argument in the very
/// first position — is dropped the same way).
pub(super) struct RuntimePatchAttrRaw {
    pub(super) owner: AttrOwner,
    pub(super) args: Vec<RuntimePatchArg>,
}

/// One `CustomAttribute` row's raw fields, bundled so
/// [`collect_runtime_patch_attr`] takes one struct instead of three loose `u32`s.
pub(super) struct RuntimePatchAttrCandidate {
    pub(super) parent: u32,
    pub(super) attr_type: u32,
    pub(super) value_blob_index: u32,
}

/// Resolves a `CustomAttributeType` coded index to the `MemberRef` row it
/// names and that `MemberRef`'s `Class` `TypeRef` name — the shape every
/// HarmonyLib marker/constructor attribute takes (the attribute type lives
/// in another assembly, HarmonyLib, referenced as a `MemberRef` whose
/// `Class` names a `TypeRef`). `None` for any other shape: a
/// `MethodDef`-referenced attribute is defined in this same assembly, so
/// never one of HarmonyLib's own.
fn resolve_attribute_member_ref<'a>(
    attr_type: u32,
    member_ref_classes: &[u32],
    type_ref_names: &'a [String],
) -> Option<(usize, &'a str)> {
    let (tag, row) = decode_coded_index(attr_type, CUSTOM_ATTRIBUTE_TYPE.1);
    if tag != CUSTOM_ATTRIBUTE_TYPE_TAG_MEMBER_REF || row == 0 {
        return None;
    }
    let member_ref_row = row.checked_sub(1)? as usize;
    let class_coded = *member_ref_classes.get(member_ref_row)?;
    let (class_tag, class_row) = decode_coded_index(class_coded, MEMBER_REF_PARENT.1);
    if class_tag != MEMBER_REF_PARENT_TAG_TYPE_REF || class_row == 0 {
        return None;
    }
    let type_ref_row = class_row.checked_sub(1)? as usize;
    let name = type_ref_names.get(type_ref_row)?.as_str();
    Some((member_ref_row, name))
}

/// Whether `attr_type` names the library's own patch-target attribute
/// (`HarmonyPatch`). Returns that `MemberRef`'s 0-based row, needed to
/// look up its constructor signature next.
pub(super) fn runtime_patch_ctor_member_ref_row(
    attr_type: u32,
    member_ref_classes: &[u32],
    type_ref_names: &[String],
) -> Option<usize> {
    let (member_ref_row, name) =
        resolve_attribute_member_ref(attr_type, member_ref_classes, type_ref_names)?;
    (name == attribute_names::PATCH_TARGET).then_some(member_ref_row)
}

/// Whether `attr_type` names one of the library's bare kind-marker attributes
/// (`HarmonyPrefix`/`HarmonyPostfix`/`HarmonyTranspiler`) — simpler than
/// [`runtime_patch_ctor_member_ref_row`]'s target-selector counterpart because
/// these take no constructor arguments, so no signature-blob or value-blob
/// decoding ever applies.
pub(super) fn runtime_patch_kind_attribute(
    attr_type: u32,
    member_ref_classes: &[u32],
    type_ref_names: &[String],
) -> Option<RuntimePatchKind> {
    let (_, name) = resolve_attribute_member_ref(attr_type, member_ref_classes, type_ref_names)?;
    match name {
        attribute_names::PREFIX_MARKER => Some(RuntimePatchKind::Prefix),
        attribute_names::POSTFIX_MARKER => Some(RuntimePatchKind::Postfix),
        attribute_names::TRANSPILER_MARKER => Some(RuntimePatchKind::Transpiler),
        _ => None,
    }
}

/// Examines one `CustomAttribute` row and, when it's a recognized runtime-patch
/// kind-marker attribute applied to a method, records that method's kind in
/// `out`. Every other row is silently skipped, the same "only ever adds
/// facts" contract [`collect_runtime_patch_attr`] documents.
/// `out.entry(...).or_insert(...)` keeps the first kind found for a given
/// method row deterministic (table row order) on the hostile/malformed input
/// of a method carrying more than one kind marker.
pub(super) fn collect_runtime_patch_kind_attr(
    parent: u32,
    attr_type: u32,
    member_ref_classes: &[u32],
    type_ref_names: &[String],
    out: &mut HashMap<u32, RuntimePatchKind>,
) {
    let Some(kind) = runtime_patch_kind_attribute(attr_type, member_ref_classes, type_ref_names)
    else {
        return;
    };
    let (owner_tag, owner_row) = decode_coded_index(parent, HAS_CUSTOM_ATTRIBUTE.1);
    if owner_tag == HAS_CUSTOM_ATTRIBUTE_TAG_METHOD_DEF && owner_row != 0 {
        out.entry(owner_row).or_insert(kind);
    }
}

/// Decodes the fixed-argument kinds of one `HarmonyPatch` constructor
/// overload from its signature blob: each parameter is `System.String`
/// (`ELEMENT_TYPE_STRING`, `0x0E`, → [`ArgKind::Text`]) or a reference
/// type (`ELEMENT_TYPE_CLASS`, `0x12` — a custom attribute's fixed
/// arguments only ever allow `System.Type` there, per ECMA-335 II.23.3, →
/// [`ArgKind::Type`]).
///
/// Decoding stops at the first parameter that is neither — the library has
/// several overloads with a `Type`/`string` prefix followed by a trailing
/// `params Type[] argumentTypes` (`ELEMENT_TYPE_SZARRAY`, `0x1D`) or
/// `MethodType methodType` (an enum, encoded as `ELEMENT_TYPE_VALUETYPE`,
/// `0x11`, over its `TypeDefOrRef`): `(Type, string, Type[])`, `(Type,
/// string, MethodType)`, `(string, string, MethodType)`. The leading
/// `Type`/`string` prefix is exactly the type/method-name information
/// Runtime-patch detection needs; the trailing argument is never read. A
/// constructor whose *first* parameter is already unsupported (no
/// `HarmonyPatch` overload this reader models) yields an empty prefix, which
/// [`collect_runtime_patch_attr`]'s `param_count == 0` check then drops — this
/// function itself never fails on an unsupported shape, only on a genuinely
/// malformed or truncated blob.
pub(super) fn member_ref_ctor_param_count(
    bytes: &[u8],
    blob_offset: usize,
    blob_size: usize,
    signature_blob_index: u32,
) -> Option<Vec<ArgKind>> {
    const ELEMENT_TYPE_STRING: u8 = 0x0E;
    const ELEMENT_TYPE_CLASS: u8 = 0x12;

    let sig = read_blob(bytes, blob_offset, blob_size, signature_blob_index).ok()?;
    // Byte 0 is the calling-convention flags (HASTHIS et al. — a
    // constructor is always an instance method, and `this` isn't counted
    // in ParamCount); skip it.
    let (param_count, count_len) = compressed_uint(sig, 1).ok()?;
    // RetType follows; a constructor always returns void, one byte.
    let mut offset = 1usize.checked_add(count_len)?.checked_add(1)?;
    let mut kinds = Vec::new();
    for _ in 0..param_count {
        let elem = *sig.get(offset)?;
        match elem {
            ELEMENT_TYPE_STRING => {
                kinds.push(ArgKind::Text);
                offset = offset.checked_add(1)?;
            }
            ELEMENT_TYPE_CLASS => {
                kinds.push(ArgKind::Type);
                let (_target, coded_len) = compressed_uint(sig, offset.checked_add(1)?).ok()?;
                offset = offset.checked_add(1)?.checked_add(coded_len)?;
            }
            _ => break,
        }
    }
    Some(kinds)
}

/// Decodes a `CustomAttribute.Value` blob's fixed arguments (ECMA-335
/// II.23.3): a `0x0001` prolog, then one `SerString` per entry in `kinds`, in
/// sequence — valid only because [`member_ref_ctor_param_count`] already
/// confirmed every one of those parameters is `SerString`-encoded. A
/// [`ArgKind::Type`] value is normalized to its bare type name (see
/// [`normalize_type_name`]) as it's decoded — the assembly-qualifier never
/// needs to exist past this point. Named arguments (if any) and any trailing
/// fixed argument beyond `kinds.len()` (an unsupported suffix, e.g. a `params
/// Type[]`) are never read; the fixed arguments `kinds` describes are all
/// Runtime-patch detection needs.
pub(super) fn decode_attribute_args(
    value_blob: &[u8],
    kinds: &[ArgKind],
) -> Option<Vec<RuntimePatchArg>> {
    let prolog = u16::from_le_bytes([*value_blob.first()?, *value_blob.get(1)?]);
    if prolog != 1 {
        return None;
    }
    let mut offset = 2;
    let mut args = Vec::with_capacity(kinds.len());
    for &kind in kinds {
        let (text, consumed) = read_ser_string(value_blob, offset).ok()?;
        let value = if kind == ArgKind::Type {
            text.as_deref().map(normalize_type_name)
        } else {
            text
        };
        args.push(RuntimePatchArg { kind, value });
        offset += consumed;
    }
    Some(args)
}

/// Examines one `CustomAttribute` row and, when it's a recognized
/// `HarmonyPatch(...)` application on a class or method, decodes it into
/// `out`. Every other row (a different attribute entirely, an unsupported
/// constructor shape, a malformed blob) is silently skipped — this reader
/// only ever adds facts, never errors, for attribute decoding.
#[allow(
    clippy::too_many_arguments,
    reason = "internal helper called from one site in read_tables; splitting the table context into a struct would only move the same seven pieces of data one level down"
)]
pub(super) fn collect_runtime_patch_attr(
    candidate: RuntimePatchAttrCandidate,
    bytes: &[u8],
    blob_offset: usize,
    blob_size: usize,
    member_ref_classes: &[u32],
    member_ref_signatures: &[u32],
    type_ref_names: &[String],
    out: &mut Vec<RuntimePatchAttrRaw>,
) {
    let Some(member_ref_row) =
        runtime_patch_ctor_member_ref_row(candidate.attr_type, member_ref_classes, type_ref_names)
    else {
        return;
    };
    let Some(&signature_blob_index) = member_ref_signatures.get(member_ref_row) else {
        return;
    };
    let Some(kinds) =
        member_ref_ctor_param_count(bytes, blob_offset, blob_size, signature_blob_index)
    else {
        return;
    };
    if kinds.is_empty() || kinds.len() > 2 {
        return;
    }
    let Ok(value_blob) = read_blob(bytes, blob_offset, blob_size, candidate.value_blob_index)
    else {
        return;
    };
    let Some(args) = decode_attribute_args(value_blob, &kinds) else {
        return;
    };

    let (owner_tag, owner_row) = decode_coded_index(candidate.parent, HAS_CUSTOM_ATTRIBUTE.1);
    let owner = if owner_tag == HAS_CUSTOM_ATTRIBUTE_TAG_TYPE_DEF && owner_row != 0 {
        AttrOwner::TypeDef(owner_row - 1)
    } else if owner_tag == HAS_CUSTOM_ATTRIBUTE_TAG_METHOD_DEF && owner_row != 0 {
        AttrOwner::MethodDef(owner_row)
    } else {
        return;
    };
    out.push(RuntimePatchAttrRaw { owner, args });
}

/// The 0-based `TypeDef` row that owns `method_row` (a 1-based `MethodDef`
/// row), via `TypeDef.MethodList` ranges: ECMA-335 requires those values
/// non-decreasing across `TypeDef` rows in table order, so the type with
/// the *last* start `<= method_row` is the one whose range contains it —
/// an earlier type sharing that same start owns zero methods.
pub(super) fn owning_type_def(method_row: u32, type_def_method_starts: &[u32]) -> Option<u32> {
    type_def_method_starts
        .iter()
        .enumerate()
        .filter(|&(_, &start)| start != 0 && start <= method_row)
        .max_by_key(|&(_, &start)| start)
        .map(|(i, _)| i as u32)
}

/// The `[start, end)` 1-based `MethodDef` row range TypeDef row
/// `type_def_row` (0-based) owns: from its own `MethodList` start up to
/// the next `TypeDef` row's start — ECMA-335 requires `MethodList`
/// non-decreasing across `TypeDef` rows in table order, so the next row's
/// start is exactly this row's exclusive end (an empty range when the two
/// are equal, i.e. this class owns no methods). The last `TypeDef` row's
/// end is `method_def_count + 1`, so its range still reaches the final
/// `MethodDef` row.
pub(super) fn method_range_for_type_def(
    type_def_row: u32,
    type_def_method_starts: &[u32],
    method_def_count: u32,
) -> std::ops::Range<u32> {
    let end_of_table = method_def_count.saturating_add(1);
    let start = type_def_method_starts
        .get(type_def_row as usize)
        .copied()
        .unwrap_or(end_of_table);
    let end = type_def_method_starts
        .get(type_def_row as usize + 1)
        .copied()
        .unwrap_or(end_of_table);
    start..end
}

/// The runtime patch-method kind `method_row` (a 1-based `MethodDef` row)
/// implements: an explicit
/// `[HarmonyPrefix]`/`[HarmonyPostfix]`/`[HarmonyTranspiler]` marker
/// attribute (`method_kind_attrs`) takes precedence; absent that, the library's
/// own convention-based discovery — a method literally named
/// `Prefix`/`Postfix`/`Transpiler` — is checked next. `None` when neither
/// mechanism applies to this method.
pub(super) fn method_kind(
    method_row: u32,
    method_def_names: &[String],
    method_kind_attrs: &HashMap<u32, RuntimePatchKind>,
) -> Option<RuntimePatchKind> {
    if let Some(&kind) = method_kind_attrs.get(&method_row) {
        return Some(kind);
    }
    match method_def_names
        .get(method_row.checked_sub(1)? as usize)?
        .as_str()
    {
        "Prefix" => Some(RuntimePatchKind::Prefix),
        "Postfix" => Some(RuntimePatchKind::Postfix),
        "Transpiler" => Some(RuntimePatchKind::Transpiler),
        _ => None,
    }
}

/// Where [`kinds_for_scope`] should look for the method(s) implementing one
/// decoded `(type_name, method_name)` association: a self-contained patch —
/// where the association's own `[HarmonyPatch]` attribute sits directly on
/// the implementing method (the library treats that exact method as the patch,
/// checking its own attributes/name for role) — narrows the search to that
/// one `MethodDef` row ([`Self::Method`]); a class-level association searches
/// every method the class owns ([`Self::ClassMethods`]), since the
/// implementing `Prefix`/`Postfix`/`Transpiler` method(s) are the class's
/// *other* members, not the class itself.
enum AssociationScope {
    Method(u32),
    ClassMethods(std::ops::Range<u32>),
}

/// Every distinct kind found within `scope`, via [`method_kind`]. Empty
/// when nothing in scope carries a recognized kind attribute or name —
/// callers must treat that as [`RuntimePatchKind::Unknown`], never guess.
fn kinds_for_scope(
    scope: &AssociationScope,
    method_def_names: &[String],
    method_kind_attrs: &HashMap<u32, RuntimePatchKind>,
) -> BTreeSet<RuntimePatchKind> {
    match scope {
        AssociationScope::Method(row) => method_kind(*row, method_def_names, method_kind_attrs)
            .into_iter()
            .collect(),
        AssociationScope::ClassMethods(range) => range
            .clone()
            .filter_map(|row| method_kind(row, method_def_names, method_kind_attrs))
            .collect(),
    }
}

/// Merges every decoded `[HarmonyPatch(...)]` application in one assembly
/// into concrete `(type, method, kind)` targets, the way the library itself
/// merges a class's and its methods' attributes:
///
/// - A 2-argument attribute (on a class or a method) is a complete target
///   on its own — its first argument is always the type name (`Type` or
///   `string`), its second the method name. A class-level 2-argument
///   attribute also records the class's type, so a sibling method-level
///   1-argument attribute elsewhere in the same class can still combine.
/// - A 1-argument class-level [`ArgKind::Type`] attribute (`typeof(T)`
///   alone, or `HarmonyPatch(string typeName)`) records the class's
///   target type, without yet naming a method.
/// - A 1-argument class-level [`ArgKind::Text`] attribute
///   (`[HarmonyPatch("MethodName")]` directly on the class) names the
///   *method*, not the type — the library's only single-`string`-argument
///   constructor is `HarmonyPatch(string methodName)`, meant to be read
///   alongside a sibling class-level `Type` attribute; there is no
///   single-argument overload that takes a type name as a plain string.
///   A class carrying both a `Type` and a `Text` single-argument
///   attribute — in either order, attribute rows carry no ordering
///   guarantee — combines them into one target.
/// - A 1-argument method-level [`ArgKind::Text`] attribute (a method name
///   alone) combines with its *declaring class's* type, once every class
///   in the assembly has been seen.
///
/// A 1-argument attribute (class- or method-level) whose class never resolved
/// a type name (no class-level `Type` attribute, or no sibling `Text`
/// attribute, at all) contributes nothing — the library would then resolve the
/// type via the surrounding `PatchClassProcessor` scan, which is explicitly
/// out of scope here.
///
/// Each target also carries a `kind`: once an association's `(type_name,
/// method_name)` is known, [`kinds_for_scope`] searches its
/// [`AssociationScope`] for the implementing `Prefix`/`Postfix`/`Transpiler`
/// method(s). An association with no kind evidence at all still produces one
/// target, kind [`RuntimePatchKind::Unknown`] — the required conservative
/// default; a class implementing more than one role for the same association
/// (e.g. both a `Prefix` and a `Postfix` method) produces one target per role
/// found, since the transpiler-collision finding only ever cares about
/// `Transpiler` specifically and must be able to tell it apart from the
/// others sharing the same target.
pub(super) fn combine_runtime_patches(
    attrs: &[RuntimePatchAttrRaw],
    type_def_method_starts: &[u32],
    method_def_names: &[String],
    method_kind_attrs: &HashMap<u32, RuntimePatchKind>,
) -> Vec<RuntimePatchTarget> {
    let method_def_count = method_def_names.len() as u32;
    let class_scope = |type_def_row: u32| {
        AssociationScope::ClassMethods(method_range_for_type_def(
            type_def_row,
            type_def_method_starts,
            method_def_count,
        ))
    };

    let mut class_type_name: HashMap<u32, String> = HashMap::new();
    let mut class_method_name: HashMap<u32, String> = HashMap::new();
    let mut associations: Vec<(String, String, AssociationScope)> = Vec::new();

    for attr in attrs {
        match attr.args.as_slice() {
            [type_arg, method_arg] => {
                if let (Some(type_name), Some(method_name)) = (&type_arg.value, &method_arg.value) {
                    if let AttrOwner::TypeDef(type_def_row) = attr.owner {
                        class_type_name
                            .entry(type_def_row)
                            .or_insert_with(|| type_name.clone());
                    }
                    let scope = match attr.owner {
                        AttrOwner::TypeDef(row) => class_scope(row),
                        AttrOwner::MethodDef(row) => AssociationScope::Method(row),
                    };
                    associations.push((type_name.clone(), method_name.clone(), scope));
                }
            }
            [arg] => {
                if let (AttrOwner::TypeDef(type_def_row), Some(value)) = (attr.owner, &arg.value) {
                    match arg.kind {
                        ArgKind::Type => {
                            class_type_name
                                .entry(type_def_row)
                                .or_insert_with(|| value.clone());
                        }
                        ArgKind::Text => {
                            class_method_name
                                .entry(type_def_row)
                                .or_insert_with(|| value.clone());
                        }
                    }
                }
            }
            _ => {}
        }
    }

    // A class carrying both a single-argument `Type` and a single-argument
    // `Text` attribute — in either order — names one complete target.
    for (type_def_row, type_name) in &class_type_name {
        if let Some(method_name) = class_method_name.get(type_def_row) {
            associations.push((
                type_name.clone(),
                method_name.clone(),
                class_scope(*type_def_row),
            ));
        }
    }

    for attr in attrs {
        if let (AttrOwner::MethodDef(method_row), [arg]) = (attr.owner, attr.args.as_slice())
            && arg.kind == ArgKind::Text
            && let Some(method_name) = &arg.value
            && let Some(type_def_row) = owning_type_def(method_row, type_def_method_starts)
            && let Some(type_name) = class_type_name.get(&type_def_row)
        {
            associations.push((
                type_name.clone(),
                method_name.clone(),
                AssociationScope::Method(method_row),
            ));
        }
    }

    let mut targets: BTreeSet<(String, String, RuntimePatchKind)> = BTreeSet::new();
    for (type_name, method_name, scope) in &associations {
        let kinds = kinds_for_scope(scope, method_def_names, method_kind_attrs);
        if kinds.is_empty() {
            targets.insert((
                type_name.clone(),
                method_name.clone(),
                RuntimePatchKind::Unknown,
            ));
        } else {
            for kind in kinds {
                targets.insert((type_name.clone(), method_name.clone(), kind));
            }
        }
    }

    targets
        .into_iter()
        .map(|(type_name, method_name, kind)| RuntimePatchTarget {
            type_name,
            method_name,
            kind,
        })
        .collect()
}
