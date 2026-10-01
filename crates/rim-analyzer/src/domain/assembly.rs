//! Metadata read from a mod's `Assemblies/**/*.dll` files.

use serde::{Deserialize, Serialize};

/// One assembly named in another assembly's `AssemblyRef` table, plus
/// whether it's referenced in a way RimWorld resolves at DLL-load time.
///
/// After loading a mod's DLL, RimWorld calls `Assembly.GetTypes()`. If any
/// type in the DLL has a base type or implemented interface living in
/// another, not-yet-loaded assembly, that call throws and the whole DLL is
/// dropped — a hard, load-order-sensitive dependency. A reference that
/// only appears in a method body, field type, or attribute resolves
/// lazily (on first use) and works in any load order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AssemblyReference {
    /// Lowercased referenced assembly name.
    pub name: String,
    /// `true` when at least one `TypeDef.Extends` or `InterfaceImpl` in
    /// the referencing assembly resolves (directly, or through a chain of
    /// nested-type `TypeRef`s) to a type in this assembly.
    pub load_time: bool,
}

/// An assembly's own version, read from its `Assembly` metadata table row
/// (major.minor.build.revision — .NET's four-part scheme, not semver). Used
/// to decide, for two mods shipping the same assembly name, which copy is
/// newer (see [`crate::domain::DuplicateAssembly`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AssemblyVersion {
    pub major: u16,
    pub minor: u16,
    pub build: u16,
    pub revision: u16,
}

/// Which runtime patch-method role a [`RuntimePatchTarget`] was declared
/// with. The library discovers this two
/// ways, both modelled by `extract::pe_metadata`: an explicit
/// prefix/postfix/transpiler kind-marker attribute on
/// the implementing method, or — with no attribute at all — a method
/// simply *named* `Prefix`/`Postfix`/`Transpiler` inside a
/// patch-target-annotated class.
///
/// The distinction matters because `Prefix`/`Postfix` methods each run
/// independently around the original method and compose safely no matter how
/// many mods add one, whereas a `Transpiler` rewrites the IL the *next*
/// transpiler must still pattern-match — so two or more transpilers on the
/// same target are order-sensitive in a way prefixes/postfixes never are (the
/// fact behind [`crate::domain::TranspilerCollision`]).
///
/// [`Self::Unknown`] is the required, conservative default for a target
/// this reader cannot confidently classify by either mechanism — a kind
/// is never guessed, since only a positively identified `Transpiler` may
/// ever justify constraining load order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimePatchKind {
    Prefix,
    Postfix,
    Transpiler,
    Unknown,
}

/// One patch-target-attribute-declared patch target, decoded from that
/// custom-attribute constructor call: a `typeof(T)` argument (or a plain
/// type-name string) plus a method-name argument, combined across a
/// class-level and its methods' attributes the way the library itself merges them
/// (see `extract::pe_metadata`'s decoder). The conflicts this data feeds —
/// two mods patching the same target — are built in `analysis::conflicts`;
/// this only records the facts.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct RuntimePatchTarget {
    /// The target type's bare name, e.g. `Verse.Pawn`. A `System.Type`
    /// constructor argument (`typeof(T)`) serializes in the attribute blob
    /// as the *assembly-qualified* name (`Verse.Pawn, Assembly-CSharp,
    /// Version=..., Culture=neutral, PublicKeyToken=null`, per ECMA-335
    /// II.23.3); `extract::pe_metadata` strips everything from the first
    /// comma onward at decode time, so this field is always the bare name,
    /// never assembly-qualified, regardless of which constructor overload
    /// produced it.
    pub type_name: String,
    /// The target method's name.
    pub method_name: String,
    /// Which patch-method role this target was declared with. A class
    /// implementing more than one role for the same `(type_name,
    /// method_name)` (e.g. both a `Prefix` and a `Postfix`) produces one
    /// [`RuntimePatchTarget`] per role found — see
    /// `extract::pe_metadata::combine_runtime_patches`'s own doc comment.
    pub kind: RuntimePatchKind,
}

/// The assembly identity and outbound references read from one DLL's
/// ECMA-335 metadata (Assembly, AssemblyRef, TypeRef, TypeDef, and
/// InterfaceImpl tables).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AssemblyInfo {
    /// The file stem, kept for diagnostics (e.g. "which .dll produced this").
    pub file_name: String,
    /// The assembly's own defined name (Assembly table), lowercased.
    pub name: String,
    /// Names referenced via AssemblyRef, lowercased, deduplicated, each
    /// tagged with whether it's a load-time or lazy reference.
    pub references: Vec<AssemblyReference>,
    /// This assembly's own version (Assembly table row). `None` when
    /// metadata parsing failed.
    pub version: Option<AssemblyVersion>,
    /// Every runtime-patch target this assembly's custom attributes
    /// declare. Empty when metadata parsing failed.
    pub runtime_patches: Vec<RuntimePatchTarget>,
    /// Every type this assembly declares, keyed by its own full
    /// (`Namespace.Name`) name, alongside its base type's own full name
    /// where resolvable — see `extract::pe_metadata::AssemblyMetadata
    /// ::type_hierarchy`'s own doc comment. Empty when metadata parsing
    /// failed.
    pub type_hierarchy: Vec<(String, Option<String>)>,
    /// True when metadata parsing failed and `name`/`references` are a
    /// best-effort fallback (file stem, no references).
    pub parse_failed: bool,
}
