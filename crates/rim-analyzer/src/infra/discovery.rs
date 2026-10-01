//! Enumerates every mod directory under the game's `Data/` and `Mods/`
//! folders and the Steam workshop content folder, reading each one's
//! `About.xml` to build the `packageId -> mod` map the rest of scanning
//! looks mods up in.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use rayon::prelude::*;

use crate::domain::{GameVersion, GeneratedMarker, InactiveMod, ModId, Source, Warning};
use crate::extract::about_xml::{self, AboutXmlData};
use crate::extract::expansion_defs::{self, ExpansionDefEntry};
use crate::extract::rimmerge_marker;

use super::mod_scan::read_bounded;
use super::paths;

/// One mod found on disk, with its `About.xml` already parsed.
pub struct DiscoveredMod {
    pub id: ModId,
    pub path: PathBuf,
    pub source: Source,
    pub about: AboutXmlData,
    /// This folder's `rimmerge.json` marker, when it has (and this
    /// version recognizes) one. See [`crate::domain::GeneratedMarker`].
    pub generated: Option<GeneratedMarker>,
    /// The Steam Workshop published-file id, when `source` is
    /// [`Source::Workshop`]: the workshop content tree names each mod
    /// folder by its published-file id (`<workshop_dir>/<id>/`), so this
    /// is `path`'s own folder name parsed as an integer. `None` for every
    /// other source, and for a Workshop folder whose name — contrary to
    /// every real Steam-managed install — isn't purely numeric.
    pub workshop_id: Option<u64>,
}

/// [`DiscoveredMod::workshop_id`]'s own logic: `path`'s final folder
/// component, parsed as `u64`, only when `source` is
/// [`Source::Workshop`].
fn workshop_id_of(path: &Path, source: Source) -> Option<u64> {
    if source != Source::Workshop {
        return None;
    }
    path.file_name()?.to_str()?.parse().ok()
}

/// Every mod directory found on disk, indexed for the two ways
/// `ModsConfig.xml` can name one: a bare packageId, or — when a local
/// (`Data`/`Mods`) copy and a Steam Workshop copy of the same packageId
/// both exist on disk — that packageId with `_steam` appended for the
/// Workshop copy specifically (see [`ModId::base`]).
pub struct Discovered {
    /// Non-Workshop copies (`Core`, `Dlc`, `Local`), keyed by their own
    /// packageId. The map a bare `ModsConfig.xml` id resolves against
    /// first.
    primary: HashMap<ModId, DiscoveredMod>,
    /// Workshop copies, keyed by their own packageId. Resolved by a
    /// `_steam`-suffixed id, and as the fallback for a bare id with no
    /// non-Workshop copy on disk (the overwhelmingly common case: a
    /// Workshop-only mod with no local duplicate never gets the suffix).
    workshop: HashMap<ModId, DiscoveredMod>,
}

impl Discovered {
    /// Resolves one `ModsConfig.xml` active-mod entry to the mod
    /// directory it names: `X_steam` resolves to the Workshop copy of
    /// `X`; a bare `X` resolves to a non-Workshop copy of `X` first, the
    /// Workshop copy otherwise.
    #[must_use]
    pub fn lookup(&self, id: &ModId) -> Option<&DiscoveredMod> {
        match id.as_str().strip_suffix("_steam") {
            Some(base) => self.workshop.get(&ModId::new(base)),
            None => self.primary.get(id).or_else(|| self.workshop.get(id)),
        }
    }

    /// The total number of mod directories found, active or not.
    #[must_use]
    pub fn len(&self) -> usize {
        self.primary.len() + self.workshop.len()
    }

    /// The `ModsConfig.xml` id that would activate `entry`'s own copy —
    /// [`Self::lookup`]'s resolution rule run in reverse: bare for a
    /// `primary` entry; bare for a `workshop` entry too, *unless* a `primary`
    /// copy of the same packageId also exists on disk, in which case the
    /// workshop copy needs the `_steam` suffix to be distinguishable from
    /// that copy at all (both copies of a shadowed pair are listed, under `X`
    /// and `X_steam`).
    fn active_mods_id(&self, id: &ModId, is_workshop: bool) -> ModId {
        if is_workshop && self.primary.contains_key(id) {
            ModId::new(format!("{id}_steam"))
        } else {
            id.clone()
        }
    }

    /// Every discovered mod not among `resolved_active`'s own entries — the
    /// `InactiveMod` list for `Report.inactive_mods`. `resolved_active` is
    /// exactly what `infra::resolve_active_mods` already produced
    /// (`to_scan`): the `(active id, DiscoveredMod)` pairs
    /// `Discovered::lookup` itself resolved each active-list entry to.
    ///
    /// **Matched by `path`, not by re-deriving each entry's own
    /// [`Self::active_mods_id`] and comparing that against `active`** —
    /// deliberately: [`Self::lookup`]'s own resolution is *not* a mirror of
    /// `active_mods_id`. `lookup` resolves a `_steam`-suffixed active entry
    /// to the Workshop map unconditionally — including a Workshop-*only* mod
    /// with no `primary` copy at all (its own doc comment: "and as the
    /// fallback for a bare id with no non-Workshop copy on disk") — while
    /// `active_mods_id` only emits the `_steam` suffix when a `primary` copy
    /// *also* exists. So an id-based check would re-derive a Workshop-only
    /// mod active as `x.mod_steam` as bare `x.mod`, find that exact string
    /// absent from the active set, and list the very folder just scanned as
    /// active a second time, as inactive — the same physical directory in
    /// both `mods` and `inactive_mods` at once, breaking the disjointness
    /// invariant `real_install_inactive.rs` exists to check. Comparing by
    /// `path` is immune to the whole class: a folder that was resolved into
    /// `resolved_active` is never also inactive, full stop, regardless of
    /// which id string named it. `active_mods_id` is still used for the
    /// *emitted* id of an inactive entry — what id would activate *this* copy
    /// is exactly what it answers.
    ///
    /// Sorted by id: `primary` and `workshop` are both `HashMap`s, so
    /// this is the one place that ordering is imposed, the same
    /// determinism contract every other `Report`-reaching collection in
    /// this crate follows.
    #[must_use]
    pub fn inactive_excluding(
        &self,
        resolved_active: &[(&ModId, &DiscoveredMod)],
    ) -> Vec<InactiveMod> {
        let active_paths: HashSet<&Path> = resolved_active
            .iter()
            .map(|(_, m)| m.path.as_path())
            .collect();
        let mut out: Vec<InactiveMod> = self
            .all()
            .into_iter()
            .filter(|m| !active_paths.contains(m.path.as_path()))
            .collect();
        out.sort_by(|a, b| a.id.cmp(&b.id));
        out
    }

    /// Every discovered mod, active or not, shaped as an [`InactiveMod`]
    /// — the discovery-only inventory `infra::inventory` builds
    /// identity and declared
    /// order only, no defs/patches/assemblies read for any of them. Not
    /// sorted (unlike [`Self::inactive_excluding`]) — callers that need a
    /// deterministic order (that method, and `infra::inventory` itself)
    /// impose it themselves.
    #[must_use]
    pub fn all(&self) -> Vec<InactiveMod> {
        let mut out = Vec::with_capacity(self.len());
        for (id, discovered) in &self.primary {
            out.push(to_inactive_mod(id.clone(), discovered));
        }
        for (id, discovered) in &self.workshop {
            let effective_id = self.active_mods_id(id, true);
            out.push(to_inactive_mod(effective_id, discovered));
        }
        out
    }
}

/// [`DiscoveredMod`] -> [`InactiveMod`]: the identity and declared-order
/// facts discovery already has for free, under `id` (which, for a Workshop
/// copy, may differ from `discovered.id` — see `Discovered::active_mods_id`).
/// Despite the name (kept for the type it builds, [`InactiveMod`] — the shape
/// every discovered mod uses whether active or not, per that type's own doc
/// comment), `pub(super)` so `infra::inventory` can call it directly for an
/// *active* entry too, keyed by the caller's own already-resolved active id
/// rather than [`Discovered::active_mods_id`]'s re-derivation — see that
/// function's own doc comment for why re-deriving an active id from the
/// folder alone is unsound for a Workshop-only mod.
pub(super) fn to_inactive_mod(id: ModId, discovered: &DiscoveredMod) -> InactiveMod {
    InactiveMod {
        id,
        name: discovered.about.name.clone(),
        authors: discovered.about.authors.clone(),
        path: discovered.path.clone(),
        source: discovered.source,
        supported_versions: discovered.about.supported_versions.clone(),
        workshop_id: discovered.workshop_id,
        generated: discovered.generated.clone(),
        declared: discovered.about.declared.clone(),
    }
}

/// Scans `Data/*`, `Mods/*`, and `<workshop_dir>/*` for mod directories,
/// parsing each one's `About.xml`. Directories whose `About.xml` is
/// missing or fails to parse are skipped with a [`Warning`] rather than
/// aborting the run. `Data`/`Mods` and Workshop copies of the same
/// packageId are kept as two distinct entries (see [`Discovered`]); a true
/// collision *within* one of those two groups (e.g. two different `Mods/`
/// directories declaring the same packageId) still keeps the first one
/// found (Data, then Mods; Workshop directories in their own listing
/// order) and records the other as a warning.
///
/// `Data/Core`/DLC entries have no `<name>` in their own `About.xml` —
/// their display name is resolved from `Defs/Misc/ExpansionDefs/ExpansionDefs.xml`
/// (read once from `Data/Core`), falling back to a fixed table of known
/// packageIds when that file can't be read or doesn't cover a given one.
#[must_use]
pub fn discover(
    game_dir: &Path,
    workshop_dir: &Path,
    game_version: GameVersion,
) -> (Discovered, Vec<Warning>) {
    let mut warnings = Vec::new();
    let expansion_names = load_expansion_names(game_dir, &mut warnings);

    let candidates = candidate_dirs(game_dir, workshop_dir);

    let results: Vec<Result<(DiscoveredMod, Option<Warning>), Warning>> = candidates
        .par_iter()
        .map(|(path, source)| load_about(path, *source, game_version, &expansion_names))
        .collect();

    let mut primary: HashMap<ModId, DiscoveredMod> = HashMap::new();
    let mut workshop: HashMap<ModId, DiscoveredMod> = HashMap::new();
    for result in results {
        match result {
            Ok((discovered, marker_warning)) => {
                warnings.extend(marker_warning);
                let bucket = if discovered.source == Source::Workshop {
                    &mut workshop
                } else {
                    &mut primary
                };
                if let Some(existing) = bucket.get(&discovered.id) {
                    warnings.push(Warning::new(
                        Some(discovered.id.clone()),
                        format!(
                            "duplicate packageId on disk: {} (kept {}, ignored {})",
                            discovered.id,
                            existing.path.display(),
                            discovered.path.display()
                        ),
                    ));
                } else {
                    bucket.insert(discovered.id.clone(), discovered);
                }
            }
            Err(warning) => warnings.push(warning),
        }
    }
    (Discovered { primary, workshop }, warnings)
}

/// Reads and parses `Data/Core/Defs/Misc/ExpansionDefs/ExpansionDefs.xml`.
/// A missing or malformed file is non-fatal — Core/DLC display names fall
/// back to the fixed table in that case — but is recorded as a [`Warning`].
fn load_expansion_names(
    game_dir: &Path,
    warnings: &mut Vec<Warning>,
) -> HashMap<ModId, ExpansionDefEntry> {
    let path = paths::expansion_defs_path(game_dir);
    let bytes = match read_bounded(&path) {
        Ok(bytes) => bytes,
        Err(e) => {
            warnings.push(Warning::new(
                None,
                format!("{}: cannot read ExpansionDefs.xml: {e}", path.display()),
            ));
            return HashMap::new();
        }
    };
    match expansion_defs::parse(&bytes) {
        Ok(map) => map,
        Err(e) => {
            warnings.push(Warning::new(
                None,
                format!("{}: invalid ExpansionDefs.xml: {e}", path.display()),
            ));
            HashMap::new()
        }
    }
}

fn candidate_dirs(game_dir: &Path, workshop_dir: &Path) -> Vec<(PathBuf, Source)> {
    let mut out = Vec::new();
    for entry in subdirectories(&game_dir.join("Data")) {
        let source = if entry.file_name().is_some_and(|n| n == "Core") {
            Source::Core
        } else {
            Source::Dlc
        };
        out.push((entry, source));
    }
    for entry in subdirectories(&game_dir.join("Mods")) {
        out.push((entry, Source::Local));
    }
    for entry in subdirectories(workshop_dir) {
        out.push((entry, Source::Workshop));
    }
    out
}

fn subdirectories(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    dirs
}

fn load_about(
    path: &Path,
    source: Source,
    game_version: GameVersion,
    expansion_names: &HashMap<ModId, ExpansionDefEntry>,
) -> Result<(DiscoveredMod, Option<Warning>), Warning> {
    let about_path = path.join("About").join("About.xml");
    let bytes = read_bounded(&about_path).map_err(|e| {
        Warning::new(
            None,
            format!("{}: cannot read About.xml: {e}", path.display()),
        )
    })?;
    let mut about = about_xml::parse(&bytes, game_version)
        .map_err(|e| Warning::new(None, format!("{}: invalid About.xml: {e}", path.display())))?;

    if source.is_vanilla() {
        about.name = expansion_names
            .get(&about.id)
            .map(|entry| entry.label.clone())
            .or_else(|| about_xml::known_dlc_display_name(&about.id).map(str::to_string))
            .unwrap_or(about.name);
    }

    let (generated, marker_warning) = read_generated_marker(&about.id, path);
    let workshop_id = workshop_id_of(path, source);

    Ok((
        DiscoveredMod {
            id: about.id.clone(),
            path: path.to_path_buf(),
            source,
            about,
            generated,
            workshop_id,
        },
        marker_warning,
    ))
}

/// Reads and parses `<mod>/rimmerge.json`, the marker rimmerge writes into
/// every folder it generates. A missing, oversized, or otherwise
/// unreadable file is silently `None` — the file is optional, and the read
/// goes through the same [`read_bounded`] cap as every other mod file. A
/// file that *is* read but fails to parse (malformed JSON, or a `kind`
/// this version doesn't recognize) is also `None`, plus a [`Warning`]
/// naming the path: a user can put anything in a folder, so this is never
/// fatal.
fn read_generated_marker(
    mod_id: &ModId,
    path: &Path,
) -> (Option<GeneratedMarker>, Option<Warning>) {
    let marker_path = path.join("rimmerge.json");
    let Ok(bytes) = read_bounded(&marker_path) else {
        return (None, None);
    };
    match rimmerge_marker::parse(&bytes) {
        Some(marker) => (Some(marker), None),
        None => {
            let warning = Warning::new(
                Some(mod_id.clone()),
                format!(
                    "{}: unrecognized rimmerge.json marker",
                    marker_path.display()
                ),
            );
            (None, Some(warning))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::fs;

    use crate::domain::GeneratedKind;

    use super::*;

    fn tempdir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rim-analyzer-discovery-test-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_about(mod_dir: &Path, package_id: &str) {
        fs::create_dir_all(mod_dir.join("About")).unwrap();
        fs::write(mod_dir.join("About").join("About.xml"),
            format!("<ModMetaData><packageId>{package_id}</packageId><name>{package_id}</name></ModMetaData>"
            ))
        .unwrap();
    }

    #[test]
    fn discover_reads_a_valid_patch_marker_into_the_mod() {
        let root = tempdir("valid-patch-marker");
        let mod_dir = root.join("Mods").join("Patched");
        write_about(&mod_dir, "sample.abcompat");
        fs::write(mod_dir.join("rimmerge.json"),
            r#"{"kind":"patch","patchId":"3f9a1c02be77","profileHash":"abc","scope":["fixture.moda","fixture.modb"],"decisionsSha256":"def","rimmergeVersion":"0.1.0"}"#)
        .unwrap();

        let (discovered, warnings) = discover(
            &root,
            &root.join("workshop_does_not_exist"),
            GameVersion::new(1, 6),
        );

        let found = discovered
            .lookup(&ModId::new("sample.abcompat"))
            .expect("mod must be discovered");
        let marker = found.generated.as_ref().expect("marker must be parsed");
        assert_eq!(marker.kind, GeneratedKind::Patch);
        assert_eq!(marker.patch_id.as_deref(), Some("3f9a1c02be77"));
        assert_eq!(
            marker.scope,
            Some(BTreeSet::from([
                ModId::new("fixture.moda"),
                ModId::new("fixture.modb"),
            ]))
        );
        // `game_dir` has no `Data/Core`, so `discover` always warns about a
        // missing `ExpansionDefs.xml` — unrelated to the marker under test.
        assert!(!warnings.iter().any(|w| w.message.contains("rimmerge.json")));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn discover_reads_a_valid_assignment_marker_into_the_mod() {
        let root = tempdir("valid-assignment-marker");
        let mod_dir = root.join("Mods").join("Assigned");
        write_about(&mod_dir, "sample.partassign");
        fs::write(mod_dir.join("rimmerge.json"),
            r#"{"kind":"assignment","assignmentId":"example-race-groups","profileHash":"abc","scope":["fixture.example","fixture.target"],"decisionsSha256":"def","rimmergeVersion":"0.1.0"}"#)
        .unwrap();

        let (discovered, warnings) = discover(
            &root,
            &root.join("workshop_does_not_exist"),
            GameVersion::new(1, 6),
        );

        let found = discovered
            .lookup(&ModId::new("sample.partassign"))
            .expect("mod must be discovered");
        let marker = found.generated.as_ref().expect("marker must be parsed");
        assert_eq!(marker.kind, GeneratedKind::Assignment);
        assert_eq!(marker.patch_id.as_deref(), Some("example-race-groups"));
        assert_eq!(
            marker.scope,
            Some(BTreeSet::from([
                ModId::new("fixture.example"),
                ModId::new("fixture.target"),
            ]))
        );
        assert!(!warnings.iter().any(|w| w.message.contains("rimmerge.json")));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn discover_treats_a_malformed_marker_as_none_plus_a_warning() {
        let root = tempdir("malformed-marker");
        let mod_dir = root.join("Mods").join("Broken");
        write_about(&mod_dir, "someone.broken");
        fs::write(mod_dir.join("rimmerge.json"), b"{ not json").unwrap();

        let (discovered, warnings) = discover(
            &root,
            &root.join("workshop_does_not_exist"),
            GameVersion::new(1, 6),
        );

        let found = discovered
            .lookup(&ModId::new("someone.broken"))
            .expect("mod must be discovered");
        assert!(found.generated.is_none());
        assert!(warnings.iter().any(|w| w.message.contains("rimmerge.json")));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn discover_treats_a_well_formed_but_unknown_kind_as_none_plus_a_warning() {
        let root = tempdir("unknown-kind-marker");
        let mod_dir = root.join("Mods").join("FutureKind");
        write_about(&mod_dir, "someone.futurekind");
        fs::write(
            mod_dir.join("rimmerge.json"),
            r#"{"kind":"something_future","rimmergeVersion":"9.9.9"}"#,
        )
        .unwrap();

        let (discovered, warnings) = discover(
            &root,
            &root.join("workshop_does_not_exist"),
            GameVersion::new(1, 6),
        );

        let found = discovered
            .lookup(&ModId::new("someone.futurekind"))
            .expect("mod must be discovered");
        // A well-formed but unrecognized `kind` is `None`, not an error —
        // never a panic, never aborting the scan — but it still deserves
        // the same warning a malformed file gets: the folder claims to be
        // a marker this version can't understand.
        assert!(found.generated.is_none());
        assert!(warnings.iter().any(|w| w.message.contains("rimmerge.json")));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn discover_parses_workshop_id_from_the_numeric_content_folder_name() {
        let root = tempdir("workshop-id");
        let workshop_dir = root.join("workshop");
        let mod_dir = workshop_dir.join("3000000012");
        write_about(&mod_dir, "someone.workshopmod");

        let (discovered, _) = discover(&root, &workshop_dir, GameVersion::new(1, 6));

        let found = discovered
            .lookup(&ModId::new("someone.workshopmod"))
            .expect("mod must be discovered");
        assert_eq!(found.source, Source::Workshop);
        assert_eq!(found.workshop_id, Some(3_000_000_012));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn discover_never_assigns_a_workshop_id_to_a_local_mod() {
        let root = tempdir("no-workshop-id-for-local");
        let mod_dir = root.join("Mods").join("3000000012");
        write_about(&mod_dir, "someone.localmod");

        let (discovered, _) = discover(
            &root,
            &root.join("workshop_does_not_exist"),
            GameVersion::new(1, 6),
        );

        let found = discovered
            .lookup(&ModId::new("someone.localmod"))
            .expect("mod must be discovered");
        assert_eq!(found.workshop_id, None);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn discover_treats_a_missing_marker_as_none_with_no_warning() {
        let root = tempdir("no-marker");
        let mod_dir = root.join("Mods").join("Plain");
        write_about(&mod_dir, "someone.plain");

        let (discovered, warnings) = discover(
            &root,
            &root.join("workshop_does_not_exist"),
            GameVersion::new(1, 6),
        );

        let found = discovered
            .lookup(&ModId::new("someone.plain"))
            .expect("mod must be discovered");
        assert!(found.generated.is_none());
        // No `rimmerge.json` at all is the common case (an ordinary mod)
        // and must never itself be warned about — unlike the malformed
        // case above.
        assert!(!warnings.iter().any(|w| w.message.contains("rimmerge.json")));

        let _ = fs::remove_dir_all(&root);
    }

    /// [`infra::resolve_active_mods`](super::resolve_active_mods)'s own
    /// resolution, for a test that needs to build the exact
    /// `resolved_active` argument [`Discovered::inactive_excluding`]
    /// takes — a bare `[ModId]` isn't enough on its own (see that
    /// method's own doc comment for why), so every test below resolves
    /// its active ids through [`Discovered::lookup`] first, the same way
    /// the real scan path does.
    fn resolve<'a>(
        discovered: &'a Discovered,
        active: &'a [ModId],
    ) -> Vec<(&'a ModId, &'a DiscoveredMod)> {
        active
            .iter()
            .filter_map(|id| discovered.lookup(id).map(|m| (id, m)))
            .collect()
    }

    #[test]
    fn inactive_lists_every_discovered_mod_not_in_active_sorted_by_id() {
        let root = tempdir("inactive-basic");
        write_about(&root.join("Mods").join("Zzz"), "zzz.mod");
        write_about(&root.join("Mods").join("Aaa"), "aaa.mod");
        write_about(&root.join("Mods").join("Active"), "active.mod");

        let (discovered, _) = discover(
            &root,
            &root.join("workshop_does_not_exist"),
            GameVersion::new(1, 6),
        );

        let active = [ModId::new("active.mod")];
        let inactive = discovered.inactive_excluding(&resolve(&discovered, &active));
        let ids: Vec<ModId> = inactive.iter().map(|m| m.id.clone()).collect();
        assert_eq!(ids, vec![ModId::new("aaa.mod"), ModId::new("zzz.mod")]);

        let _ = fs::remove_dir_all(&root);
    }

    /// The basic test above only ever has two inactive entries in a
    /// `HashMap`, which a broken/removed sort passes roughly half the time by
    /// sheer luck. Ten mods, ids handed to `write_about` in reverse order (so
    /// directory-creation order — which is what iteration over
    /// `subdirectories`'s own sorted `Vec` and then insertion into a
    /// `HashMap` ultimately traces back to — is *also* the opposite of
    /// sorted, not merely uncorrelated with it), asserting the exact sorted
    /// vector: the odds of ten items independently landing in sorted order by
    /// chance are astronomically smaller than the two-item test's own ~50%.
    #[test]
    fn inactive_sorts_many_mods_by_id_regardless_of_hashmap_order() {
        let root = tempdir("inactive-many-sorted");
        let ids = [
            "j.mod", "i.mod", "h.mod", "g.mod", "f.mod", "e.mod", "d.mod", "c.mod", "b.mod",
            "a.mod",
        ];
        for id in ids {
            write_about(&root.join("Mods").join(id), id);
        }

        let (discovered, _) = discover(
            &root,
            &root.join("workshop_does_not_exist"),
            GameVersion::new(1, 6),
        );

        let inactive = discovered.inactive_excluding(&[]);
        let got_ids: Vec<ModId> = inactive.iter().map(|m| m.id.clone()).collect();
        let mut expected: Vec<ModId> = ids.iter().map(|id| ModId::new(*id)).collect();
        expected.sort();
        assert_eq!(got_ids, expected);

        let _ = fs::remove_dir_all(&root);
    }

    /// The `_steam` rule (`Discovered::active_mods_id`) is `Discovered::lookup`'s
    /// mirror: a Workshop copy shadowed by a local copy of the same
    /// packageId needs the `_steam` suffix to be named at all, even in
    /// the inactive list, since the bare id already belongs to the local
    /// copy.
    #[test]
    fn inactive_suffixes_a_shadowed_workshop_copy_with_steam() {
        let root = tempdir("inactive-steam-shadowed");
        let workshop = root.join("workshop");
        write_about(&root.join("Mods").join("DupLocal"), "dup.mod");
        write_about(&workshop.join("111111"), "dup.mod");

        let (discovered, _) = discover(&root, &workshop, GameVersion::new(1, 6));

        // Neither copy is active: both must appear, the workshop one
        // under the suffixed id.
        let inactive = discovered.inactive_excluding(&[]);
        let ids: Vec<ModId> = inactive.iter().map(|m| m.id.clone()).collect();
        assert_eq!(
            ids,
            vec![ModId::new("dup.mod"), ModId::new("dup.mod_steam")]
        );

        // Only the local copy active: the workshop copy is still
        // inactive, still under the suffixed id.
        let active = [ModId::new("dup.mod")];
        let inactive = discovered.inactive_excluding(&resolve(&discovered, &active));
        let ids: Vec<ModId> = inactive.iter().map(|m| m.id.clone()).collect();
        assert_eq!(ids, vec![ModId::new("dup.mod_steam")]);

        let _ = fs::remove_dir_all(&root);
    }

    /// A Workshop-only mod (no local copy on disk) never gets the
    /// `_steam` suffix in the inactive list either — the overwhelmingly
    /// common case, and the one `Discovered::lookup`'s own doc comment
    /// leads with.
    #[test]
    fn inactive_never_suffixes_a_workshop_only_mod() {
        let root = tempdir("inactive-workshop-only");
        let workshop = root.join("workshop");
        write_about(&workshop.join("222222"), "solo.mod");

        let (discovered, _) = discover(&root, &workshop, GameVersion::new(1, 6));

        let inactive = discovered.inactive_excluding(&[]);
        let ids: Vec<ModId> = inactive.iter().map(|m| m.id.clone()).collect();
        assert_eq!(ids, vec![ModId::new("solo.mod")]);

        let _ = fs::remove_dir_all(&root);
    }

    /// A Workshop-only mod (no `primary` copy at all) active under its
    /// `_steam`-suffixed id must not also appear inactive under the bare id —
    /// `Discovered::lookup` resolves `x.mod_steam` to this exact folder
    /// regardless of whether a `primary` copy exists (its own doc comment's
    /// "fallback for a bare id with no non-Workshop copy" sentence), so the
    /// folder is unambiguously active; an id-re-derivation approach would
    /// list it both ways (`scanned=["x.mod_steam"]`, `inactive=["x.mod"]`) —
    /// matching by path makes that impossible by construction.
    #[test]
    fn inactive_excludes_a_workshop_only_mod_active_under_its_steam_suffix() {
        let root = tempdir("inactive-workshop-only-steam-active");
        let workshop = root.join("workshop");
        write_about(&workshop.join("333333"), "x.mod");

        let (discovered, _) = discover(&root, &workshop, GameVersion::new(1, 6));

        let active = [ModId::new("x.mod_steam")];
        let resolved = resolve(&discovered, &active);
        assert_eq!(
            resolved.len(),
            1,
            "x.mod_steam must resolve to the workshop-only folder"
        );

        let inactive = discovered.inactive_excluding(&resolved);
        assert!(
            inactive.is_empty(),
            "the same folder must not also appear inactive under the bare id: {inactive:?}"
        );

        let _ = fs::remove_dir_all(&root);
    }

    /// `Discovered::all` — unlike `inactive_excluding` — includes every
    /// discovered mod regardless of active status, and imposes no order of
    /// its own (`infra::inventory` sorts what it needs sorted).
    #[test]
    fn all_includes_every_discovered_mod_active_or_not() {
        let root = tempdir("all-includes-active");
        write_about(&root.join("Mods").join("Zzz"), "zzz.mod");
        write_about(&root.join("Mods").join("Active"), "active.mod");

        let (discovered, _) = discover(
            &root,
            &root.join("workshop_does_not_exist"),
            GameVersion::new(1, 6),
        );

        let mut ids: Vec<ModId> = discovered.all().into_iter().map(|m| m.id).collect();
        ids.sort();
        assert_eq!(ids, vec![ModId::new("active.mod"), ModId::new("zzz.mod")]);

        let _ = fs::remove_dir_all(&root);
    }
}
