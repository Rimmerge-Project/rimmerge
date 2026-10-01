//! Target gates, and validating a project's sections against the active mods before rendering.

use std::collections::BTreeMap;

use rim_analyzer::domain::{ModId, Source};
use rim_merge::assign::TargetGate;
use rim_resolve::domain::{
    AssignmentProject, FieldRole, KnownDefs, RowKey, RowValue, Section, TargetRef,
};

use super::outcome::AssignmentSkip;
use crate::Session;
use crate::assignment_refs::SessionKnownDefs;

/// Whether `owner` is registered as [`Source::Core`] in the active scan —
/// the gate a candidate's own row renders `MayRequire` against (Core
/// ungated, everything else — DLC included — gated).
fn is_core(session: &Session, owner: &ModId) -> bool {
    session
        .report()
        .mods
        .iter()
        .find(|m| m.id.base() == owner.base())
        .is_some_and(|m| m.source == Source::Core)
}

/// Every row's own [`TargetGate`], from the target's winning owner (the
/// last-loaded-in-`order` owner sharing its `(def_type, def_name)` key,
/// matching an ordinary `DefOverride`'s own rule) — a target with no
/// active owner at all in [`rim_analyzer::analysis::SourceIndex::owners_by_def`]
/// gets no entry, letting [`assign::render_rows`](rim_merge::assign::render_rows)' own no-gate
/// handling skip it as "no gate decision" rather than this use case guessing one.
pub(super) fn build_gates(
    session: &Session,
    sections: &BTreeMap<String, Section>,
) -> BTreeMap<TargetRef, TargetGate> {
    let order = session.orders().get(session.selected()).clone();
    let mut gates = BTreeMap::new();
    for section in sections.values() {
        for key in section.rows.keys() {
            let RowKey::Target(target) = key else {
                continue;
            };
            let lookup_key = (target.def.def_type.clone(), target.def.def_name.clone());
            let Some(owners) = session.sources().owners_by_def.get(&lookup_key) else {
                continue;
            };
            let Some(owner) = order
                .as_slice()
                .iter()
                .rev()
                .find(|id| owners.contains(id))
                .cloned()
            else {
                continue;
            };
            let gate = if is_core(session, &owner) {
                TargetGate::Core
            } else {
                TargetGate::Mod(owner.base())
            };
            gates.insert(target.clone(), gate);
        }
    }
    gates
}

/// Drops (and records as an [`AssignmentSkip`]) any [`FieldRole::ItemSlot`]
/// value naming an item that no longer exists in the active list *and*
/// isn't a currently-known own instance of another live section — a
/// choice made when the item's own owning mod was still active, since
/// gone inactive (`known.contains`), or a free-standing row that has
/// itself since been cleared while its own section survives
/// (`known.own_instances`). **This is the one and only place an
/// `ItemSlot` value is checked against "still there"**:
/// `rim_merge::assign::render_rows`/`render_sections` carry no guard of
/// their own. A second, engine-side guard would be redundant with this
/// function for the engine's one real caller (this use case) — every name
/// this function lets through already satisfies
/// `known.contains(item_type, name) || own.contains(name)` — and wrong on
/// top: the engine has no active-install index, so it could only list
/// the `own` half, and whenever a project carried a section for the
/// referenced item type it would re-reject a perfectly legitimate
/// *active* reference this function had already approved, blocking a
/// project's own new instance from being referenced *alongside* external
/// instances of the same type. The force-removed-section case is caught
/// here: `known.own_instances(item_type)` reads
/// `project.section(item_type)` fresh on every call, so a force-removed
/// section's former own name is simply absent from `own` and the
/// reference is skipped exactly like any other vanished item (pinned by
/// this module's own test
/// `a_reference_to_a_force_removed_sections_own_row_is_skipped_with_a_reason`).
/// [`assign::render_sections`](rim_merge::assign::render_sections) itself performs no
/// validation of its own; this is the sole, session-backed check, run once up front
/// (across every section) so a vanished item's field is omitted from the
/// render exactly like any other omitted field, rather than shipping a
/// dangling reference.
///
/// **Load-bearing invariant this function's whole "sole authority" claim
/// rests on**:
/// `known.contains(item_type, name)` means "active" only because
/// `SessionKnownDefs::contains` (`assignment_refs.rs`) reads
/// `SourceIndex::owners_by_def`, which `rim_analyzer::analysis::source_index::build`
/// populates by iterating `scan.load_order` alone (`crates/rim-analyzer/src/analysis/source_index.rs`) —
/// an *inactive* mod's defs are never indexed there at all. If that index
/// ever grows to also cover inactive mods (or a future caller starts
/// consulting a different, broader index for `contains`), "active" would
/// silently become "known to the scan, active or not", this function
/// would stop actually filtering out a vanished item's stale reference,
/// and — because the engine has no guard of its own — there would
/// be no second check behind it to catch the regression. Anyone
/// changing `owners_by_def`'s own population rule should re-verify this
/// function's own contract before shipping.
pub(super) fn validate_and_clean_sections(
    session: &Session,
    project: &AssignmentProject,
) -> (BTreeMap<String, Section>, Vec<AssignmentSkip>) {
    let known = SessionKnownDefs::new(session, project.identity().package_id().clone(), project);
    let mut skipped = Vec::new();
    let mut cleaned_sections = BTreeMap::new();

    for (def_type, section) in project.sections() {
        let mut cleaned_rows = BTreeMap::new();
        for (key, row) in &section.rows {
            let mut row = row.clone();
            for (path, spec) in &section.schema.fields {
                let FieldRole::ItemSlot {
                    def_type: item_type,
                } = &spec.role
                else {
                    continue;
                };
                let Some(RowValue::Names(names)) = row.values.get(path) else {
                    continue;
                };
                let own = known.own_instances(item_type);
                let missing: Vec<&str> = names
                    .iter()
                    .map(String::as_str)
                    .filter(|name| !known.contains(item_type, name) && !own.contains(*name))
                    .collect();
                if missing.is_empty() {
                    continue;
                }
                skipped.push(AssignmentSkip {
                    def_type: def_type.clone(),
                    row: key.clone(),
                    path: path.clone(),
                    reason: format!(
                        "item(s) {} no longer exist in the active list",
                        missing.join(", ")
                    ),
                });
                row.values.remove(path);
            }
            cleaned_rows.insert(key.clone(), row);
        }
        cleaned_sections.insert(
            def_type.clone(),
            Section {
                schema: section.schema.clone(),
                rows: cleaned_rows,
            },
        );
    }

    (cleaned_sections, skipped)
}
