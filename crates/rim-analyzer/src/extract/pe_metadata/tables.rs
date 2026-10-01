//! The #~ stream: table ids, coded-index tags, column widths, and the row reader.

use std::collections::HashMap;

use super::headers::{read_heap_string, read_index, read_u16, read_u32, read_u64};
use super::refs::{
    RuntimePatchAttrCandidate, RuntimePatchAttrRaw, collect_runtime_patch_attr,
    collect_runtime_patch_kind_attr, combine_runtime_patches,
    resolve_type_def_or_ref_to_assembly_ref, resolve_type_spec_generic_base,
};
use super::{AssemblyMetadata, PeMetadataError};
use crate::domain::{AssemblyReference, AssemblyVersion, RuntimePatchKind};

const T_MODULE: u8 = 0x00;

const T_TYPE_REF: u8 = 0x01;

pub(super) const T_TYPE_DEF: u8 = 0x02;

const T_FIELD: u8 = 0x04;

const T_METHOD_DEF: u8 = 0x06;

const T_PARAM: u8 = 0x08;

const T_INTERFACE_IMPL: u8 = 0x09;

const T_MEMBER_REF: u8 = 0x0A;

const T_CONSTANT: u8 = 0x0B;

const T_CUSTOM_ATTRIBUTE: u8 = 0x0C;

const T_FIELD_MARSHAL: u8 = 0x0D;

const T_DECL_SECURITY: u8 = 0x0E;

const T_CLASS_LAYOUT: u8 = 0x0F;

const T_FIELD_LAYOUT: u8 = 0x10;

const T_STAND_ALONE_SIG: u8 = 0x11;

const T_EVENT_MAP: u8 = 0x12;

const T_EVENT: u8 = 0x14;

const T_PROPERTY_MAP: u8 = 0x15;

const T_PROPERTY: u8 = 0x17;

const T_METHOD_SEMANTICS: u8 = 0x18;

const T_METHOD_IMPL: u8 = 0x19;

const T_MODULE_REF: u8 = 0x1A;

const T_TYPE_SPEC: u8 = 0x1B;

const T_IMPL_MAP: u8 = 0x1C;

const T_FIELD_RVA: u8 = 0x1D;

const T_ASSEMBLY: u8 = 0x20;

const T_ASSEMBLY_PROCESSOR: u8 = 0x21;

const T_ASSEMBLY_OS: u8 = 0x22;

const T_ASSEMBLY_REF: u8 = 0x23;

const T_FILE: u8 = 0x26;

const T_EXPORTED_TYPE: u8 = 0x27;

const T_MANIFEST_RESOURCE: u8 = 0x28;

const T_GENERIC_PARAM: u8 = 0x2A;

const T_METHOD_SPEC: u8 = 0x2B;

const T_GENERIC_PARAM_CONSTRAINT: u8 = 0x2C;

const RESOLUTION_SCOPE: (&[u8], u32) = (&[T_MODULE, T_MODULE_REF, T_ASSEMBLY_REF, T_TYPE_REF], 2);

pub(super) const TYPE_DEF_OR_REF: (&[u8], u32) = (&[T_TYPE_DEF, T_TYPE_REF, T_TYPE_SPEC], 2);

// `ResolutionScope` coded-index tags (ECMA-335 II.24.2.6).
pub(super) const RESOLUTION_SCOPE_TAG_ASSEMBLY_REF: u32 = 2;

pub(super) const RESOLUTION_SCOPE_TAG_TYPE_REF: u32 = 3;

pub(super) const RESOLUTION_SCOPE_TAG_BITS: u32 = 2;

// `TypeDefOrRef` coded-index tag (ECMA-335 II.24.2.6).
pub(super) const TYPE_DEF_OR_REF_TAG_TYPE_REF: u32 = 1;

pub(super) const TYPE_DEF_OR_REF_TAG_TYPE_SPEC: u32 = 2;

pub(super) const TYPE_DEF_OR_REF_TAG_BITS: u32 = 2;

pub(super) const MEMBER_REF_PARENT: (&[u8], u32) = (
    &[
        T_TYPE_DEF,
        T_TYPE_REF,
        T_MODULE_REF,
        T_METHOD_DEF,
        T_TYPE_SPEC,
    ],
    3,
);

// `MemberRefParent` coded-index tag (ECMA-335 II.24.2.6) — the only one
// runtime-patch detection cares about: the patch-target attribute's
// constructor is always referenced as a `MemberRef` whose `Class` names a
// `TypeRef` (the attribute type lives in another assembly, the runtime
// patching library).
pub(super) const MEMBER_REF_PARENT_TAG_TYPE_REF: u32 = 1;

// `HasCustomAttribute` coded-index tags (ECMA-335 II.24.2.6) — array order
// above is the spec's tag order, so these are that array's indices.
pub(super) const HAS_CUSTOM_ATTRIBUTE_TAG_METHOD_DEF: u32 = 0;

pub(super) const HAS_CUSTOM_ATTRIBUTE_TAG_TYPE_DEF: u32 = 3;

// `CustomAttributeType` coded-index tags (ECMA-335 II.24.2.6): tags 0/1/4
// are unused, 2 is MethodDef, 3 is MemberRef.
pub(super) const CUSTOM_ATTRIBUTE_TYPE_TAG_MEMBER_REF: u32 = 3;

const HAS_CONSTANT: (&[u8], u32) = (&[T_FIELD, T_PARAM, T_PROPERTY], 2);

#[rustfmt::skip]
pub(super) const HAS_CUSTOM_ATTRIBUTE: (&[u8], u32) = (&[
    T_METHOD_DEF, T_FIELD, T_TYPE_REF, T_TYPE_DEF, T_PARAM, T_INTERFACE_IMPL, T_MEMBER_REF, T_MODULE,
    T_DECL_SECURITY, T_PROPERTY, T_EVENT, T_STAND_ALONE_SIG, T_MODULE_REF, T_TYPE_SPEC, T_ASSEMBLY,
    T_ASSEMBLY_REF, T_FILE, T_EXPORTED_TYPE, T_MANIFEST_RESOURCE, T_GENERIC_PARAM, T_GENERIC_PARAM_CONSTRAINT,
    T_METHOD_SPEC,
], 5);

const HAS_FIELD_MARSHAL: (&[u8], u32) = (&[T_FIELD, T_PARAM], 1);

const HAS_DECL_SECURITY: (&[u8], u32) = (&[T_TYPE_DEF, T_METHOD_DEF, T_ASSEMBLY], 2);

const HAS_SEMANTICS: (&[u8], u32) = (&[T_EVENT, T_PROPERTY], 1);

const METHOD_DEF_OR_REF: (&[u8], u32) = (&[T_METHOD_DEF, T_MEMBER_REF], 1);

const MEMBER_FORWARDED: (&[u8], u32) = (&[T_FIELD, T_METHOD_DEF], 1);

// Tags 0/1/4 are unused by CustomAttributeType; only MethodDef(2)/MemberRef(3)
// matter for sizing purposes, and both are already in our row-count table.
pub(super) const CUSTOM_ATTRIBUTE_TYPE: (&[u8], u32) = (&[T_METHOD_DEF, T_MEMBER_REF], 3);

/// Column widths derived from the `#~` header's `HeapSizes` byte and every
/// present table's row count — everything needed to compute row sizes for
/// tables `0x00..=0x23`.
pub(super) struct Widths {
    str: usize,
    blob: usize,
    guid: usize,
    resolution_scope: usize,
    type_def_or_ref: usize,
    member_ref_parent: usize,
    has_constant: usize,
    has_custom_attribute: usize,
    has_field_marshal: usize,
    has_decl_security: usize,
    has_semantics: usize,
    method_def_or_ref: usize,
    member_forwarded: usize,
    custom_attribute_type: usize,
    simple_type_def: usize,
    simple_field: usize,
    simple_method_def: usize,
    simple_param: usize,
    simple_event: usize,
    simple_property: usize,
    simple_module_ref: usize,
}

impl Widths {
    pub(super) fn compute(
        row_counts: &[u32; 64],
        str_wide: bool,
        guid_wide: bool,
        blob_wide: bool,
    ) -> Self {
        let coded = |spec: (&[u8], u32)| coded_width(row_counts, spec.0, spec.1);
        let simple = |table: u8| simple_width(row_counts, table);
        Self {
            str: if str_wide { 4 } else { 2 },
            blob: if blob_wide { 4 } else { 2 },
            guid: if guid_wide { 4 } else { 2 },
            resolution_scope: coded(RESOLUTION_SCOPE),
            type_def_or_ref: coded(TYPE_DEF_OR_REF),
            member_ref_parent: coded(MEMBER_REF_PARENT),
            has_constant: coded(HAS_CONSTANT),
            has_custom_attribute: coded(HAS_CUSTOM_ATTRIBUTE),
            has_field_marshal: coded(HAS_FIELD_MARSHAL),
            has_decl_security: coded(HAS_DECL_SECURITY),
            has_semantics: coded(HAS_SEMANTICS),
            method_def_or_ref: coded(METHOD_DEF_OR_REF),
            member_forwarded: coded(MEMBER_FORWARDED),
            custom_attribute_type: coded(CUSTOM_ATTRIBUTE_TYPE),
            simple_type_def: simple(T_TYPE_DEF),
            simple_field: simple(T_FIELD),
            simple_method_def: simple(T_METHOD_DEF),
            simple_param: simple(T_PARAM),
            simple_event: simple(T_EVENT),
            simple_property: simple(T_PROPERTY),
            simple_module_ref: simple(T_MODULE_REF),
        }
    }
}

/// A simple table-index column is 4 bytes once the target table has more
/// rows than fit in 16 bits, 2 bytes otherwise.
pub(super) fn simple_width(row_counts: &[u32; 64], table: u8) -> usize {
    if row_counts[table as usize] > 0xFFFF {
        4
    } else {
        2
    }
}

/// A coded-index column packs a tag (`tag_bits`) and a row index into one
/// field, 4 bytes once the largest participating table doesn't fit in the
/// remaining `16 - tag_bits` bits, 2 bytes otherwise.
pub(super) fn coded_width(row_counts: &[u32; 64], tables: &[u8], tag_bits: u32) -> usize {
    let max_rows = tables
        .iter()
        .map(|&t| row_counts[t as usize])
        .max()
        .unwrap_or(0);
    let threshold = 1u32 << (16 - tag_bits);
    if max_rows >= threshold { 4 } else { 2 }
}

/// Byte size of one row of `table_id`, for every table that can appear at
/// or before `AssemblyRef` (0x23) in table order. Column order follows
/// ECMA-335 II.22.
pub(super) fn row_size_for_table(table_id: u8, w: &Widths) -> Result<usize, PeMetadataError> {
    Ok(match table_id {
        T_MODULE => 2 + w.str + 3 * w.guid, // Generation, Name, Mvid, EncId, EncBaseId
        T_TYPE_REF => w.resolution_scope + 2 * w.str,
        T_TYPE_DEF => 4 + 2 * w.str + w.type_def_or_ref + w.simple_field + w.simple_method_def,
        T_FIELD => 2 + w.str + w.blob,
        T_METHOD_DEF => 4 + 2 + 2 + w.str + w.blob + w.simple_param,
        T_PARAM => 2 + 2 + w.str,
        T_INTERFACE_IMPL => w.simple_type_def + w.type_def_or_ref,
        T_MEMBER_REF => w.member_ref_parent + w.str + w.blob,
        T_CONSTANT => 2 + w.has_constant + w.blob,
        T_CUSTOM_ATTRIBUTE => w.has_custom_attribute + w.custom_attribute_type + w.blob,
        T_FIELD_MARSHAL => w.has_field_marshal + w.blob,
        T_DECL_SECURITY => 2 + w.has_decl_security + w.blob,
        T_CLASS_LAYOUT => 2 + 4 + w.simple_type_def,
        T_FIELD_LAYOUT => 4 + w.simple_field,
        T_STAND_ALONE_SIG => w.blob,
        T_EVENT_MAP => w.simple_type_def + w.simple_event,
        T_EVENT => 2 + w.str + w.type_def_or_ref,
        T_PROPERTY_MAP => w.simple_type_def + w.simple_property,
        T_PROPERTY => 2 + w.str + w.blob,
        T_METHOD_SEMANTICS => 2 + w.simple_method_def + w.has_semantics,
        T_METHOD_IMPL => w.simple_type_def + 2 * w.method_def_or_ref,
        T_MODULE_REF => w.str,
        T_TYPE_SPEC => w.blob,
        T_IMPL_MAP => 2 + w.member_forwarded + w.str + w.simple_module_ref,
        T_FIELD_RVA => 4 + w.simple_field,
        T_ASSEMBLY => 4 + 2 + 2 + 2 + 2 + 4 + w.blob + 2 * w.str,
        T_ASSEMBLY_PROCESSOR => 4,
        T_ASSEMBLY_OS => 4 + 4 + 4,
        T_ASSEMBLY_REF => 2 + 2 + 2 + 2 + 4 + w.blob + 2 * w.str + w.blob,
        other => return Err(PeMetadataError::UnsupportedTable(other)),
    })
}

/// The byte offset of row `row` (0-based) within a table starting at
/// `table_offset`, each row `row_size` bytes.
pub(super) fn nth_row_offset(
    table_offset: usize,
    row: usize,
    row_size: usize,
) -> Result<usize, PeMetadataError> {
    row.checked_mul(row_size)
        .and_then(|delta| table_offset.checked_add(delta))
        .ok_or(PeMetadataError::Truncated)
}

#[allow(
    clippy::too_many_lines,
    reason = "one pass over the tables stream collecting every fact this reader produces; splitting the match arms into separate functions would need to thread the same half-dozen accumulator vectors through each one for no real gain in clarity"
)]
pub(super) fn read_tables(
    bytes: &[u8],
    tables_offset: usize,
    strings_offset: usize,
    strings_size: usize,
    blob_offset: usize,
    blob_size: usize,
) -> Result<AssemblyMetadata, PeMetadataError> {
    let heap_sizes = *bytes
        .get(tables_offset + 6)
        .ok_or(PeMetadataError::Truncated)?;
    let valid = read_u64(bytes, tables_offset + 8)?;

    let mut row_counts = [0u32; 64];
    let mut cursor = tables_offset + 24; // Reserved(4) + MajorVersion(1) + MinorVersion(1) + HeapSizes(1) + Reserved(1) + Valid(8) + Sorted(8)
    for table_id in 0u8..64 {
        if valid & (1u64 << table_id) != 0 {
            row_counts[table_id as usize] = read_u32(bytes, cursor)?;
            cursor += 4;
        }
    }

    let widths = Widths::compute(
        &row_counts,
        heap_sizes & 0x01 != 0,
        heap_sizes & 0x02 != 0,
        heap_sizes & 0x04 != 0,
    );

    let mut offset = cursor;
    let mut assembly_row: Option<(String, AssemblyVersion)> = None;
    let mut reference_names = Vec::new();
    // Raw `ResolutionScope` coded-index value per TypeRef row, in row
    // order (row `i` here is TypeRef row `i + 1`) — enough to walk a
    // nested-type TypeRef chain up to the AssemblyRef (or Module/
    // ModuleRef) that ultimately resolves it.
    let mut type_ref_scopes: Vec<u32> = Vec::new();
    // Each TypeRef row's own `Name` (identifying a patch-target attribute
    // type), same row order as `type_ref_scopes`.
    let mut type_ref_names: Vec<String> = Vec::new();
    // Each TypeRef row's own `Namespace`, same row order as
    // `type_ref_names` — together they let a `TypeDef.Extends` target
    // that names another assembly's type resolve to that type's own
    // full (`Namespace.Name`) name, the same shape
    // [`AssemblyMetadata::type_hierarchy`] keys every type under.
    let mut type_ref_namespaces: Vec<String> = Vec::new();
    // Raw `TypeDefOrRef` coded-index value of every TypeDef's `Extends`
    // and every InterfaceImpl's `Interface` field, deferred and resolved
    // once every TypeRef/TypeSpec row has been read (TypeDef/InterfaceImpl
    // always precede AssemblyRef in table order, but that doesn't matter
    // here — what matters is that they precede nothing they depend on:
    // TypeRef/TypeSpec rows, which are read earlier in table order).
    let mut extends_or_interface_targets: Vec<u32> = Vec::new();
    // Each TypeDef row's own full (`Namespace.Name`, or bare `Name` when
    // `Namespace` is empty) name, in row order — [`AssemblyMetadata
    // ::type_hierarchy`]'s own key set.
    let mut type_def_full_names: Vec<String> = Vec::new();
    // Each TypeDef row's own raw `Extends` `TypeDefOrRef` coded-index
    // value, same row order as `type_def_full_names` — kept separate
    // from `extends_or_interface_targets` above (which also carries
    // every `InterfaceImpl.Interface` value, interleaved and useless for
    // a per-`TypeDef` lookup) specifically so [`Self::type_hierarchy`]
    // can resolve exactly one base per type.
    let mut type_def_extends: Vec<u32> = Vec::new();
    // Each TypeDef row's own `MethodList` field (the 1-based MethodDef row it
    // starts owning) — the runtime-patch class/method attribute combine needs to
    // map a patched method back to its declaring class.
    let mut type_def_method_starts: Vec<u32> = Vec::new();
    // Each TypeSpec row's own signature blob index, resolved after the full
    // table walk once `type_ref_scopes` is complete.
    let mut type_spec_blobs: Vec<u32> = Vec::new();
    // Each MemberRef row's raw `Class` (MemberRefParent) coded value and
    // `Signature` blob index — both are needed to recognize a patch-target
    // attribute constructor call and to know its argument count.
    let mut member_ref_classes: Vec<u32> = Vec::new();
    let mut member_ref_signatures: Vec<u32> = Vec::new();
    let mut runtime_patch_attrs: Vec<RuntimePatchAttrRaw> = Vec::new();
    // Each MethodDef row's own `Name` (the library's convention-based kind
    // discovery — a method literally named `Prefix`/`Postfix`/`Transpiler`
    // with no attribute at all), same 1-based row numbering
    // `AttrOwner::MethodDef` and `TypeDef.MethodList` already use.
    let mut method_def_names: Vec<String> = Vec::new();
    // Every MethodDef row (1-based) carrying an explicit
    // prefix/postfix/transpiler kind-marker attribute — these take no
    // constructor arguments, so unlike
    // `runtime_patch_attrs` above, only the attribute's type and parent need
    // deciding, never its value blob.
    let mut method_kind_attrs: HashMap<u32, RuntimePatchKind> = HashMap::new();

    for table_id in 0u8..=T_ASSEMBLY_REF {
        let count = row_counts[table_id as usize];
        if count == 0 {
            continue;
        }
        let row_size = row_size_for_table(table_id, &widths)?;

        match table_id {
            T_ASSEMBLY => {
                let major_offset = offset.checked_add(4).ok_or(PeMetadataError::Truncated)?;
                let minor_offset = offset.checked_add(6).ok_or(PeMetadataError::Truncated)?;
                let build_offset = offset.checked_add(8).ok_or(PeMetadataError::Truncated)?;
                let revision_offset = offset.checked_add(10).ok_or(PeMetadataError::Truncated)?;
                let version = AssemblyVersion {
                    major: read_u16(bytes, major_offset)?,
                    minor: read_u16(bytes, minor_offset)?,
                    build: read_u16(bytes, build_offset)?,
                    revision: read_u16(bytes, revision_offset)?,
                };
                let name_offset_in_row = 4 + 2 + 2 + 2 + 2 + 4 + widths.blob;
                let field_offset = offset
                    .checked_add(name_offset_in_row)
                    .ok_or(PeMetadataError::Truncated)?;
                let name_index = read_index(bytes, field_offset, widths.str)?;
                let name = read_heap_string(bytes, strings_offset, strings_size, name_index)?;
                assembly_row = Some((name, version));
            }
            T_ASSEMBLY_REF => {
                let name_offset_in_row = 2 + 2 + 2 + 2 + 4 + widths.blob;
                for row in 0..count as usize {
                    let row_offset = nth_row_offset(offset, row, row_size)?;
                    let field_offset = row_offset
                        .checked_add(name_offset_in_row)
                        .ok_or(PeMetadataError::Truncated)?;
                    let name_index = read_index(bytes, field_offset, widths.str)?;
                    reference_names.push(read_heap_string(
                        bytes,
                        strings_offset,
                        strings_size,
                        name_index,
                    )?);
                }
            }
            T_TYPE_REF => {
                for row in 0..count as usize {
                    let row_offset = nth_row_offset(offset, row, row_size)?;
                    let scope = read_index(bytes, row_offset, widths.resolution_scope)?;
                    type_ref_scopes.push(scope);
                    let name_offset = row_offset
                        .checked_add(widths.resolution_scope)
                        .ok_or(PeMetadataError::Truncated)?;
                    let name_index = read_index(bytes, name_offset, widths.str)?;
                    type_ref_names.push(read_heap_string(
                        bytes,
                        strings_offset,
                        strings_size,
                        name_index,
                    )?);
                    let namespace_offset = name_offset
                        .checked_add(widths.str)
                        .ok_or(PeMetadataError::Truncated)?;
                    let namespace_index = read_index(bytes, namespace_offset, widths.str)?;
                    type_ref_namespaces.push(read_heap_string(
                        bytes,
                        strings_offset,
                        strings_size,
                        namespace_index,
                    )?);
                }
            }
            T_TYPE_DEF => {
                // Flags(4) + Name(str) + Namespace(str), then Extends,
                // FieldList, MethodList.
                let name_offset_in_row = 4;
                let namespace_offset_in_row = name_offset_in_row + widths.str;
                let extends_offset_in_row = 4 + 2 * widths.str;
                let method_list_offset_in_row =
                    extends_offset_in_row + widths.type_def_or_ref + widths.simple_field;
                for row in 0..count as usize {
                    let row_offset = nth_row_offset(offset, row, row_size)?;
                    let name_field = row_offset
                        .checked_add(name_offset_in_row)
                        .ok_or(PeMetadataError::Truncated)?;
                    let name_index = read_index(bytes, name_field, widths.str)?;
                    let name = read_heap_string(bytes, strings_offset, strings_size, name_index)?;
                    let namespace_field = row_offset
                        .checked_add(namespace_offset_in_row)
                        .ok_or(PeMetadataError::Truncated)?;
                    let namespace_index = read_index(bytes, namespace_field, widths.str)?;
                    let namespace =
                        read_heap_string(bytes, strings_offset, strings_size, namespace_index)?;
                    type_def_full_names.push(full_type_name(&namespace, &name));
                    let extends_field = row_offset
                        .checked_add(extends_offset_in_row)
                        .ok_or(PeMetadataError::Truncated)?;
                    let extends = read_index(bytes, extends_field, widths.type_def_or_ref)?;
                    extends_or_interface_targets.push(extends);
                    type_def_extends.push(extends);
                    let method_list_field = row_offset
                        .checked_add(method_list_offset_in_row)
                        .ok_or(PeMetadataError::Truncated)?;
                    let method_list =
                        read_index(bytes, method_list_field, widths.simple_method_def)?;
                    type_def_method_starts.push(method_list);
                }
            }
            T_METHOD_DEF => {
                // RVA(4) + ImplFlags(2) + Flags(2), then Name(str).
                let name_offset_in_row = 4 + 2 + 2;
                for row in 0..count as usize {
                    let row_offset = nth_row_offset(offset, row, row_size)?;
                    let name_field = row_offset
                        .checked_add(name_offset_in_row)
                        .ok_or(PeMetadataError::Truncated)?;
                    let name_index = read_index(bytes, name_field, widths.str)?;
                    method_def_names.push(read_heap_string(
                        bytes,
                        strings_offset,
                        strings_size,
                        name_index,
                    )?);
                }
            }
            T_INTERFACE_IMPL => {
                // Class(simple TypeDef index), then Interface.
                let interface_offset_in_row = widths.simple_type_def;
                for row in 0..count as usize {
                    let row_offset = nth_row_offset(offset, row, row_size)?;
                    let field_offset = row_offset
                        .checked_add(interface_offset_in_row)
                        .ok_or(PeMetadataError::Truncated)?;
                    let interface = read_index(bytes, field_offset, widths.type_def_or_ref)?;
                    extends_or_interface_targets.push(interface);
                }
            }
            T_MEMBER_REF => {
                // Class(MemberRefParent), then Name(str), then Signature(blob).
                for row in 0..count as usize {
                    let row_offset = nth_row_offset(offset, row, row_size)?;
                    let class = read_index(bytes, row_offset, widths.member_ref_parent)?;
                    member_ref_classes.push(class);
                    let sig_offset_in_row = widths.member_ref_parent + widths.str;
                    let sig_field = row_offset
                        .checked_add(sig_offset_in_row)
                        .ok_or(PeMetadataError::Truncated)?;
                    member_ref_signatures.push(read_index(bytes, sig_field, widths.blob)?);
                }
            }
            T_TYPE_SPEC => {
                for row in 0..count as usize {
                    let row_offset = nth_row_offset(offset, row, row_size)?;
                    type_spec_blobs.push(read_index(bytes, row_offset, widths.blob)?);
                }
            }
            T_CUSTOM_ATTRIBUTE => {
                // Parent(HasCustomAttribute), Type(CustomAttributeType), Value(blob).
                for row in 0..count as usize {
                    let row_offset = nth_row_offset(offset, row, row_size)?;
                    let parent = read_index(bytes, row_offset, widths.has_custom_attribute)?;
                    let type_field = row_offset
                        .checked_add(widths.has_custom_attribute)
                        .ok_or(PeMetadataError::Truncated)?;
                    let attr_type = read_index(bytes, type_field, widths.custom_attribute_type)?;
                    let value_field = type_field
                        .checked_add(widths.custom_attribute_type)
                        .ok_or(PeMetadataError::Truncated)?;
                    let value_blob_index = read_index(bytes, value_field, widths.blob)?;
                    collect_runtime_patch_attr(
                        RuntimePatchAttrCandidate {
                            parent,
                            attr_type,
                            value_blob_index,
                        },
                        bytes,
                        blob_offset,
                        blob_size,
                        &member_ref_classes,
                        &member_ref_signatures,
                        &type_ref_names,
                        &mut runtime_patch_attrs,
                    );
                    collect_runtime_patch_kind_attr(
                        parent,
                        attr_type,
                        &member_ref_classes,
                        &type_ref_names,
                        &mut method_kind_attrs,
                    );
                }
            }
            _ => {}
        }

        offset = nth_row_offset(offset, count as usize, row_size)?;
    }

    let (raw_name, version) = assembly_row.ok_or(PeMetadataError::MissingAssemblyRow)?;
    let name = raw_name.to_lowercase();

    let type_spec_assembly_refs: Vec<Option<usize>> = type_spec_blobs
        .iter()
        .map(|&blob_index| {
            resolve_type_spec_generic_base(
                bytes,
                blob_offset,
                blob_size,
                blob_index,
                &type_ref_scopes,
            )
        })
        .collect();

    // Every TypeRef's own full name, indexed the same way `type_ref_names`/
    // `type_ref_scopes` already are — [`resolve_extends_target`]'s own
    // TypeRef case reads this, not `type_ref_names` alone, so a base type
    // living in another assembly keeps its own namespace.
    let type_ref_full_names: Vec<String> = type_ref_names
        .iter()
        .zip(&type_ref_namespaces)
        .map(|(name, namespace)| full_type_name(namespace, name))
        .collect();
    let type_hierarchy: Vec<(String, Option<String>)> =
        type_def_full_names
            .iter()
            .cloned()
            .zip(type_def_extends.iter().map(|&raw| {
                resolve_extends_target(raw, &type_def_full_names, &type_ref_full_names)
            }))
            .collect();

    let mut load_time_by_ref_row = vec![false; reference_names.len()];
    for &target in &extends_or_interface_targets {
        if let Some(ref_row) = resolve_type_def_or_ref_to_assembly_ref(
            target,
            &type_ref_scopes,
            &type_spec_assembly_refs,
        ) && let Some(flag) = load_time_by_ref_row.get_mut(ref_row)
        {
            *flag = true;
        }
    }

    let mut merged: Vec<AssemblyReference> = Vec::new();
    let mut index_by_name: HashMap<String, usize> = HashMap::new();
    for (raw_name, load_time) in reference_names.into_iter().zip(load_time_by_ref_row) {
        let name = raw_name.to_lowercase();
        match index_by_name.get(&name) {
            Some(&existing) => merged[existing].load_time |= load_time,
            None => {
                index_by_name.insert(name.clone(), merged.len());
                merged.push(AssemblyReference { name, load_time });
            }
        }
    }

    let runtime_patches = combine_runtime_patches(
        &runtime_patch_attrs,
        &type_def_method_starts,
        &method_def_names,
        &method_kind_attrs,
    );

    Ok(AssemblyMetadata {
        name,
        references: merged,
        version,
        runtime_patches,
        type_hierarchy,
    })
}

/// Decodes a coded-index raw value into its `(tag, 1-based row)` pair.
pub(super) fn decode_coded_index(value: u32, tag_bits: u32) -> (u32, u32) {
    let mask = (1u32 << tag_bits) - 1;
    (value & mask, value >> tag_bits)
}

/// `"{namespace}.{name}"`, or bare `name` when `namespace` is empty (the
/// global namespace) — every [`AssemblyMetadata::type_hierarchy`] key and
/// value uses this same shape, matching how RimWorld's own
/// `GenTypes.GetTypeInAnyAssembly` resolves a dotted XML def-type tag.
fn full_type_name(namespace: &str, name: &str) -> String {
    if namespace.is_empty() {
        name.to_string()
    } else {
        format!("{namespace}.{name}")
    }
}

/// Resolves one `TypeDef.Extends` raw `TypeDefOrRef` coded-index value to
/// its target's own full name — `None` for a `TypeDefOrRef` value of `0`
/// (ECMA-335's "not present": an interface, or a type with no base at
/// all), a `TypeSpec` target (a generic instantiation, e.g. `MyDef<T>` —
/// this reader has no generic-argument-aware name to give it), or a row
/// index this assembly's own tables don't actually contain (malformed
/// input). A `TypeDef` target resolves within `type_def_full_names`
/// (same assembly); a `TypeRef` target resolves within
/// `type_ref_full_names` (another assembly, by name only — this reader
/// never opens a referenced assembly to confirm the type really exists
/// there).
fn resolve_extends_target(
    raw: u32,
    type_def_full_names: &[String],
    type_ref_full_names: &[String],
) -> Option<String> {
    if raw == 0 {
        return None;
    }
    let (tag, row) = decode_coded_index(raw, TYPE_DEF_OR_REF_TAG_BITS);
    let index = (row as usize).checked_sub(1)?;
    if tag == TYPE_DEF_OR_REF_TAG_TYPE_REF {
        type_ref_full_names.get(index).cloned()
    } else if tag == TYPE_DEF_OR_REF_TAG_TYPE_SPEC {
        None
    } else {
        // Tag 0: TypeDef, the only remaining `TypeDefOrRef` tag.
        type_def_full_names.get(index).cloned()
    }
}
