//! Tests for the PE metadata reader.

use super::headers::{
    Section, compressed_uint, read_blob, read_heap_string, read_padded_cstr, read_ser_string,
    read_u32, rva_to_offset,
};
use super::refs::{
    ArgKind, AttrOwner, RuntimePatchArg, RuntimePatchAttrRaw, collect_runtime_patch_kind_attr,
    combine_runtime_patches, decode_attribute_args, member_ref_ctor_param_count, method_kind,
    method_range_for_type_def, normalize_type_name, owning_type_def,
    resolve_type_def_or_ref_to_assembly_ref, resolve_type_ref_to_assembly_ref,
    resolve_type_spec_generic_base, runtime_patch_ctor_member_ref_row,
    runtime_patch_kind_attribute,
};
use super::tables::{
    CUSTOM_ATTRIBUTE_TYPE_TAG_MEMBER_REF, HAS_CUSTOM_ATTRIBUTE,
    HAS_CUSTOM_ATTRIBUTE_TAG_METHOD_DEF, HAS_CUSTOM_ATTRIBUTE_TAG_TYPE_DEF,
    MEMBER_REF_PARENT_TAG_TYPE_REF, RESOLUTION_SCOPE_TAG_ASSEMBLY_REF, RESOLUTION_SCOPE_TAG_BITS,
    RESOLUTION_SCOPE_TAG_TYPE_REF, T_TYPE_DEF, TYPE_DEF_OR_REF, TYPE_DEF_OR_REF_TAG_BITS,
    TYPE_DEF_OR_REF_TAG_TYPE_REF, TYPE_DEF_OR_REF_TAG_TYPE_SPEC, Widths, coded_width,
    decode_coded_index, nth_row_offset, row_size_for_table, simple_width,
};
use super::*;
use crate::domain::RuntimePatchKind;
use std::collections::HashMap;

#[test]
fn read_u32_round_trips_little_endian_bytes() {
    let bytes = [0x78, 0x56, 0x34, 0x12];
    assert_eq!(read_u32(&bytes, 0).unwrap(), 0x1234_5678);
}

#[test]
fn bounds_checked_reads_error_instead_of_panicking() {
    let bytes = [0u8; 2];
    assert_eq!(read_u32(&bytes, 0), Err(PeMetadataError::Truncated));
    assert_eq!(
        read_u32(&bytes, usize::MAX - 1),
        Err(PeMetadataError::Truncated)
    );
}

#[test]
fn simple_width_is_2_bytes_under_64k_rows_else_4() {
    let mut counts = [0u32; 64];
    counts[T_TYPE_DEF as usize] = 100;
    assert_eq!(simple_width(&counts, T_TYPE_DEF), 2);
    counts[T_TYPE_DEF as usize] = 70_000;
    assert_eq!(simple_width(&counts, T_TYPE_DEF), 4);
}

#[test]
fn coded_width_accounts_for_tag_bits() {
    let mut counts = [0u32; 64];
    // TypeDefOrRef has 2 tag bits, so 2 bytes fits up to 2^14 - 1 rows.
    counts[T_TYPE_DEF as usize] = (1 << 14) - 1;
    assert_eq!(
        coded_width(&counts, TYPE_DEF_OR_REF.0, TYPE_DEF_OR_REF.1),
        2
    );
    counts[T_TYPE_DEF as usize] = 1 << 14;
    assert_eq!(
        coded_width(&counts, TYPE_DEF_OR_REF.0, TYPE_DEF_OR_REF.1),
        4
    );
}

#[test]
fn read_padded_cstr_pads_name_length_to_4_bytes() {
    // "#~\0" is 3 bytes -> pads to 4.
    let bytes = b"#~\0\0extra";
    let (name, len) = read_padded_cstr(bytes, 0).unwrap();
    assert_eq!(name, "#~");
    assert_eq!(len, 4);
}

#[test]
fn read_padded_cstr_pads_to_next_multiple_of_4() {
    // "#Strings\0" is 9 bytes -> pads to 12.
    let bytes = b"#Strings\0\0\0\0tail";
    let (name, len) = read_padded_cstr(bytes, 0).unwrap();
    assert_eq!(name, "#Strings");
    assert_eq!(len, 12);
}

#[test]
fn read_heap_string_reads_null_terminated_utf8() {
    let heap = b"\0Foo\0Other\0";
    let s = read_heap_string(heap, 0, heap.len(), 1).unwrap();
    assert_eq!(s, "Foo");
}

#[test]
fn read_heap_string_rejects_index_past_heap_size() {
    let heap = b"\0Foo\0";
    assert_eq!(
        read_heap_string(heap, 0, heap.len(), 100),
        Err(PeMetadataError::Truncated)
    );
}

#[test]
fn rva_to_offset_maps_into_the_containing_section() {
    let sections = vec![Section {
        virtual_address: 0x2000,
        virtual_size: 0x1000,
        size_of_raw_data: 0x1000,
        pointer_to_raw_data: 0x400,
    }];
    assert_eq!(rva_to_offset(&sections, 0x2010).unwrap(), 0x410);
}

#[test]
fn rva_to_offset_rejects_rva_outside_every_section() {
    let sections = vec![Section {
        virtual_address: 0x2000,
        virtual_size: 0x1000,
        size_of_raw_data: 0x1000,
        pointer_to_raw_data: 0x400,
    }];
    assert_eq!(
        rva_to_offset(&sections, 0x5000),
        Err(PeMetadataError::RvaOutOfRange(0x5000))
    );
}

/// A hostile section header with `PointerToRawData = u32::MAX` must not
/// overflow the `pointer_to_raw_data + (rva - virtual_address)` addition
/// (unchecked, it panics in a debug build). Must return a typed error.
#[test]
fn rva_to_offset_rejects_pointer_to_raw_data_overflow_instead_of_panicking() {
    let sections = vec![Section {
        virtual_address: 0x1000,
        virtual_size: 0x2000,
        size_of_raw_data: 0x2000,
        pointer_to_raw_data: u32::MAX,
    }];
    assert_eq!(
        rva_to_offset(&sections, 0x1010),
        Err(PeMetadataError::RvaOutOfRange(0x1010))
    );
}

/// Truncation sweep: feeding `read` every possible byte-length prefix
/// of a synthetic (invalid past the DOS header) buffer must never
/// panic, only ever return a typed error.
#[test]
fn read_never_panics_on_any_truncation_of_hostile_input() {
    let mut buf = vec![0u8; 1024];
    buf[0] = b'M';
    buf[1] = b'Z';
    buf[0x3C..0x40].copy_from_slice(&0x80u32.to_le_bytes());
    buf[0x80..0x84].copy_from_slice(b"PE\0\0");
    // Bogus but plausible-looking COFF/optional header bytes so the
    // parser walks deep into the structure before running out of
    // buffer, exercising more of the bounds-checked path per prefix.
    for (i, byte) in buf.iter_mut().enumerate().skip(0x84).take(200) {
        *byte = (i % 256) as u8;
    }

    for len in 0..=buf.len() {
        let _ = read(&buf[..len]);
    }
}

#[test]
fn read_rejects_non_pe_input() {
    assert_eq!(
        read(b"not a pe file"),
        Err(PeMetadataError::InvalidPeSignature)
    );
}

#[test]
fn row_size_for_table_rejects_unsupported_table_ids() {
    let widths = Widths::compute(&[0u32; 64], false, false, false);
    assert_eq!(
        row_size_for_table(0x03, &widths),
        Err(PeMetadataError::UnsupportedTable(0x03))
    );
}

fn encode(tag: u32, row: u32, tag_bits: u32) -> u32 {
    (row << tag_bits) | tag
}

#[test]
fn decode_coded_index_splits_tag_and_row() {
    let value = encode(
        RESOLUTION_SCOPE_TAG_ASSEMBLY_REF,
        5,
        RESOLUTION_SCOPE_TAG_BITS,
    );
    assert_eq!(
        decode_coded_index(value, RESOLUTION_SCOPE_TAG_BITS),
        (RESOLUTION_SCOPE_TAG_ASSEMBLY_REF, 5)
    );
}

#[test]
fn type_ref_resolves_directly_to_assembly_ref() {
    // TypeRef row 1's scope is AssemblyRef row 3.
    let scopes = vec![encode(RESOLUTION_SCOPE_TAG_ASSEMBLY_REF, 3, 2)];
    assert_eq!(resolve_type_ref_to_assembly_ref(1, &scopes), Some(2));
}

/// A nested type's `TypeRef` names its enclosing type's `TypeRef` as
/// its scope — walking that chain must reach the outermost type's
/// `AssemblyRef`.
#[test]
fn type_ref_walks_nested_chain_to_the_outermost_assembly_ref() {
    let scopes = vec![
        encode(RESOLUTION_SCOPE_TAG_TYPE_REF, 2, 2), // row 1 (innermost) -> row 2
        encode(RESOLUTION_SCOPE_TAG_ASSEMBLY_REF, 4, 2), // row 2 (outermost) -> AssemblyRef 4
    ];
    assert_eq!(resolve_type_ref_to_assembly_ref(1, &scopes), Some(3));
}

#[test]
fn type_ref_resolving_to_module_or_module_ref_is_not_an_assembly_reference() {
    let scopes = vec![encode(0, 1, 2)]; // tag 0 = Module
    assert_eq!(resolve_type_ref_to_assembly_ref(1, &scopes), None);
}

/// Regression guard: a hostile or malformed cyclic `ResolutionScope`
/// chain (row 1 -> row 2 -> row 1 -> ...) must terminate with `None`
/// instead of looping forever.
#[test]
fn type_ref_cyclic_chain_terminates_instead_of_looping_forever() {
    let scopes = vec![
        encode(RESOLUTION_SCOPE_TAG_TYPE_REF, 2, 2), // row 1 -> row 2
        encode(RESOLUTION_SCOPE_TAG_TYPE_REF, 1, 2), // row 2 -> row 1
    ];
    assert_eq!(resolve_type_ref_to_assembly_ref(1, &scopes), None);
}

#[test]
fn extends_targeting_a_type_def_is_not_a_cross_assembly_reference() {
    let coded = encode(0, 1, TYPE_DEF_OR_REF_TAG_BITS); // tag 0 = TypeDef
    assert_eq!(
        resolve_type_def_or_ref_to_assembly_ref(coded, &[], &[]),
        None
    );
}

/// A `TypeSpec` target resolves through the precomputed per-row table
/// ([`resolve_type_spec_generic_base`]'s own tests cover how that table gets
/// built) rather than being ignored.
#[test]
fn extends_targeting_a_type_spec_resolves_via_the_precomputed_table() {
    let coded = encode(TYPE_DEF_OR_REF_TAG_TYPE_SPEC, 1, TYPE_DEF_OR_REF_TAG_BITS);
    let type_spec_assembly_refs = vec![Some(2usize)];
    assert_eq!(
        resolve_type_def_or_ref_to_assembly_ref(coded, &[], &type_spec_assembly_refs),
        Some(2)
    );
}

#[test]
fn extends_targeting_a_type_spec_with_no_resolved_target_is_none() {
    let coded = encode(TYPE_DEF_OR_REF_TAG_TYPE_SPEC, 1, TYPE_DEF_OR_REF_TAG_BITS);
    let type_spec_assembly_refs = vec![None];
    assert_eq!(
        resolve_type_def_or_ref_to_assembly_ref(coded, &[], &type_spec_assembly_refs),
        None
    );
}

#[test]
fn extends_targeting_a_type_ref_resolves_through_to_its_assembly_ref() {
    let scopes = vec![encode(RESOLUTION_SCOPE_TAG_ASSEMBLY_REF, 1, 2)];
    let coded = encode(TYPE_DEF_OR_REF_TAG_TYPE_REF, 1, TYPE_DEF_OR_REF_TAG_BITS);
    assert_eq!(
        resolve_type_def_or_ref_to_assembly_ref(coded, &scopes, &[]),
        Some(0)
    );
}

#[test]
fn null_extends_reference_resolves_to_none() {
    let coded = encode(TYPE_DEF_OR_REF_TAG_TYPE_REF, 0, TYPE_DEF_OR_REF_TAG_BITS);
    assert_eq!(
        resolve_type_def_or_ref_to_assembly_ref(coded, &[], &[]),
        None
    );
}

// ----------------------------------------------------------------- TypeSpec
// generic-base unwrapping
// -----------------------------------------------------------------

/// Builds a minimal `#Blob`-heap-shaped byte buffer holding exactly
/// one blob (a compressed length prefix followed by `content`) at
/// index 0, for feeding directly to blob-index-taking functions.
fn one_blob_heap(content: &[u8]) -> Vec<u8> {
    let mut heap = vec![content.len() as u8];
    heap.extend_from_slice(content);
    heap
}

#[test]
fn type_spec_generic_inst_over_a_type_ref_resolves_to_its_assembly_ref() {
    // GENERICINST(0x15) CLASS(0x12) <coded TypeRef row 1> <genArgCount 1> <arg: irrelevant, not read>
    let signature = [
        0x15,
        0x12,
        encode(TYPE_DEF_OR_REF_TAG_TYPE_REF, 1, 2) as u8,
        0x01,
        0x1C,
    ];
    let blob_heap = one_blob_heap(&signature);
    let scopes = vec![encode(RESOLUTION_SCOPE_TAG_ASSEMBLY_REF, 3, 2)];

    let resolved = resolve_type_spec_generic_base(&blob_heap, 0, blob_heap.len(), 0, &scopes);

    assert_eq!(resolved, Some(2));
}

#[test]
fn type_spec_generic_inst_over_a_local_type_def_is_not_load_time() {
    // GENERICINST(0x15) CLASS(0x12) <coded TypeDef row 1, tag 0>
    let signature = [0x15, 0x12, encode(0, 1, 2) as u8];
    let blob_heap = one_blob_heap(&signature);

    let resolved = resolve_type_spec_generic_base(&blob_heap, 0, blob_heap.len(), 0, &[]);

    assert_eq!(resolved, None);
}

#[test]
fn type_spec_that_is_not_a_generic_instantiation_is_none() {
    // A plain SZARRAY(0x1D) signature, not GENERICINST.
    let signature = [0x1D, 0x0E];
    let blob_heap = one_blob_heap(&signature);

    let resolved = resolve_type_spec_generic_base(&blob_heap, 0, blob_heap.len(), 0, &[]);

    assert_eq!(resolved, None);
}

/// The blob's own length prefix cuts the signature off before the
/// coded `TypeDefOrRef` index byte the `GENERICINST`/`CLASS` prefix
/// promises — a truncated blob, not a well-formed one with an
/// unsupported shape.
#[test]
fn type_spec_generic_inst_with_a_truncated_blob_is_none() {
    // Length prefix 2: only GENERICINST(0x15) and CLASS(0x12) fit,
    // no room left for the coded index compressed_uint needs.
    let blob_heap = [0x02, 0x15, 0x12];

    let resolved = resolve_type_spec_generic_base(&blob_heap, 0, blob_heap.len(), 0, &[]);

    assert_eq!(resolved, None);
}

#[test]
fn nth_row_offset_rejects_overflowing_arithmetic_instead_of_panicking() {
    assert_eq!(
        nth_row_offset(usize::MAX, 2, 10),
        Err(PeMetadataError::Truncated)
    );
}

// -----------------------------------------------------------------
// Compressed integers and the blob heap
// -----------------------------------------------------------------

#[test]
fn compressed_uint_decodes_the_one_byte_form() {
    assert_eq!(compressed_uint(&[0x05], 0), Ok((5, 1)));
}

#[test]
fn compressed_uint_decodes_the_two_byte_form() {
    // 0x80 | high 6 bits, then the low byte: encodes 0x80 (128).
    assert_eq!(compressed_uint(&[0x80, 0x80], 0), Ok((128, 2)));
}

#[test]
fn compressed_uint_decodes_the_four_byte_form() {
    // 0xC0 | high 5 bits, then 3 more bytes: encodes 0x4000 (16384).
    assert_eq!(
        compressed_uint(&[0xC0, 0x00, 0x40, 0x00], 0),
        Ok((0x4000, 4))
    );
}

#[test]
fn compressed_uint_rejects_an_invalid_leading_byte() {
    assert_eq!(
        compressed_uint(&[0xFF], 0),
        Err(PeMetadataError::InvalidCompressedInteger)
    );
}

#[test]
fn read_blob_reads_a_length_prefixed_entry() {
    let heap = [0x03, b'a', b'b', b'c'];
    assert_eq!(read_blob(&heap, 0, heap.len(), 0), Ok(b"abc".as_slice()));
}

#[test]
fn read_blob_rejects_an_index_past_the_heap_size() {
    let heap = [0x03, b'a', b'b', b'c'];
    assert_eq!(
        read_blob(&heap, 0, heap.len(), 100),
        Err(PeMetadataError::Truncated)
    );
}

/// Regression guard for the `#Blob`-heap-windowing fix: the length
/// prefix claims 5 content bytes, but the heap is declared only 2
/// bytes long (prefix + 1 byte) — the file has more bytes right after
/// it that an unwindowed read would happily treat as blob content.
#[test]
fn read_blob_rejects_a_length_prefix_that_overruns_the_declared_heap_size() {
    let mut file = vec![0x05, b'a'];
    file.extend_from_slice(b"bcdefghij"); // bytes belonging to whatever follows the #Blob heap in the file
    assert_eq!(read_blob(&file, 0, 2, 0), Err(PeMetadataError::Truncated));
}

#[test]
fn read_ser_string_decodes_a_length_prefixed_utf8_string() {
    let bytes = [0x03, b'F', b'o', b'o'];
    assert_eq!(read_ser_string(&bytes, 0), Ok((Some("Foo".to_string()), 4)));
}

#[test]
fn read_ser_string_decodes_the_null_marker() {
    assert_eq!(read_ser_string(&[0xFF], 0), Ok((None, 1)));
}

// -----------------------------------------------------------------
// Runtime-patch custom-attribute decoding
// -----------------------------------------------------------------

/// A `MethodDefSig` for a `.ctor` taking `param_types` (each either
/// `ELEMENT_TYPE_STRING` or `ELEMENT_TYPE_CLASS` + a coded index),
/// HASTHIS calling convention, void return.
fn ctor_signature(param_types: &[&[u8]]) -> Vec<u8> {
    let mut sig = vec![0x20]; // HASTHIS
    sig.push(param_types.len() as u8); // ParamCount (fits in 1 byte here)
    sig.push(0x01); // RetType: void
    for param in param_types {
        sig.extend_from_slice(param);
    }
    sig
}

const ELEMENT_TYPE_STRING: &[u8] = &[0x0E];
// CLASS + a throwaway 1-byte coded index (value doesn't matter: only
// the shape, string-vs-class, is read for a signature's own param count).
const ELEMENT_TYPE_CLASS_ARG: &[u8] = &[0x12, 0x01];

#[test]
fn member_ref_ctor_param_count_reads_a_type_and_string_overload() {
    let sig = ctor_signature(&[ELEMENT_TYPE_CLASS_ARG, ELEMENT_TYPE_STRING]);
    let heap = one_blob_heap(&sig);
    assert_eq!(
        member_ref_ctor_param_count(&heap, 0, heap.len(), 0),
        Some(vec![ArgKind::Type, ArgKind::Text])
    );
}

/// An unsupported shape in the very first parameter position (no
/// `Type`/`string` prefix at all) yields an empty prefix rather than
/// `None` — [`collect_runtime_patch_attr`]'s own `is_empty()` check is what
/// drops it.
#[test]
fn member_ref_ctor_param_count_yields_an_empty_prefix_for_a_leading_unsupported_parameter() {
    // ELEMENT_TYPE_I4 (0x08) — an enum-backed `MethodType` argument, out of
    // this decoder's scope, with nothing recognizable before it.
    let sig = ctor_signature(&[&[0x08]]);
    let heap = one_blob_heap(&sig);
    assert_eq!(
        member_ref_ctor_param_count(&heap, 0, heap.len(), 0),
        Some(vec![])
    );
}

// Trailing-arg overloads: a `Type`/`string` prefix the library actually uses,
// followed by a `params Type[] argumentTypes` (SZARRAY, 0x1D) or `MethodType
// methodType` (an enum, encoded as VALUETYPE, 0x11) argument this decoder
// never reads.
const TRAILING_TYPE_ARRAY: &[u8] = &[0x1D, 0x12, 0x01]; // SZARRAY of CLASS
const TRAILING_METHOD_TYPE: &[u8] = &[0x11, 0x01]; // VALUETYPE (enum)

#[test]
fn member_ref_ctor_param_count_stops_at_a_trailing_type_array_argument() {
    // (Type, string, params Type[] argumentTypes) overload
    let sig = ctor_signature(&[
        ELEMENT_TYPE_CLASS_ARG,
        ELEMENT_TYPE_STRING,
        TRAILING_TYPE_ARRAY,
    ]);
    let heap = one_blob_heap(&sig);
    assert_eq!(
        member_ref_ctor_param_count(&heap, 0, heap.len(), 0),
        Some(vec![ArgKind::Type, ArgKind::Text])
    );
}

#[test]
fn member_ref_ctor_param_count_stops_at_a_trailing_method_type_after_type_and_string() {
    // (Type, string, MethodType methodType) overload
    let sig = ctor_signature(&[
        ELEMENT_TYPE_CLASS_ARG,
        ELEMENT_TYPE_STRING,
        TRAILING_METHOD_TYPE,
    ]);
    let heap = one_blob_heap(&sig);
    assert_eq!(
        member_ref_ctor_param_count(&heap, 0, heap.len(), 0),
        Some(vec![ArgKind::Type, ArgKind::Text])
    );
}

#[test]
fn member_ref_ctor_param_count_stops_at_a_trailing_method_type_after_two_strings() {
    // (string typeName, string methodName, MethodType methodType) overload
    let sig = ctor_signature(&[
        ELEMENT_TYPE_STRING,
        ELEMENT_TYPE_STRING,
        TRAILING_METHOD_TYPE,
    ]);
    let heap = one_blob_heap(&sig);
    assert_eq!(
        member_ref_ctor_param_count(&heap, 0, heap.len(), 0),
        Some(vec![ArgKind::Text, ArgKind::Text])
    );
}

#[test]
fn decode_attribute_args_reads_the_fixed_ser_strings_after_the_prolog() {
    let mut value = vec![0x01, 0x00]; // prolog
    value.push(0x03);
    value.extend_from_slice(b"Foo");
    value.push(0x01);
    value.extend_from_slice(b"M");
    let args = decode_attribute_args(&value, &[ArgKind::Type, ArgKind::Text]).unwrap();
    assert_eq!(args[0].value.as_deref(), Some("Foo"));
    assert_eq!(args[1].value.as_deref(), Some("M"));
}

/// A `System.Type` fixed argument is assembly-qualified in the blob; decoding
/// must strip it down to the bare type name.
#[test]
fn decode_attribute_args_normalizes_an_assembly_qualified_type_argument() {
    let qualified =
        b"Verse.Pawn, Assembly-CSharp, Version=1.0.0.0, Culture=neutral, PublicKeyToken=null";
    let mut value = vec![0x01, 0x00]; // prolog
    value.push(qualified.len() as u8);
    value.extend_from_slice(qualified);

    let args = decode_attribute_args(&value, &[ArgKind::Type]).unwrap();

    assert_eq!(args[0].value.as_deref(), Some("Verse.Pawn"));
}

#[test]
fn decode_attribute_args_rejects_a_bad_prolog() {
    let value = [0x00, 0x00];
    assert!(decode_attribute_args(&value, &[]).is_none());
}

#[test]
fn normalize_type_name_strips_everything_from_the_first_comma() {
    assert_eq!(
        normalize_type_name(
            "Verse.Pawn, Assembly-CSharp, Version=1.0.0.0, Culture=neutral, PublicKeyToken=null"
        ),
        "Verse.Pawn"
    );
}

#[test]
fn normalize_type_name_is_a_no_op_for_an_already_bare_name() {
    assert_eq!(normalize_type_name("Verse.Pawn"), "Verse.Pawn");
}

#[test]
fn runtime_patch_ctor_member_ref_row_recognizes_a_member_ref_named_harmony_patch() {
    let type_ref_names = vec!["HarmonyPatch".to_string()];
    // MemberRefParent tag 1 = TypeRef, row 1.
    let member_ref_classes = vec![encode(MEMBER_REF_PARENT_TAG_TYPE_REF, 1, 3)];
    // CustomAttributeType tag 3 = MemberRef, row 1.
    let attr_type = encode(CUSTOM_ATTRIBUTE_TYPE_TAG_MEMBER_REF, 1, 3);

    assert_eq!(
        runtime_patch_ctor_member_ref_row(attr_type, &member_ref_classes, &type_ref_names),
        Some(0)
    );
}

#[test]
fn runtime_patch_ctor_member_ref_row_rejects_a_differently_named_attribute() {
    let type_ref_names = vec!["HarmonyPriority".to_string()];
    let member_ref_classes = vec![encode(MEMBER_REF_PARENT_TAG_TYPE_REF, 1, 3)];
    let attr_type = encode(CUSTOM_ATTRIBUTE_TYPE_TAG_MEMBER_REF, 1, 3);

    assert_eq!(
        runtime_patch_ctor_member_ref_row(attr_type, &member_ref_classes, &type_ref_names),
        None
    );
}

/// Each of the library's three bare kind-marker attributes is recognized by
/// `TypeRef` name, the same shape the patch-target attribute itself uses.
#[test]
fn runtime_patch_kind_attribute_recognizes_all_three_marker_names() {
    let member_ref_classes = vec![encode(MEMBER_REF_PARENT_TAG_TYPE_REF, 1, 3)];
    let attr_type = encode(CUSTOM_ATTRIBUTE_TYPE_TAG_MEMBER_REF, 1, 3);

    for (name, expected) in [
        ("HarmonyPrefix", RuntimePatchKind::Prefix),
        ("HarmonyPostfix", RuntimePatchKind::Postfix),
        ("HarmonyTranspiler", RuntimePatchKind::Transpiler),
    ] {
        let type_ref_names = vec![name.to_string()];
        assert_eq!(
            runtime_patch_kind_attribute(attr_type, &member_ref_classes, &type_ref_names),
            Some(expected),
            "expected {name} to decode as {expected:?}"
        );
    }
}

#[test]
fn runtime_patch_kind_attribute_rejects_a_differently_named_attribute() {
    let type_ref_names = vec!["HarmonyPatch".to_string()];
    let member_ref_classes = vec![encode(MEMBER_REF_PARENT_TAG_TYPE_REF, 1, 3)];
    let attr_type = encode(CUSTOM_ATTRIBUTE_TYPE_TAG_MEMBER_REF, 1, 3);

    assert_eq!(
        runtime_patch_kind_attribute(attr_type, &member_ref_classes, &type_ref_names),
        None
    );
}

/// A kind-marker attribute owned by a `TypeDef` (a class, never how the library's
/// own attributes are applied) is not recorded — only a `MethodDef` owner is.
#[test]
fn collect_runtime_patch_kind_attr_only_records_a_method_owner() {
    let type_ref_names = vec!["HarmonyPostfix".to_string()];
    let member_ref_classes = vec![encode(MEMBER_REF_PARENT_TAG_TYPE_REF, 1, 3)];
    let attr_type = encode(CUSTOM_ATTRIBUTE_TYPE_TAG_MEMBER_REF, 1, 3);
    let method_parent = encode(
        HAS_CUSTOM_ATTRIBUTE_TAG_METHOD_DEF,
        5,
        HAS_CUSTOM_ATTRIBUTE.1,
    );
    let type_parent = encode(HAS_CUSTOM_ATTRIBUTE_TAG_TYPE_DEF, 1, HAS_CUSTOM_ATTRIBUTE.1);

    let mut out = HashMap::new();
    collect_runtime_patch_kind_attr(
        method_parent,
        attr_type,
        &member_ref_classes,
        &type_ref_names,
        &mut out,
    );
    collect_runtime_patch_kind_attr(
        type_parent,
        attr_type,
        &member_ref_classes,
        &type_ref_names,
        &mut out,
    );

    assert_eq!(out, HashMap::from([(5, RuntimePatchKind::Postfix)]));
}

#[test]
fn method_range_for_type_def_spans_up_to_the_next_types_start() {
    let starts = vec![1, 5, 8];
    assert_eq!(method_range_for_type_def(0, &starts, 10), 1..5);
    assert_eq!(method_range_for_type_def(1, &starts, 10), 5..8);
}

#[test]
fn method_range_for_type_def_reaches_the_end_of_the_table_for_the_last_type() {
    let starts = vec![1, 5];
    assert_eq!(method_range_for_type_def(1, &starts, 10), 5..11);
}

#[test]
fn method_range_for_type_def_is_empty_for_a_type_owning_no_methods() {
    // Row 0 and row 1 share the same MethodList start: row 0 owns zero
    // methods, per ECMA-335's "next distinct start" reading.
    let starts = vec![3, 3];
    assert_eq!(method_range_for_type_def(0, &starts, 10), 3..3);
}

#[test]
fn method_kind_prefers_an_explicit_attribute_over_the_conventional_name() {
    let method_def_names = vec!["Prefix".to_string()];
    let method_kind_attrs = HashMap::from([(1, RuntimePatchKind::Postfix)]);
    assert_eq!(
        method_kind(1, &method_def_names, &method_kind_attrs),
        Some(RuntimePatchKind::Postfix)
    );
}

#[test]
fn method_kind_falls_back_to_the_conventional_name_with_no_attribute() {
    let method_def_names = vec!["Transpiler".to_string()];
    assert_eq!(
        method_kind(1, &method_def_names, &HashMap::new()),
        Some(RuntimePatchKind::Transpiler)
    );
}

#[test]
fn method_kind_is_none_for_an_unrecognized_name_with_no_attribute() {
    let method_def_names = vec!["DoTheThing".to_string()];
    assert_eq!(method_kind(1, &method_def_names, &HashMap::new()), None);
}

fn runtime_patch_attr(owner: AttrOwner, args: &[(ArgKind, Option<&str>)]) -> RuntimePatchAttrRaw {
    RuntimePatchAttrRaw {
        owner,
        args: args
            .iter()
            .map(|&(kind, value)| RuntimePatchArg {
                kind,
                value: value.map(str::to_string),
            })
            .collect(),
    }
}

#[test]
fn combine_runtime_patches_takes_a_two_arg_class_level_attribute_directly() {
    let attrs = vec![runtime_patch_attr(
        AttrOwner::TypeDef(0),
        &[
            (ArgKind::Type, Some("Verse.Pawn")),
            (ArgKind::Text, Some("Kill")),
        ],
    )];
    let targets = combine_runtime_patches(&attrs, &[], &[], &HashMap::new());
    assert_eq!(
        targets,
        vec![RuntimePatchTarget {
            type_name: "Verse.Pawn".to_string(),
            method_name: "Kill".to_string(),
            kind: RuntimePatchKind::Unknown,
        }]
    );
}

/// An explicit transpiler kind-marker attribute on the implementing
/// method is recognized, whichever style associated the target (here, the
/// realistic class-level-type + method-level-name combo the previous test
/// above also exercises without a kind).
#[test]
fn combine_runtime_patches_recognizes_an_explicit_transpiler_attribute() {
    let type_def_method_starts = vec![5];
    let attrs = vec![
        runtime_patch_attr(
            AttrOwner::TypeDef(0),
            &[(ArgKind::Type, Some("Verse.Pawn"))],
        ),
        runtime_patch_attr(AttrOwner::MethodDef(5), &[(ArgKind::Text, Some("Kill"))]),
    ];
    let method_kind_attrs = HashMap::from([(5, RuntimePatchKind::Transpiler)]);

    let targets = combine_runtime_patches(&attrs, &type_def_method_starts, &[], &method_kind_attrs);

    assert_eq!(
        targets,
        vec![RuntimePatchTarget {
            type_name: "Verse.Pawn".to_string(),
            method_name: "Kill".to_string(),
            kind: RuntimePatchKind::Transpiler,
        }]
    );
}

/// Convention-based kind discovery: no attribute at all, just a method
/// literally named `Transpiler` inside the patched class.
#[test]
fn combine_runtime_patches_recognizes_a_conventionally_named_transpiler_method() {
    // TypeDef row 0 owns MethodDef rows [1, 3): row 1 is unrelated,
    // row 2 is literally named "Transpiler".
    let type_def_method_starts = vec![1, 3];
    let method_def_names = vec!["SomeOtherMethod".to_string(), "Transpiler".to_string()];
    let attrs = vec![runtime_patch_attr(
        AttrOwner::TypeDef(0),
        &[
            (ArgKind::Type, Some("Verse.Pawn")),
            (ArgKind::Text, Some("Kill")),
        ],
    )];

    let targets = combine_runtime_patches(
        &attrs,
        &type_def_method_starts,
        &method_def_names,
        &HashMap::new(),
    );

    assert_eq!(
        targets,
        vec![RuntimePatchTarget {
            type_name: "Verse.Pawn".to_string(),
            method_name: "Kill".to_string(),
            kind: RuntimePatchKind::Transpiler,
        }]
    );
}

/// A class implementing more than one role for the same association
/// (a `Prefix` and a `Postfix` sharing one class-level target)
/// produces one target per role found, not one target with an
/// arbitrarily chosen kind.
#[test]
fn combine_runtime_patches_emits_one_target_per_kind_when_a_class_implements_several() {
    // TypeDef row 0 owns MethodDef rows [1, 3).
    let type_def_method_starts = vec![1, 3];
    let method_def_names = vec!["Prefix".to_string(), "Postfix".to_string()];
    let attrs = vec![runtime_patch_attr(
        AttrOwner::TypeDef(0),
        &[
            (ArgKind::Type, Some("Verse.Pawn")),
            (ArgKind::Text, Some("Kill")),
        ],
    )];

    // `RuntimePatchKind`'s derive order (Prefix < Postfix) is what
    // makes this a stable expectation — `targets` comes back sorted by
    // the underlying `BTreeSet<(type, method, kind)>`.
    let targets = combine_runtime_patches(
        &attrs,
        &type_def_method_starts,
        &method_def_names,
        &HashMap::new(),
    );

    assert_eq!(
        targets,
        vec![
            RuntimePatchTarget {
                type_name: "Verse.Pawn".to_string(),
                method_name: "Kill".to_string(),
                kind: RuntimePatchKind::Prefix,
            },
            RuntimePatchTarget {
                type_name: "Verse.Pawn".to_string(),
                method_name: "Kill".to_string(),
                kind: RuntimePatchKind::Postfix,
            },
        ]
    );
}

/// The realistic shape: a class-level patch-target attribute naming a
/// type, and a method-level patch-target attribute naming a method, on a
/// `Prefix`/`Postfix` method inside it.
#[test]
fn combine_runtime_patches_merges_a_class_level_type_with_a_method_level_name() {
    // TypeDef row 0 (1-based row 1) owns MethodDef rows starting at 5.
    let type_def_method_starts = vec![5];
    let attrs = vec![
        runtime_patch_attr(
            AttrOwner::TypeDef(0),
            &[(ArgKind::Type, Some("Verse.Pawn"))],
        ),
        runtime_patch_attr(AttrOwner::MethodDef(5), &[(ArgKind::Text, Some("Kill"))]),
    ];

    let targets = combine_runtime_patches(&attrs, &type_def_method_starts, &[], &HashMap::new());

    assert_eq!(
        targets,
        vec![RuntimePatchTarget {
            type_name: "Verse.Pawn".to_string(),
            method_name: "Kill".to_string(),
            kind: RuntimePatchKind::Unknown,
        }]
    );
}

/// A method-level attribute naming only a method, with no class-level
/// attribute anywhere in the assembly to supply the type, contributes nothing
/// — the decoder never guesses the target via naming-convention scanning.
#[test]
fn combine_runtime_patches_drops_an_orphaned_method_level_attribute() {
    let type_def_method_starts = vec![5];
    let attrs = vec![runtime_patch_attr(
        AttrOwner::MethodDef(5),
        &[(ArgKind::Text, Some("Kill"))],
    )];

    assert!(
        combine_runtime_patches(&attrs, &type_def_method_starts, &[], &HashMap::new()).is_empty()
    );
}

/// the library's only single-`string`-argument constructor takes just
/// `(string methodName)` — a lone `Text` argument directly on the
/// class names the *method*, not the type, and needs a sibling `Type`
/// attribute on the same class to become a complete target.
#[test]
fn combine_runtime_patches_treats_a_class_level_single_text_arg_as_a_method_name() {
    let attrs = vec![
        runtime_patch_attr(
            AttrOwner::TypeDef(0),
            &[(ArgKind::Type, Some("Verse.Pawn"))],
        ),
        runtime_patch_attr(AttrOwner::TypeDef(0), &[(ArgKind::Text, Some("Kill"))]),
    ];

    let targets = combine_runtime_patches(&attrs, &[], &[], &HashMap::new());

    assert_eq!(
        targets,
        vec![RuntimePatchTarget {
            type_name: "Verse.Pawn".to_string(),
            method_name: "Kill".to_string(),
            kind: RuntimePatchKind::Unknown,
        }]
    );
}

/// The same pair of class-level attributes in the opposite table
/// order must produce the identical target — attribute rows carry no
/// ordering guarantee between a class's own attributes.
#[test]
fn combine_runtime_patches_combines_class_level_type_and_text_in_either_order() {
    let attrs = vec![
        runtime_patch_attr(AttrOwner::TypeDef(0), &[(ArgKind::Text, Some("Kill"))]),
        runtime_patch_attr(
            AttrOwner::TypeDef(0),
            &[(ArgKind::Type, Some("Verse.Pawn"))],
        ),
    ];

    let targets = combine_runtime_patches(&attrs, &[], &[], &HashMap::new());

    assert_eq!(
        targets,
        vec![RuntimePatchTarget {
            type_name: "Verse.Pawn".to_string(),
            method_name: "Kill".to_string(),
            kind: RuntimePatchKind::Unknown,
        }]
    );
}

/// A class-level single `Text` attribute with no sibling `Type`
/// attribute anywhere on the same class contributes nothing — same
/// "never guess" policy as an orphaned method-level attribute.
#[test]
fn combine_runtime_patches_drops_a_class_level_text_arg_with_no_sibling_type() {
    let attrs = vec![runtime_patch_attr(
        AttrOwner::TypeDef(0),
        &[(ArgKind::Text, Some("Kill"))],
    )];

    assert!(combine_runtime_patches(&attrs, &[], &[], &HashMap::new()).is_empty());
}

#[test]
fn owning_type_def_picks_the_last_type_with_a_start_at_or_before_the_method_row() {
    // Row 0 starts at 1, row 1 (an interface, no methods) also starts
    // at 3, row 2 starts at 3 too and is the real owner of method 3.
    let starts = vec![1, 3, 3];
    assert_eq!(owning_type_def(3, &starts), Some(2));
    assert_eq!(owning_type_def(1, &starts), Some(0));
}

#[test]
fn owning_type_def_is_none_before_the_first_type_starts() {
    let starts = vec![5];
    assert_eq!(owning_type_def(1, &starts), None);
}
