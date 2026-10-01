//! Detects mods whose real engine lives outside `Assemblies/`.
//!
//! The analyzer only reads `Assemblies/**/*.dll` (matching what RimWorld
//! itself loads) — see `infra::mod_scan`. A few mods ship a small loader
//! there that side-loads its real engine at runtime from another folder
//! (e.g. aurelian's Aurora Framework, which loads its components from
//! `Aurora/Components/`). For those mods this analyzer reports zero
//! runtime patches and zero assembly references from the unseen code,
//! which reads as "no conflicts exist" when the truth is "we didn't
//! look" — worst for exactly the mods where it matters most: performance
//! mods and frameworks that patch hot vanilla methods.
//!
//! This module only classifies paths and decides whether to note it;
//! `infra::mod_scan` does the actual filesystem walk (see this crate's
//! `extract`/`infra` split).

use std::collections::BTreeSet;
use std::path::Path;

/// Directory names that mark a dev/build artifact, never a real side-loaded
/// engine — a mod's `Source/` tree commonly contains its own `obj`/`bin`
/// output, sometimes with a nested copy of a framework's DLLs.
/// `packages`/`lib`/`nuget`/`.nuget` cover a committed NuGet restore tree — a
/// mod whose `Source/` directory (or repo root) is checked in wholesale can
/// carry `packages/<name>.<version>/lib/net*/*.dll` for every target
/// framework NuGet restored, none of them ever loaded by RimWorld: a real
/// install's checklist mod (workshop id 3000000007) ships exactly this shape,
/// `loaded=3` (`Assemblies/`) vs. `side=7`
/// (`packages/lib.examplepatchlib.2.1.0/lib/net{35,45,472,48,5.0,netcoreapp3.0,
/// netcoreapp3.1}`), firing this module's note for a mod that side-loads
/// nothing at all. A slice, not a fixed-size array — its length is not worth
/// maintaining by hand.
const DEV_DIR_NAMES: &[&str] = &[
    "source", "obj", "bin", ".git", ".vs", "packages", "lib", "nuget", ".nuget",
];

/// Directory names for a mod's own legacy/backup copy of its assemblies —
/// kept for rollback or reference, never loaded by RimWorld. A slice, not
/// a fixed-size array, for the same reason as [`DEV_DIR_NAMES`].
const INERT_DIR_NAMES: &[&str] = &[
    "oldassemblies",
    "rollback",
    "assemblydump",
    "references",
    "lastversion",
];

/// Whether `name` (already lowercased) is a dev or inert-looking directory
/// name — one `classify` always ignores regardless of what it contains, since
/// the dev/inert-first precedence in [`classify`] means no case (not even a
/// nested `Assemblies/` component) depends on a dev/inert directory's own
/// content. `pub(crate)` so `infra::mod_scan`'s own walk can prune these
/// directories entirely via `filter_entry` instead of duplicating this name
/// list — a single source of truth for "always ignored regardless of
/// content".
#[must_use]
pub(crate) fn is_always_ignored(name: &str) -> bool {
    DEV_DIR_NAMES.contains(&name) || INERT_DIR_NAMES.contains(&name)
}

/// Where one `.dll` path (relative to its mod's root) falls for side-load
/// detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssemblyLocation {
    /// Under an `Assemblies/` directory (any depth) — what RimWorld itself
    /// loads, and what this analyzer already reads.
    Loaded,
    /// Outside `Assemblies/`, and not under a dev or inert-looking
    /// directory — a candidate for a genuinely side-loaded engine.
    SideLoaded,
    /// Outside `Assemblies/`, but under a dev or inert-looking directory —
    /// never counted either way: a leftover `Source`/`obj`/`bin` build
    /// artifact, or a deliberate rollback/reference copy.
    Ignored,
}

/// Classifies `relative_path` (relative to the mod's root directory) by
/// whether any path component names a dev or inert-looking directory,
/// then an `Assemblies` one, case-insensitively.
///
/// **Dev/inert is checked *first*, before `Assemblies`.** An
/// `Assemblies`-first order is not harmless: a vendored
/// `Source/**/Assemblies/**` copy (a full mod bundled into another mod's own
/// source tree, `Assemblies` and all) would inflate `loaded_count`, and
/// `note`'s gate is `side_loaded > loaded` — so a genuinely side-loading mod
/// whose *own* loader also happens to ship such a vendored copy could have
/// its true side-loaded engine silently hidden. Dev/inert-first is also the
/// semantically true answer: RimWorld never loads a DLL under `Source/` (or
/// any other dev/inert directory) even when it happens to sit inside a folder
/// named `Assemblies`, so such a path is never really `Loaded`.
#[must_use]
pub fn classify(relative_path: &Path) -> AssemblyLocation {
    let components: Vec<String> = relative_path
        .components()
        .filter_map(|c| c.as_os_str().to_str())
        .map(str::to_lowercase)
        .collect();

    if components.iter().any(|c| is_always_ignored(c)) {
        return AssemblyLocation::Ignored;
    }
    if components.iter().any(|c| c == "assemblies") {
        return AssemblyLocation::Loaded;
    }
    AssemblyLocation::SideLoaded
}

/// [`note`]'s two same-typed inputs, grouped into one type so a call site
/// cannot swap them silently — two positional `usize` counts would give the
/// compiler no way to catch `note(side_loaded, loaded)` typed the wrong way
/// round.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SideLoadCounts {
    /// DLLs found under an `Assemblies/` component — what RimWorld loads.
    pub loaded: usize,
    /// DLLs found outside `Assemblies/`, dev, and inert-looking
    /// directories — genuinely unanalyzed.
    pub side_loaded: usize,
}

/// Builds the scan-note message for a mod whose side-loaded DLL count
/// exceeds its loaded one, or `None` when it shouldn't fire. Worded
/// informationally, not as a fault — shipping a loader that side-loads its
/// engine is legitimate; this only discloses what the report couldn't see.
///
/// `offending_top_level` names the top-level folder(s) (relative to the mod's
/// root) the side-loaded DLLs were found under, sorted — naming them makes a
/// false positive self-evident from the note text alone (e.g. a NuGet
/// `packages/` restore tree not yet on `DEV_DIR_NAMES`) instead of an
/// unexplained bare count a reader has to go spelunking on disk to explain.
#[must_use]
pub fn note(counts: SideLoadCounts, offending_top_level: &BTreeSet<String>) -> Option<String> {
    if counts.side_loaded == 0 || counts.side_loaded <= counts.loaded {
        return None;
    }
    let folders = offending_top_level
        .iter()
        .map(|name| format!("{name}/"))
        .collect::<Vec<_>>()
        .join(", ");
    let side_loaded_count = counts.side_loaded;
    Some(format!(
        "{side_loaded_count} assemblies outside `Assemblies/` ({folders}) were not analyzed \
         — runtime patches and assembly references in them are not represented \
         in this report."
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn classifies_any_depth_under_assemblies_as_loaded() {
        assert_eq!(
            classify(&PathBuf::from("Assemblies/Foo.dll")),
            AssemblyLocation::Loaded
        );
        assert_eq!(
            classify(&PathBuf::from("1.6/Assemblies/Sub/Foo.dll")),
            AssemblyLocation::Loaded
        );
    }

    #[test]
    fn assemblies_component_match_is_case_insensitive() {
        assert_eq!(
            classify(&PathBuf::from("ASSEMBLIES/Foo.dll")),
            AssemblyLocation::Loaded
        );
    }

    #[test]
    fn classifies_a_plain_outside_folder_as_side_loaded() {
        assert_eq!(
            classify(&PathBuf::from("Aurora/Components/Foo.dll")),
            AssemblyLocation::SideLoaded
        );
    }

    #[test]
    fn dev_directories_are_ignored_case_insensitively() {
        for dir in ["Source", "obj", "BIN", ".git", ".vs"] {
            let path = PathBuf::from(format!("{dir}/Foo.dll"));
            assert_eq!(classify(&path), AssemblyLocation::Ignored, "{dir}");
        }
    }

    #[test]
    fn inert_directories_are_ignored_case_insensitively() {
        for dir in [
            "OldAssemblies",
            "RollBack",
            "AssemblyDump",
            "References",
            "LastVersion",
        ] {
            let path = PathBuf::from(format!("{dir}/Foo.dll"));
            assert_eq!(classify(&path), AssemblyLocation::Ignored, "{dir}");
        }
    }

    #[test]
    fn a_nested_dev_directory_is_still_ignored() {
        // A vendored source copy of another mod's build output, nested
        // several levels down — the dev-dir check must look at every
        // component, not just the first.
        assert_eq!(
            classify(&PathBuf::from("Vendor/ExampleLib/Source/obj/Debug/Foo.dll")),
            AssemblyLocation::Ignored
        );
    }

    #[test]
    fn a_dev_directory_wins_over_an_assemblies_component_nested_inside_it() {
        // A dev directory is checked *first* — a vendored full mod copy under
        // `Source/`, `Assemblies` and all, reads as `Ignored`, not `Loaded`,
        // since RimWorld never loads a DLL under `Source/` regardless of what
        // it's nested inside.
        assert_eq!(
            classify(&PathBuf::from(
                "Vendor/ExampleLib/Source/Assemblies/Foo.dll"
            )),
            AssemblyLocation::Ignored
        );
    }

    /// A real install's counter-example — a committed NuGet restore tree
    /// (`packages/lib.examplepatchlib.2.1.0/lib/net{35,45,472}`, a checklist mod,
    /// workshop id 3000000007) must classify `Ignored`, not `SideLoaded`,
    /// since `packages`/`lib` are denylisted.
    #[test]
    fn a_committed_nuget_restore_tree_is_ignored() {
        for path in [
            "packages/Lib.ExamplePatchLib.2.1.0/lib/net35/0ExamplePatchLib.dll",
            "packages/Lib.ExamplePatchLib.2.1.0/lib/net472/0ExamplePatchLib.dll",
        ] {
            assert_eq!(
                classify(&PathBuf::from(path)),
                AssemblyLocation::Ignored,
                "{path}"
            );
        }
    }

    fn folders(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|n| (*n).to_string()).collect()
    }

    #[test]
    fn note_fires_only_when_side_loaded_exceeds_loaded() {
        let some = folders(&["Lunar"]);
        assert!(
            note(
                SideLoadCounts {
                    loaded: 5,
                    side_loaded: 18
                },
                &some
            )
            .is_some()
        );
        assert!(
            note(
                SideLoadCounts {
                    loaded: 4,
                    side_loaded: 15
                },
                &some
            )
            .is_some()
        );
        assert!(
            note(
                SideLoadCounts {
                    loaded: 0,
                    side_loaded: 2
                },
                &some
            )
            .is_some()
        );
    }

    #[test]
    fn note_does_not_fire_when_loaded_meets_or_exceeds_side_loaded() {
        let some = folders(&["Lunar"]);
        assert!(
            note(
                SideLoadCounts {
                    loaded: 3,
                    side_loaded: 3
                },
                &some
            )
            .is_none()
        );
        assert!(
            note(
                SideLoadCounts {
                    loaded: 3,
                    side_loaded: 2
                },
                &some
            )
            .is_none()
        );
        assert!(
            note(
                SideLoadCounts {
                    loaded: 10,
                    side_loaded: 0
                },
                &some
            )
            .is_none()
        );
    }

    #[test]
    fn note_never_fires_on_zero_side_loaded_regardless_of_loaded_count() {
        assert!(
            note(
                SideLoadCounts {
                    loaded: 0,
                    side_loaded: 0
                },
                &BTreeSet::new()
            )
            .is_none()
        );
    }

    #[test]
    fn note_wording_names_the_side_loaded_count_and_the_offending_folders() {
        let message = note(
            SideLoadCounts {
                loaded: 1,
                side_loaded: 5,
            },
            &folders(&["Lunar", "packages"]),
        )
        .expect("must fire");
        assert!(message.starts_with("5 assemblies outside"));
        assert!(
            message.contains("(Lunar/, packages/)"),
            "expected the offending folders named: {message}"
        );
        assert!(message.contains("not analyzed"));
        assert!(message.contains("not represented in this report"));
    }
}
