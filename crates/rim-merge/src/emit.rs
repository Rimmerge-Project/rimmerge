//! Turns [`MergePlan`]s into the generated merge mod's files:
//! `About/About.xml`, one `Patches/rimmerge_<DefType>.xml` per def type,
//! a `Textures/<path>` copy per shipped asset, and `rimmerge.json`.
//!
//! Pure text generation — nothing here touches a filesystem;
//! [`RenderedFile::content`] names either literal text or a source path
//! for the caller (`rim-session`'s `MergeModWriter`) to copy.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use rim_analyzer::domain::ModId;
pub use rim_resolve::domain::GeneratedModIdentity;

use crate::plan::MergePlan;
use about::render_about_xml;
use patches::render_patches_file;
use rimmerge_json::{display_name_for, render_rimmerge_json};

mod about;
mod input;
mod patches;
mod rendered;
mod rimmerge_json;

pub use input::{AboutSpec, AssetCopy, Dependencies, EmitInput, Provenance};
pub use patches::patch_file_path;
pub use rendered::{
    EmitError, FileContent, RenderedDefsFile, RenderedFile, RenderedMod, defs_file_path,
};

// Sibling-file tests; `#[path]` keeps the module named `tests`.
#[cfg(test)]
#[path = "emit/emit_tests.rs"]
mod tests;

/// Renders the generated merge mod's files from `input`. Fully
/// deterministic for a [`Provenance::CompatPatch`] render; for a
/// [`Provenance::ProfileMerge`] one, every file but `rimmerge.json` is
/// (its `generated_at` timestamp isn't).
///
/// # Errors
///
/// [`EmitError::IncompletePlan`] if any plan in [`EmitInput::plans`]
/// still has unresolved fields; [`EmitError::UndeclaredDependency`] if
/// [`AboutSpec::dependencies`] is [`Dependencies::Exactly`] and some op or
/// asset depends on a mod outside it.
pub fn render(input: &EmitInput<'_>) -> Result<RenderedMod, EmitError> {
    for plan in input.plans {
        if !plan.unresolved.is_empty() {
            return Err(EmitError::IncompletePlan {
                key: plan.key.clone(),
                unresolved: plan.unresolved.len(),
            });
        }
    }

    // Every mod this render's actual content depends on — base-normalized
    // (`PlannedOp::depends_on` is documented as base-only, but normalizing
    // here too, matching the asset loop below, means a `_steam`-suffixed
    // and base copy of the same owner always collapse to one entry rather
    // than trusting that invariant across a crate boundary).
    let mut used: BTreeSet<ModId> = BTreeSet::new();
    for plan in input.plans {
        for op in &plan.ops {
            for id in &op.depends_on {
                used.insert(id.base());
            }
        }
    }
    for asset in input.assets {
        used.insert(asset.from.base());
    }

    let names_for = |ids: &BTreeSet<ModId>| -> BTreeMap<ModId, String> {
        ids.iter()
            .map(|id| {
                (
                    id.clone(),
                    display_name_for(input.mod_names, id).to_string(),
                )
            })
            .collect()
    };
    let (depends_on, load_after): (BTreeMap<ModId, String>, BTreeSet<ModId>) =
        match input.about.dependencies {
            Dependencies::FromContent => {
                let depends_on = names_for(&used);
                let load_after = depends_on.keys().cloned().collect();
                (depends_on, load_after)
            }
            Dependencies::Exactly(scope) => {
                if let Some(mod_id) = used.iter().find(|id| !scope.contains(id)) {
                    return Err(EmitError::UndeclaredDependency {
                        mod_id: mod_id.clone(),
                    });
                }
                (names_for(scope), scope.clone())
            }
            Dependencies::ExactlyWithLoadAfter {
                depends_on: scope,
                load_after_only,
            } => {
                if let Some(mod_id) = used.iter().find(|id| !scope.contains(id)) {
                    return Err(EmitError::UndeclaredDependency {
                        mod_id: mod_id.clone(),
                    });
                }
                let load_after = scope.union(load_after_only).cloned().collect();
                (names_for(scope), load_after)
            }
        };

    let mut files = vec![RenderedFile {
        relative_path: PathBuf::from("About/About.xml"),
        content: FileContent::Text(render_about_xml(
            &input.about,
            input.game_version,
            &depends_on,
            &load_after,
        )),
    }];

    for defs_file in input.defs {
        files.push(RenderedFile {
            relative_path: defs_file.relative_path.clone(),
            content: FileContent::Text(defs_file.content.clone()),
        });
    }

    let mut plans_by_def_type: BTreeMap<&str, Vec<&MergePlan>> = BTreeMap::new();
    for plan in input.plans {
        plans_by_def_type
            .entry(plan.key.def_type.as_str())
            .or_default()
            .push(plan);
    }
    for (def_type, plans) in plans_by_def_type {
        if let Some(text) = render_patches_file(&plans) {
            files.push(RenderedFile {
                relative_path: patch_file_path(def_type),
                content: FileContent::Text(text),
            });
        }
    }

    for asset in input.assets {
        files.push(RenderedFile {
            relative_path: PathBuf::from("Textures").join(&asset.relative_target),
            content: FileContent::CopyFrom(asset.source.clone()),
        });
    }

    files.push(RenderedFile {
        relative_path: PathBuf::from("rimmerge.json"),
        content: FileContent::Text(render_rimmerge_json(
            &input.provenance,
            input.rimmerge_version,
        )),
    });

    Ok(RenderedMod {
        folder_name: input.about.identity.folder_name.clone(),
        files,
    })
}
