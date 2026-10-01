//! Resolving the raw evidence of a `Player.log` against the session's active
//! mods: a file path against the mods' folders, a display name against the
//! mods' names, a type or assembly name against the assemblies the mods ship.
//!
//! Family attribution runs once per family, never per entry, so the mod
//! folders are normalized once per import ([`PathIndex`]) instead of on every
//! lookup.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};

use rim_analyzer::analysis::SourceIndex;
use rim_analyzer::analysis::edges::AssemblyOwnership;
use rim_analyzer::analysis::indices::DisplayNameIndex;
use rim_analyzer::domain::{Mod, ModId};

use super::{AttributedClass, FamilyAttribution, LogAttribution};
use crate::ports::{AttributionInput, ClassTally, EntryClass};

/// Lowercased, forward-slashed. Windows paths compare case-insensitively
/// and RimWorld's own log always uses backslashes; a mod's own
/// `Mod::path`/`loaded_folders` are read straight off the same
/// filesystem this log was produced on, so the only real difference is
/// separator style and case.
fn normalize_path_text(text: &str) -> String {
    text.replace('\\', "/").to_lowercase()
}

/// `child` is under `parent` when it starts with `parent` and the next
/// character (if any) is a path separator — not merely a string prefix,
/// so `mods/foo` never matches `mods/foobar`.
pub(super) fn is_under(child: &str, parent: &str) -> bool {
    !parent.is_empty()
        && child.starts_with(parent)
        && child
            .as_bytes()
            .get(parent.len())
            .is_none_or(|&byte| byte == b'/')
}

/// A folder path without its trailing separators (`c:/mods/a/` is `c:/mods/a`),
/// so a folder listed with one still owns the files under it.
fn trim_trailing_separators(folder: &str) -> &str {
    folder.trim_end_matches('/')
}

/// Every active mod's own `Mod::path` and `Mod::loaded_folders`, normalized
/// once, longest first, so the first folder a path is under is the longest
/// match.
pub(super) struct PathIndex {
    folders: Vec<(String, ModId)>,
}

impl PathIndex {
    /// Indexes every folder of `mods`. The sort is stable, so of two equally
    /// long folders the one listed first (mod order, then the mod's own path
    /// before its loaded folders) wins.
    pub(super) fn new(mods: &[Mod]) -> Self {
        let mut folders: Vec<(String, ModId)> = mods
            .iter()
            .flat_map(|m| {
                std::iter::once(&m.path)
                    .chain(m.loaded_folders.iter())
                    .map(|folder| {
                        let normalized = normalize_path_text(&folder.to_string_lossy());
                        let trimmed = trim_trailing_separators(&normalized).to_string();
                        (trimmed, m.id.clone())
                    })
            })
            .filter(|(folder, _)| !folder.is_empty())
            .collect();
        folders.sort_by_key(|(folder, _)| Reverse(folder.len()));
        Self { folders }
    }

    /// The mod whose folder is the *longest* prefix of `raw_path`, so a
    /// mod's more specific `loaded_folders` entry (its `1.6/` subfolder) is
    /// preferred over its own root `path`, which is also always a prefix of
    /// it.
    pub(super) fn owner_of(&self, raw_path: &str) -> Option<&ModId> {
        let child = normalize_path_text(raw_path);
        self.folders
            .iter()
            .find(|(folder, _)| is_under(&child, folder))
            .map(|(_, id)| id)
    }
}

/// A display name reduced to its lowercase ASCII letters and digits, so
/// `Example Mod`, `Example-Mod` and `[ExampleMod]` share one key, and a name
/// with a translated prefix still matches the Latin tag the mod prints.
/// Empty when the name has no such character, which never matches anything.
fn compact_name(name: &str) -> String {
    name.chars()
        .flat_map(char::to_lowercase)
        .filter(char::is_ascii_alphanumeric)
        .collect()
}

/// What a name (a `[Tag]`, a metadata warning's mod name) resolved to.
pub(super) enum NameMatch<'a> {
    /// Exactly one active mod.
    Sole(&'a ModId),
    /// Several active mods claim the same packageId or compact name, so
    /// none may win.
    Ambiguous(&'a BTreeSet<ModId>),
    /// No active mod.
    Unmatched,
}

/// Resolves a name against the active mods: the exact (lowercased) display
/// name first, then a packageId, then the display name without spaces and
/// punctuation. The exact map keeps its first-claim-wins rule; the two
/// fallbacks must be unique, since a guess must never win silently.
pub(super) struct NameResolver {
    exact: DisplayNameIndex,
    by_package_id: BTreeMap<String, BTreeSet<ModId>>,
    by_compact_name: BTreeMap<String, BTreeSet<ModId>>,
}

impl NameResolver {
    /// Indexes `mods` once; `exact` is the shared first-claim display-name map.
    pub(super) fn new(mods: &[Mod], exact: DisplayNameIndex) -> Self {
        let mut by_package_id: BTreeMap<String, BTreeSet<ModId>> = BTreeMap::new();
        let mut by_compact_name: BTreeMap<String, BTreeSet<ModId>> = BTreeMap::new();
        for m in mods {
            let package_id = m.id.base().as_str().to_lowercase();
            by_package_id
                .entry(package_id)
                .or_default()
                .insert(m.id.clone());
            let compact = compact_name(&m.name);
            if !compact.is_empty() {
                by_compact_name
                    .entry(compact)
                    .or_default()
                    .insert(m.id.clone());
            }
        }
        Self {
            exact,
            by_package_id,
            by_compact_name,
        }
    }

    /// Resolves `name` through the three lookups, stopping at the first that
    /// finds any claimant.
    pub(super) fn resolve(&self, name: &str) -> NameMatch<'_> {
        if let Some(id) = self.exact.get(name) {
            return NameMatch::Sole(id);
        }
        let package_claimants = self.by_package_id.get(&name.to_lowercase());
        let compact = compact_name(name);
        let compact_claimants = self.by_compact_name.get(&compact);
        match package_claimants.or(compact_claimants) {
            Some(claimants) => Self::sole_or_ambiguous(claimants),
            None => NameMatch::Unmatched,
        }
    }

    fn sole_or_ambiguous(claimants: &BTreeSet<ModId>) -> NameMatch<'_> {
        let mut ids = claimants.iter();
        match (ids.next(), ids.next()) {
            (Some(id), None) => NameMatch::Sole(id),
            _ => NameMatch::Ambiguous(claimants),
        }
    }
}

/// What raw evidence is resolved against: the active mods' folders and
/// display names, and the assemblies they ship.
pub(super) struct Attributor<'a> {
    pub(super) paths: &'a PathIndex,
    pub(super) names: &'a NameResolver,
    pub(super) sources: &'a SourceIndex,
}

impl Attributor<'_> {
    /// The mod a file path lies in, or the path itself as the raw text.
    pub(super) fn by_path(&self, path: &str) -> LogAttribution {
        match self.paths.owner_of(path) {
            Some(id) => LogAttribution::Mod(id.clone()),
            None => LogAttribution::Unattributed(path.to_string()),
        }
    }

    /// The mod a display name belongs to, or the name itself as the raw text.
    /// A name several mods claim is unattributed here: the typed lists have
    /// no "ambiguous" shape.
    pub(super) fn by_name(&self, name: &str) -> LogAttribution {
        match self.names.resolve(name) {
            NameMatch::Sole(id) => LogAttribution::Mod(id.clone()),
            NameMatch::Ambiguous(_) | NameMatch::Unmatched => {
                LogAttribution::Unattributed(name.to_string())
            }
        }
    }

    /// Like [`Self::by_name`], but a name several mods claim keeps its
    /// candidates.
    fn family_by_name(&self, name: &str) -> FamilyAttribution {
        match self.names.resolve(name) {
            NameMatch::Ambiguous(candidates) => FamilyAttribution::Ambiguous {
                raw: name.to_string(),
                candidates: candidates.clone(),
            },
            NameMatch::Sole(_) | NameMatch::Unmatched => self.by_name(name).into(),
        }
    }

    /// The path first, then the name; when neither resolves, the name is the
    /// raw text.
    pub(super) fn by_path_then_name(&self, name: &str, path: Option<&str>) -> LogAttribution {
        match path.and_then(|path| self.paths.owner_of(path)) {
            Some(id) => LogAttribution::Mod(id.clone()),
            None => self.by_name(name),
        }
    }

    /// The mod(s) shipping the assembly at the longest namespace prefix of the
    /// one type the log blames, the type itself as the raw text otherwise.
    /// Only that type is asked: when it has no single owner the family stays
    /// unattributed or ambiguous, never re-blamed on a frame further out.
    fn by_type(&self, type_name: &str) -> FamilyAttribution {
        family_attribution(self.sources.namespace_ownership(type_name), type_name)
    }

    /// The mod(s) shipping the assembly a type-load error names.
    fn by_assembly(&self, assembly_name: &str) -> FamilyAttribution {
        family_attribution(
            self.sources.assembly_ownership(assembly_name),
            assembly_name,
        )
    }

    /// Resolves one family's evidence.
    pub(super) fn resolve(&self, input: &AttributionInput) -> FamilyAttribution {
        match input {
            AttributionInput::DisplayName(name) => self.family_by_name(name),
            AttributionInput::DisplayNameAndPath { name, path } => {
                match self.paths.owner_of(path) {
                    Some(id) => FamilyAttribution::Mod(id.clone()),
                    None => self.family_by_name(name),
                }
            }
            AttributionInput::Path(path) => self.by_path(path).into(),
            AttributionInput::TypeName(type_name) => self.by_type(type_name),
            AttributionInput::Assembly(assembly_name) => self.by_assembly(assembly_name),
        }
    }
}

fn family_attribution(ownership: AssemblyOwnership<'_>, raw: &str) -> FamilyAttribution {
    match ownership {
        AssemblyOwnership::Sole(id) => FamilyAttribution::Mod(id.clone()),
        AssemblyOwnership::Unowned => FamilyAttribution::Unattributed {
            raw: raw.to_string(),
        },
        AssemblyOwnership::Shared(owners) => FamilyAttribution::Ambiguous {
            raw: raw.to_string(),
            candidates: owners.iter().cloned().collect(),
        },
    }
}

/// Attributes every family of every class, once per family. A family whose
/// first entry carried no evidence has no row (a cross-reference, an engine
/// message).
pub(super) fn attribute_classes(
    classes: BTreeMap<EntryClass, ClassTally>,
    attributor: &Attributor<'_>,
) -> BTreeMap<EntryClass, AttributedClass> {
    classes
        .into_iter()
        .map(|(class, tally)| {
            let attribution = tally
                .families
                .iter()
                .filter_map(|(key, family)| {
                    let input = family.attribution_input.as_ref()?;
                    Some((key.clone(), attributor.resolve(input)))
                })
                .collect();
            (class, AttributedClass { tally, attribution })
        })
        .collect()
}
