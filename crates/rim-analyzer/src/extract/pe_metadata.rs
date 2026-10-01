//! A minimal, dependency-free ECMA-335 metadata reader.
//!
//! We need three facts from each `.dll`: the assembly's own defined name
//! (`Assembly` table), the names it references (`AssemblyRef` table), and
//! — for each reference — whether it's resolved at DLL-load time or
//! lazily (see [`crate::domain::AssemblyReference`]). Rather than pull in
//! a full PE/CIL analysis crate (evaluated: `dotscope` — see the crate
//! README for why it was passed over), this walks just enough of the
//! format by hand: PE headers -> CLI header -> metadata root -> `#~`
//! tables stream -> `Assembly`/`AssemblyRef`/`TypeRef`/`TypeDef`/
//! `InterfaceImpl` rows.
//!
//! Load-time resolution is determined by walking every `TypeDef.Extends` and
//! `InterfaceImpl.Interface` field: when it names a `TypeRef` whose
//! `ResolutionScope` is (directly, or through a chain of nested-type
//! `TypeRef`s) an `AssemblyRef`, that `AssemblyRef` is a load-time reference
//! — RimWorld's `Assembly.GetTypes()` call after loading a DLL resolves every
//! type's base type and interfaces immediately, throwing (and dropping the
//! whole assembly) if the target isn't loaded yet. A `TypeSpec` target (a
//! class deriving from `OtherMod.Base<T>`) unwraps the same way when its
//! signature blob is a `GENERICINST` over a `TypeRef`
//! (`resolve_type_def_or_ref_to_assembly_ref`'s own doc comment has the
//! detail). Every other reference — method bodies, fields, attributes —
//! resolves lazily and tolerates any load order.
//!
//! Two more facts ride along with the same table walk: each assembly's own
//! version (Assembly table row — see [`AssemblyMetadata::version`]), and
//! every runtime-patch custom-attribute target it declares (see
//! [`AssemblyMetadata::runtime_patches`]).
//!
//! Every read is bounds-checked against the input slice (`.get(..)`,
//! `checked_add`); nothing here panics on malformed or hostile input —
//! errors propagate as [`PeMetadataError`] for the caller to turn into
//! the documented fallback (file stem, zero references, a warning).

use thiserror::Error;

use crate::domain::{AssemblyReference, AssemblyVersion, RuntimePatchTarget};
use headers::{read_bytes, read_metadata_root, read_sections, read_u16, read_u32, rva_to_offset};

mod headers;
mod refs;
mod tables;

// Sibling-file tests; `#[path]` keeps the module named `tests`.
#[cfg(test)]
#[path = "pe_metadata/pe_metadata_tests.rs"]
mod tests;

/// The assembly identity and references read from one DLL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssemblyMetadata {
    /// Lowercased assembly name (Assembly table).
    pub name: String,
    /// Deduplicated referenced assemblies (AssemblyRef table), each
    /// tagged with whether it's a load-time or lazy reference.
    pub references: Vec<AssemblyReference>,
    /// This assembly's own version (Assembly table row).
    pub version: AssemblyVersion,
    /// Every runtime-patch target this assembly's custom attributes
    /// declare — see `decode_runtime_patches`'s own doc comment for exactly
    /// which constructor shapes and class/method combinations are recognized.
    pub runtime_patches: Vec<RuntimePatchTarget>,
    /// Every type this assembly declares (`TypeDef` table), keyed by its
    /// own full (`Namespace.Name`) name, alongside its base type's own
    /// full name where resolvable (`None` for no base at all, a generic
    /// `TypeSpec` base, or a target this reader can't otherwise name —
    /// see `tables::resolve_extends_target`'s own doc comment). Read the
    /// same way `references` already is (`TypeDef.Extends`), just
    /// surfaced by name instead of collapsed into a load-time/lazy flag —
    /// `analysis::inheritance`'s own source for deciding whether a
    /// `ParentTypeMismatch` child type is a genuine subclass of its
    /// resolved parent's element type.
    pub type_hierarchy: Vec<(String, Option<String>)>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PeMetadataError {
    #[error("unexpected end of file while reading PE/metadata structures")]
    Truncated,
    #[error("not a valid PE image (bad DOS/PE signature)")]
    InvalidPeSignature,
    #[error("unsupported PE optional header magic {0:#06x}")]
    UnsupportedImage(u16),
    #[error("file has no CLI header (not a managed assembly)")]
    NotAManagedAssembly,
    #[error("RVA {0:#x} is not contained in any section")]
    RvaOutOfRange(u32),
    #[error("invalid metadata root signature")]
    InvalidMetadataSignature,
    #[error("missing required metadata stream {0}")]
    MissingStream(&'static str),
    #[error("unsupported metadata table 0x{0:02x}")]
    UnsupportedTable(u8),
    #[error("assembly metadata has no Assembly table row")]
    MissingAssemblyRow,
    #[error("invalid compressed integer encoding")]
    InvalidCompressedInteger,
}

/// Reads the assembly identity and references from a `.dll`'s raw bytes.
pub fn read(bytes: &[u8]) -> Result<AssemblyMetadata, PeMetadataError> {
    if read_bytes(bytes, 0, 2)? != b"MZ".as_slice() {
        return Err(PeMetadataError::InvalidPeSignature);
    }
    let e_lfanew = read_u32(bytes, 0x3C)? as usize;
    if read_bytes(bytes, e_lfanew, 4)? != b"PE\0\0".as_slice() {
        return Err(PeMetadataError::InvalidPeSignature);
    }

    let coff_offset = e_lfanew + 4;
    let number_of_sections = read_u16(bytes, coff_offset + 2)? as usize;
    let size_of_optional_header = read_u16(bytes, coff_offset + 16)? as usize;
    let optional_header_offset = coff_offset + 20;

    let magic = read_u16(bytes, optional_header_offset)?;
    let data_dir_offset = match magic {
        0x10b => optional_header_offset + 96,  // PE32
        0x20b => optional_header_offset + 112, // PE32+
        other => return Err(PeMetadataError::UnsupportedImage(other)),
    };

    // Data directory 14 (0-indexed) is the CLR/CLI header.
    let cli_dir_offset = data_dir_offset + 14 * 8;
    let cli_rva = read_u32(bytes, cli_dir_offset)?;
    if cli_rva == 0 {
        return Err(PeMetadataError::NotAManagedAssembly);
    }

    let section_table_offset = optional_header_offset + size_of_optional_header;
    let sections = read_sections(bytes, section_table_offset, number_of_sections)?;

    let cli_header_offset = rva_to_offset(&sections, cli_rva)?;
    // IMAGE_COR20_HEADER: cb(4) + MajorRuntimeVersion(2) + MinorRuntimeVersion(2), then MetaData RVA/Size.
    let metadata_rva = read_u32(bytes, cli_header_offset + 8)?;
    let metadata_offset = rva_to_offset(&sections, metadata_rva)?;

    read_metadata_root(bytes, metadata_offset)
}

// ---------------------------------------------------------------------
// Bounds-checked primitive reads
// ---------------------------------------------------------------------
