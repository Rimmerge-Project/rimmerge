//! What a family's first entry says about the mod behind it: the raw text
//! `rim-session` later resolves against the active mods (per family, once).
//!
//! Which evidence a class carries is decided here, one arm per class: a
//! failure or block names its tag and file, a texture fallback its path, a
//! `[Tag]` line or an About.xml warning its display name, an exception its
//! stack's innermost non-engine frame type (the type its family key already
//! names), and a type-load error the type or assembly its head names. A
//! cross-reference message and the engine's own classes carry none (the log
//! never says which mod's data made a cross-reference fail).

use std::sync::LazyLock;

use regex::Regex;
use rim_session::ports::{
    AttributionInput, EngineInfoKind, EntryClass, SaveLoadPhase, leading_bracket_tag,
};

use super::classify::strip_color;
use super::entry::outer_type;
use super::patterns::{
    MOD_METADATA_NAME_PATTERN, TYPE_LOAD_ASSEMBLY_PATTERN, TYPE_LOAD_TYPE_PATTERN, fixed_regex,
};
use super::records::RecordCollector;
use super::shapes::CompiledShapes;

static MOD_METADATA_NAME_RE: LazyLock<Regex> =
    LazyLock::new(|| fixed_regex(MOD_METADATA_NAME_PATTERN));
static TYPE_LOAD_TYPE_RE: LazyLock<Regex> = LazyLock::new(|| fixed_regex(TYPE_LOAD_TYPE_PATTERN));
static TYPE_LOAD_ASSEMBLY_RE: LazyLock<Regex> =
    LazyLock::new(|| fixed_regex(TYPE_LOAD_ASSEMBLY_PATTERN));

/// Everything an entry offers as evidence when its family is new.
pub(super) struct EvidenceSource<'a> {
    pub(super) class: EntryClass,
    /// The entry's head line, as logged.
    pub(super) head: &'a str,
    /// The failure or block record the entry is still building.
    pub(super) records: &'a RecordCollector<'a>,
    pub(super) shapes: &'a CompiledShapes,
    /// The innermost non-engine frame type of the entry's stack (the one its
    /// family key names), back-references resolved.
    pub(super) key_frame: Option<&'a str>,
}

/// The attribution input of an entry of `source.class`, or `None` when the
/// class carries none or the entry holds no evidence.
pub(super) fn attribution_input(source: &EvidenceSource<'_>) -> Option<AttributionInput> {
    let head = source.head.trim();
    match source.class {
        EntryClass::PatchFailure | EntryClass::PatchStackTrace => {
            source.records.open_attribution_input()
        }
        EntryClass::TextureFallback => source
            .shapes
            .texture_fallback_path(head)
            .map(|path| AttributionInput::path(&path)),
        EntryClass::ModMessage | EntryClass::Timer => {
            leading_bracket_tag(&strip_color(head)).map(AttributionInput::display_name)
        }
        EntryClass::ModMetadataWarning => {
            metadata_mod_name(head).map(|name| AttributionInput::display_name(&name))
        }
        EntryClass::RuntimeException => source.key_frame.and_then(AttributionInput::type_name),
        EntryClass::TypeLoadError => type_load_subject(head),
        EntryClass::LoadEvent
        | EntryClass::EngineInfo(
            EngineInfoKind::Banner
            | EngineInfoKind::LoggingStopped
            | EngineInfoKind::LoggingResumed
            | EngineInfoKind::SessionMarker
            | EngineInfoKind::UnityRuntime,
        )
        | EntryClass::ConfigError
        | EntryClass::PatchError
        | EntryClass::CrossReference
        | EntryClass::MissingParent
        | EntryClass::XmlError
        | EntryClass::DuplicateDef
        | EntryClass::TextureLoadFailure
        | EntryClass::SaveLoadReference(SaveLoadPhase::Save | SaveLoadPhase::Load)
        | EntryClass::BaseGenRuleMissing
        | EntryClass::UnityRuntimeError
        | EntryClass::DefCacheLine
        | EntryClass::Unclassified => None,
    }
}

/// What a type-load error's head names: the type of an `Error in static
/// constructor of <Type>` / `Error while instantiating a mod of type <Type>`,
/// else the assembly of an `Exception loading <x>.dll` / `... getting types in
/// assembly <x>`. `None` for a head that names neither (`Error initializing
/// mod:`). The head is the family key's own text, so attribution stays a
/// function of the key.
fn type_load_subject(head: &str) -> Option<AttributionInput> {
    let head = strip_color(head);
    if let Some(captures) = TYPE_LOAD_TYPE_RE.captures(&head) {
        return AttributionInput::type_name(outer_type(&captures["type"]));
    }
    let captures = TYPE_LOAD_ASSEMBLY_RE.captures(&head)?;
    let assembly = captures.name("dll").or_else(|| captures.name("assembly"))?;
    AttributionInput::assembly(assembly.as_str())
}

/// The mod an About.xml warning names.
fn metadata_mod_name(head: &str) -> Option<String> {
    let captures = MOD_METADATA_NAME_RE.captures(head)?;
    captures
        .name("name")
        .or_else(|| captures.name("colon_name"))
        .map(|found| found.as_str().to_string())
}

#[cfg(test)]
mod tests {
    use rim_session::ports::AttributionInput;

    use super::{metadata_mod_name, type_load_subject};

    #[test]
    fn a_metadata_warning_names_its_mod_in_every_wording() {
        let cases = [
            (
                "Mod Example Mod dependency (a.b) needs to have <downloadUrl>",
                "Example Mod",
            ),
            ("Mod Example Mod has a dependency with no id", "Example Mod"),
            (
                "Mod Example: Mod <packageId> (x) is not in valid format.",
                "Example: Mod",
            ),
            (
                "Mod Example Mod is missing packageId in About.xml",
                "Example Mod",
            ),
            (
                "Mod Example Mod: targetVersion field is obsolete",
                "Example Mod",
            ),
        ];
        for (head, expected) in cases {
            assert_eq!(metadata_mod_name(head).as_deref(), Some(expected), "{head}");
        }
        assert_eq!(
            metadata_mod_name("Unable to parse version string on mod X"),
            None
        );
    }

    #[test]
    fn a_type_load_head_names_its_type_or_its_assembly() {
        let cases = [
            (
                "Error in static constructor of Example.Main: System.TypeInitializationException: x",
                AttributionInput::type_name("Example.Main"),
            ),
            (
                "Error while instantiating a mod of type Example.Mod+Nested: boom",
                AttributionInput::type_name("Example.Mod"),
            ),
            (
                "Exception loading Example.Lib.dll: boom",
                AttributionInput::assembly("Example.Lib"),
            ),
            (
                "ReflectionTypeLoadException getting types in assembly Example.Lib, Version=1.0",
                AttributionInput::assembly("Example.Lib"),
            ),
            (
                "Exception getting types in assembly Example.Lib: boom",
                AttributionInput::assembly("Example.Lib"),
            ),
            ("Error initializing mod: boom", None),
        ];
        for (head, expected) in cases {
            assert_eq!(type_load_subject(head), expected, "{head}");
        }
    }
}
