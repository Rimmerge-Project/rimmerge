//! The generated mod's `modDependencies` and `loadAfter` sets.

use std::collections::{BTreeMap, BTreeSet};

use rim_analyzer::domain::ModId;
use rim_resolve::domain::{
    AssignmentRow, AssignmentSchema, FieldRole, RowValue, TargetRef, majority_owner,
};

use crate::plan::CORE_MOD_ID;

/// `resolve(value)`'s pairs narrowed to `def_type`'s own owners.
fn owners_of(
    resolve: &dyn Fn(&str) -> Vec<(String, ModId)>,
    value: &str,
    def_type: &str,
) -> Vec<ModId> {
    resolve(value)
        .into_iter()
        .filter(|(t, _)| t == def_type)
        .map(|(_, owner)| owner)
        .collect()
}

/// Inserts `owner`'s base id into `deps`, unless it's Core: Core is
/// always active and never worth a `MayRequire`/dependency declaration,
/// the same rule `crate::plan`'s own `depends_on` already applies. A DLC
/// owner is not Core and is kept — only the base game is guaranteed
/// present regardless of declaration.
fn insert_non_core(deps: &mut BTreeSet<ModId>, owner: ModId) {
    let base = owner.base();
    if base.as_str() != CORE_MOD_ID {
        deps.insert(base);
    }
}

/// Whether `owner` (base-normalised) is this project's own package id —
/// a project's own free-standing instances resolve, per the caller's own index,
/// either to this project's own id or to nothing at all (this module's
/// own doc comment); either way they are never a real external
/// dependency.
fn is_own_package(owner: &ModId, own_package_id: &ModId) -> bool {
    owner.base() == own_package_id.base()
}

/// [`dependencies`]/[`standalone_dependencies`]'s shared core: the
/// framework (the assignment def type's own DLL owner) plus the majority
/// owner of every item named in any of `rows`' own
/// [`FieldRole::ItemSlot`] values — the two public functions differ only
/// in which row collection they iterate. `own_package_id` is
/// base-normalised and dropped from the result exactly like Core
/// ([`is_own_package`]).
fn dependencies_over<'a>(
    schema: &AssignmentSchema,
    rows: impl IntoIterator<Item = &'a AssignmentRow>,
    own_package_id: &ModId,
    resolve: &dyn Fn(&str) -> Vec<(String, ModId)>,
    dll_owner: &dyn Fn(&str) -> Option<ModId>,
) -> BTreeSet<ModId> {
    let mut deps = BTreeSet::new();
    if let Some(framework) = dll_owner(&schema.def_type) {
        insert_non_core(&mut deps, framework);
    }
    for row in rows {
        for (path, spec) in &schema.fields {
            let FieldRole::ItemSlot { def_type } = &spec.role else {
                continue;
            };
            let Some(RowValue::Names(names)) = row.values.get(path) else {
                continue;
            };
            for name in names {
                let owners = owners_of(resolve, name, def_type);
                let Some(owner) = majority_owner(&owners) else {
                    continue;
                };
                if is_own_package(&owner, own_package_id) {
                    continue;
                }
                insert_non_core(&mut deps, owner);
            }
        }
    }
    deps
}

/// The mods this project's own emitted content depends on —
/// the framework (the assignment def type's own DLL owner, via
/// `dll_owner`; `None` when the type is vanilla) plus the majority owner
/// of every item named in any row's [`FieldRole::ItemSlot`] value, using
/// the same vote [`rim_resolve::domain::majority_owner`] defines for
/// field classification. Base-normalized and Core-excluded
/// ([`insert_non_core`]); `own_package_id` is excluded the same way
/// ([`is_own_package`]) — an item slot naming one of this
/// project's own free-standing rows never becomes a dependency on
/// itself. An item name that resolves to no owner at all (including,
/// after that exclusion, an own instance the caller's `resolve` never
/// indexes at all) contributes
/// nothing here (nothing to depend on) — that is a different fact from a
/// row that fails validation entirely, which is a later export step's
/// own `skipped` list to surface, not this pure computation's job.
/// Deliberately narrower than the full dependency rule — [`load_after_only`]
/// is the other half a caller unions in.
#[must_use]
pub fn dependencies(
    schema: &AssignmentSchema,
    rows: &BTreeMap<TargetRef, AssignmentRow>,
    own_package_id: &ModId,
    resolve: &dyn Fn(&str) -> Vec<(String, ModId)>,
    dll_owner: &dyn Fn(&str) -> Option<ModId>,
) -> BTreeSet<ModId> {
    dependencies_over(schema, rows.values(), own_package_id, resolve, dll_owner)
}

/// [`dependencies`]'s sibling for a free-standing ("new def") section's
/// own rows (`BTreeMap<String /* defName */, AssignmentRow>`, the shape
/// `Section::rows` flattens to by [`RowKey::Own`](rim_resolve::domain::RowKey::Own)'s own name) — identical
/// rule, just over the other row collection a schema with no `TargetKey`
/// field ever populates.
#[must_use]
pub fn standalone_dependencies(
    schema: &AssignmentSchema,
    rows: &BTreeMap<String, AssignmentRow>,
    own_package_id: &ModId,
    resolve: &dyn Fn(&str) -> Vec<(String, ModId)>,
    dll_owner: &dyn Fn(&str) -> Option<ModId>,
) -> BTreeSet<ModId> {
    dependencies_over(schema, rows.values(), own_package_id, resolve, dll_owner)
}

/// The `loadAfter`-only half: `targets` (T), base-normalized and
/// Core-excluded the same way [`dependencies`]'s own `modDependencies` set
/// is ([`insert_non_core`]) — a target-only mod never becomes a
/// `modDependencies` entry (this project's export must still work without
/// it installed; it's a `MayRequire` gate, not a hard dependency), but
/// still needs a load-order constraint relative to this project's own mod.
/// The caller unions this with [`dependencies`]'s own result and passes both to
/// [`crate::emit::Dependencies::ExactlyWithLoadAfter`].
#[must_use]
pub fn load_after_only(targets: &BTreeSet<ModId>) -> BTreeSet<ModId> {
    let mut result = BTreeSet::new();
    for target in targets {
        insert_non_core(&mut result, target.clone());
    }
    result
}
