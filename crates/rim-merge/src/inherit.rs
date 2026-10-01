//! `ParentName` chain resolution: RimWorld's `XmlInheritance` merge,
//! reimplemented over [`FieldTree`].

use std::collections::BTreeMap;

use crate::tree::{Content, FieldNode, FieldTree};

/// A template's identity as [`TemplateSet`] keys it: `(def_type, Name)`.
type TemplateKey = (String, String);

/// Every template a `ParentName` chain can reach, keyed by
/// `(def_type, Name)` — built by the session from its `SourceIndex` plus
/// a `DefSourceReader`; [`resolve`] itself does no IO and doesn't care
/// where the trees came from.
#[derive(Debug, Clone, Default)]
pub struct TemplateSet {
    by_name: BTreeMap<TemplateKey, FieldTree>,
}

impl TemplateSet {
    /// Builds a template set from its `(def_type, Name) -> tree` map.
    #[must_use]
    pub fn new(by_name: BTreeMap<(String, String), FieldTree>) -> Self {
        Self { by_name }
    }

    /// Looks up one template by its `(def_type, Name)` key. `pub(crate)`
    /// — [`crate::effective`] walks the same chain [`resolve`] does (to
    /// attribute each inherited field to the template that supplied it,
    /// which a merged [`FieldNode`] alone can't tell you), so it needs the
    /// same lookup this module's own [`resolve_chain`] uses internally.
    #[must_use]
    pub(crate) fn get(&self, def_type: &str, name: &str) -> Option<&FieldTree> {
        self.by_name.get(&(def_type.to_string(), name.to_string()))
    }
}

/// Why [`resolve`] couldn't produce a resolved tree.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InheritError {
    /// A `ParentName` named a template not present in the [`TemplateSet`].
    #[error("missing parent template {def_type}/{name}")]
    MissingParent {
        /// The def type the chain was walking.
        def_type: String,
        /// The missing template's `Name`.
        name: String,
    },
    /// A `ParentName` chain revisited a name it had already visited.
    #[error("inheritance cycle: {}", chain.join(" -> "))]
    Cycle {
        /// The repeated name, appended to the chain that led back to it.
        chain: Vec<String>,
    },
}

/// Resolves `raw`'s `ParentName` chain against `templates`, applying
/// RimWorld's `XmlInheritance` merge law at every step:
///
/// - walk from the root-most ancestor down to `raw` itself; at each step,
///   `merge_over(parent_resolved, child_raw)`;
/// - `merge_over`: **an unverified assumption** — the child's attributes *replace*
///   the parent's wholesale (not a union: a child element with zero
///   attributes of its own resolves with zero attributes, even if the
///   parent's counterpart had some), except `Inherit`, which never
///   survives into a resolved tree regardless; if the child element
///   carries `Inherit="False"`, the child replaces the parent element
///   wholesale; otherwise, for each child element: `li` is always
///   appended (list items are never matched by name); a same-named
///   element already in the parent recurses; anything else is appended.
///   A child with [`Content::Text`] replaces the parent's whole content
///   outright (a leaf has nothing to merge into); a child with
///   [`Content::Empty`] (no text, no children, no `Inherit="False"`) is
///   *not* an override at all — it contributes nothing, so the parent's
///   content survives untouched (`Inherit="False"` is RimWorld's only
///   documented way to actually blank a value; a bare empty element
///   isn't one).
/// - every `Inherit` attribute is stripped from the *entire* resolved
///   tree before it's returned, not just from nodes `merge_over` itself
///   touched — a child subtree appended wholesale (no parent counterpart
///   to recurse into) can carry a nested `Inherit="False"` several levels
///   down that never passes through the wholesale-replace branch above,
///   and "`Inherit` never appears in the result" has to hold everywhere,
///   not just at directly-merged nodes.
///
/// This assumes a `ParentName` always names a template of the same def
/// type as the child (true of every RimWorld mod encountered so far, and
/// how [`TemplateSet`] itself is keyed) — a template chain that crosses
/// def types is out of scope. Attribute replacement (not union) is
/// unverified against a real RimWorld install.
///
/// # Errors
///
/// [`InheritError::MissingParent`] when a `ParentName` isn't in
/// `templates`; [`InheritError::Cycle`] when the chain revisits a name.
pub fn resolve(raw: &FieldTree, templates: &TemplateSet) -> Result<FieldTree, InheritError> {
    let chain_root = resolve_chain(raw.parent_name.as_deref(), &raw.root.tag, templates)?;
    let mut root = match chain_root {
        None => raw.root.clone(),
        Some(parent) => merge_over(&parent, &raw.root),
    };
    strip_inherit_recursive(&mut root);

    Ok(FieldTree {
        root,
        parent_name: None,
        name: None,
    })
}

/// Resolves just `raw`'s ancestor chain, ignoring everything `raw` itself
/// contributes — `None` when `raw` has no parent at all (nothing to
/// resolve). This is [`OwnerVersion::inherited`](crate::diff::OwnerVersion::inherited):
/// the "would this field survive without raw's own override" check
/// [`crate::plan::plan_def_override`]'s drop rule needs, to tell "raw is
/// the only source of this item" apart from "an ancestor independently
/// supplies it too" (in which case a plain removal of raw's own copy
/// would let the ancestor's resurface).
///
/// # Errors
///
/// Same as [`resolve`].
pub fn resolve_inherited_only(
    raw: &FieldTree,
    templates: &TemplateSet,
) -> Result<Option<FieldTree>, InheritError> {
    let Some(mut root) = resolve_chain(raw.parent_name.as_deref(), &raw.root.tag, templates)?
    else {
        return Ok(None);
    };
    strip_inherit_recursive(&mut root);
    Ok(Some(FieldTree {
        root,
        parent_name: None,
        name: None,
    }))
}

/// Walks the ancestor chain named by `parent_name`, root-most ancestor
/// first, pairing each template with its own `(def_type, Name)` key —
/// what [`resolve_chain`] folds over (discarding the keys, since a merged
/// [`FieldNode`] has no room for them) and what
/// [`crate::effective::compute`] needs *instead of* the fold, to attribute
/// each field to the template that supplied it. `pub(crate)` for that
/// reuse.
///
/// # Errors
///
/// [`InheritError::MissingParent`] when a `ParentName` isn't in
/// `templates`; [`InheritError::Cycle`] when the chain revisits a name.
pub(crate) fn ancestor_chain<'t>(
    parent_name: Option<&str>,
    def_type: &str,
    templates: &'t TemplateSet,
) -> Result<Vec<(&'t FieldTree, TemplateKey)>, InheritError> {
    let mut chain: Vec<(&FieldTree, TemplateKey)> = Vec::new();
    let mut visited: Vec<String> = Vec::new();
    let mut current = parent_name.map(str::to_string);

    while let Some(name) = current {
        if visited.contains(&name) {
            let mut offending = visited.clone();
            offending.push(name);
            return Err(InheritError::Cycle { chain: offending });
        }
        visited.push(name.clone());
        let template =
            templates
                .get(def_type, &name)
                .ok_or_else(|| InheritError::MissingParent {
                    def_type: def_type.to_string(),
                    name: name.clone(),
                })?;
        chain.push((template, (def_type.to_string(), name.clone())));
        current = template.parent_name.clone();
    }
    chain.reverse();
    Ok(chain)
}

/// Folds `merge_over` across [`ancestor_chain`] (root-most ancestor
/// first), returning the fully-merged chain root — `None` when
/// `parent_name` is `None` (nothing to resolve). Shared by [`resolve`]
/// (which merges `raw` over this) and [`resolve_inherited_only`] (which
/// returns this directly).
fn resolve_chain(
    parent_name: Option<&str>,
    def_type: &str,
    templates: &TemplateSet,
) -> Result<Option<FieldNode>, InheritError> {
    let chain = ancestor_chain(parent_name, def_type, templates)?;
    let mut merged: Option<FieldNode> = None;
    for (ancestor, _key) in &chain {
        merged = Some(match merged {
            None => ancestor.root.clone(),
            Some(parent) => merge_over(&parent, &ancestor.root),
        });
    }
    Ok(merged)
}

/// `pub(crate)` — [`crate::effective`]'s own merge-law mirror (it tracks
/// provenance in lockstep with [`merge_over`]/[`merge_children`] rather
/// than reimplementing them) needs the same wholesale-replace check this
/// module's [`merge_over`] uses.
pub(crate) fn is_inherit_false(node: &FieldNode) -> bool {
    node.attrs
        .get("Inherit")
        .is_some_and(|value| value.eq_ignore_ascii_case("false"))
}

/// Removes `Inherit` from `node` and every descendant, regardless of
/// which merge branch produced them — see [`resolve`]'s doc comment for
/// why a single per-node strip inside `merge_over` isn't enough on its
/// own. `pub(crate)` — [`crate::effective`] builds its own resolved tree
/// via the same [`merge_over`] fold [`resolve`] does (to guarantee its
/// provenance and its tree can never disagree), so it needs this same
/// final step too.
pub(crate) fn strip_inherit_recursive(node: &mut FieldNode) {
    node.attrs.remove("Inherit");
    if let Content::Children(children) = &mut node.content {
        for child in children {
            strip_inherit_recursive(child);
        }
    }
}

/// `pub(crate)` — [`crate::effective`] folds the same ancestor chain
/// [`resolve_chain`] does, one [`ancestor_chain`] step at a time, to
/// attribute each inherited field to the template that supplied it.
pub(crate) fn merge_over(parent: &FieldNode, child: &FieldNode) -> FieldNode {
    if is_inherit_false(child) {
        return child.clone();
    }

    // Replace, not union — see this module's `resolve` doc comment.
    let attrs = child.attrs.clone();

    let content = match (&parent.content, &child.content) {
        // Not an override at all — the child contributes nothing, so the
        // parent's content stands untouched.
        (_, Content::Empty) => parent.content.clone(),
        // A leaf child fully specifies its own content — there is
        // nothing on a leaf to merge into, so it replaces outright.
        (_, Content::Text(_)) => child.content.clone(),
        (Content::Children(parent_children), Content::Children(child_children)) => {
            Content::Children(merge_children(parent_children, child_children))
        }
        // The parent had a leaf (or nothing); the child introduces
        // structure. There's no parent list to merge into, so the
        // child's own children stand as given.
        (_, Content::Children(child_children)) => Content::Children(child_children.clone()),
    };

    FieldNode {
        tag: child.tag.clone(),
        attrs,
        content,
    }
}

/// Folds `child_children` onto `parent_children` the same way `resolve`
/// folds one `ParentName` step: a named element already present merges
/// recursively (via [`merge_over`]), a new one is appended, and a `li`
/// item is always appended (siblings are told apart by
/// `crate::tree::ItemIdentity`, never by this fold). `pub(crate)` so
/// [`crate::plan::build_resolved_node`] can reuse the exact same fold to
/// combine several fields' own single-field fragments into one tree,
/// instead of a second tree-assembly implementation.
pub(crate) fn merge_children(
    parent_children: &[FieldNode],
    child_children: &[FieldNode],
) -> Vec<FieldNode> {
    let mut result = parent_children.to_vec();
    for child in child_children {
        if child.tag == "li" {
            result.push(child.clone());
            continue;
        }
        match result.iter().position(|node| node.tag == child.tag) {
            Some(position) => {
                let merged = merge_over(&result[position], child);
                result[position] = merged;
            }
            None => result.push(child.clone()),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use proptest::prelude::*;

    use super::*;
    use crate::xml;

    fn template_set(entries: Vec<(&str, &str, &str)>) -> TemplateSet {
        let mut by_name = BTreeMap::new();
        for (def_type, name, xml_text) in entries {
            let tree = xml::parse(xml_text).unwrap();
            by_name.insert((def_type.to_string(), name.to_string()), tree);
        }
        TemplateSet::new(by_name)
    }

    #[test]
    fn a_tree_with_no_parent_resolves_to_itself_and_is_idempotent() {
        let raw = xml::parse("<HediffDef><label>x</label></HediffDef>").unwrap();
        let templates = TemplateSet::default();

        let resolved = resolve(&raw, &templates).unwrap();
        assert_eq!(resolved.root, raw.root);

        let resolved_again = resolve(&resolved, &templates).unwrap();
        assert_eq!(resolved_again, resolved);
    }

    #[test]
    fn a_leaf_override_wins_over_the_parent_leaf() {
        let templates = template_set(vec![(
            "HediffDef",
            "Base",
            "<HediffDef Name=\"Base\"><label>base label</label></HediffDef>",
        )]);
        let raw =
            xml::parse(r#"<HediffDef ParentName="Base"><label>child label</label></HediffDef>"#)
                .unwrap();

        let resolved = resolve(&raw, &templates).unwrap();
        let label = resolved.get(&"label".parse().unwrap()).unwrap();
        assert_eq!(label.content, Content::Text("child label".to_string()));
    }

    #[test]
    fn child_li_items_append_after_the_parents_own_items() {
        let templates = template_set(vec![(
            "HediffDef",
            "Base",
            r#"<HediffDef Name="Base"><comps><li Class="A"/><li Class="B"/></comps></HediffDef>"#,
        )]);
        let raw = xml::parse(
            r#"<HediffDef ParentName="Base"><comps><li Class="C"/></comps></HediffDef>"#,
        )
        .unwrap();

        let resolved = resolve(&raw, &templates).unwrap();
        let comps = resolved.get(&"comps".parse().unwrap()).unwrap();
        let Content::Children(items) = &comps.content else {
            unreachable!("comps is always a list of li items")
        };
        let classes: Vec<&str> = items
            .iter()
            .map(|item| item.attrs.get("Class").map(String::as_str).unwrap())
            .collect();
        assert_eq!(classes, vec!["A", "B", "C"]);
    }

    #[test]
    fn inherit_false_replaces_the_parent_element_wholesale() {
        let templates = template_set(vec![(
            "ThingDef",
            "Base",
            r#"<ThingDef Name="Base"><graphicData><texPath>Old</texPath></graphicData></ThingDef>"#,
        )]);
        let raw = xml::parse(r#"<ThingDef ParentName="Base"><graphicData Inherit="False"><texPath>New</texPath></graphicData></ThingDef>"#)
        .unwrap();

        let resolved = resolve(&raw, &templates).unwrap();
        let graphic_data = resolved.get(&"graphicData".parse().unwrap()).unwrap();
        assert!(!graphic_data.attrs.contains_key("Inherit"));
        let tex_path = resolved
            .get(&"graphicData/texPath".parse().unwrap())
            .unwrap();
        assert_eq!(tex_path.content, Content::Text("New".to_string()));
    }

    #[test]
    fn an_empty_child_element_is_not_an_override_and_keeps_the_parents_content() {
        let templates = template_set(vec![(
            "ThingDef",
            "Base",
            r#"<ThingDef Name="Base"><graphicData><texPath>Old</texPath></graphicData></ThingDef>"#,
        )]);
        let raw = xml::parse(r#"<ThingDef ParentName="Base"><graphicData/></ThingDef>"#).unwrap();

        let resolved = resolve(&raw, &templates).unwrap();
        let tex_path = resolved
            .get(&"graphicData/texPath".parse().unwrap())
            .unwrap();
        assert_eq!(tex_path.content, Content::Text("Old".to_string()));
    }

    #[test]
    fn a6_child_attributes_replace_the_parents_wholesale_not_a_union() {
        let templates = template_set(vec![(
            "ThingDef",
            "Base",
            r#"<ThingDef Name="Base"><statBases Class="ParentClass" Foo="1"><x>1</x></statBases></ThingDef>"#,
        )]);
        // The child re-declares `statBases` with no attributes at all —
        // the result has none either, not a union keeping `Foo`.
        let raw =
            xml::parse(r#"<ThingDef ParentName="Base"><statBases><y>2</y></statBases></ThingDef>"#)
                .unwrap();

        let resolved = resolve(&raw, &templates).unwrap();
        let stat_bases = resolved.get(&"statBases".parse().unwrap()).unwrap();
        assert!(stat_bases.attrs.is_empty());
    }

    #[test]
    fn inherit_is_stripped_recursively_even_from_a_wholesale_appended_subtree() {
        // `comps` has no parent counterpart, so the child's whole subtree
        // is appended verbatim (never passing through the `Inherit="False"`
        // wholesale-replace branch) — its own nested `Inherit="False"`
        // must still not survive into the resolved tree.
        let templates = template_set(vec![(
            "ThingDef",
            "Base",
            r#"<ThingDef Name="Base"><label>base</label></ThingDef>"#,
        )]);
        let raw = xml::parse(r#"<ThingDef ParentName="Base"><comps><nested Inherit="False"><x>1</x></nested></comps></ThingDef>"#)
        .unwrap();

        let resolved = resolve(&raw, &templates).unwrap();
        let nested = resolved.get(&"comps/nested".parse().unwrap()).unwrap();
        assert!(!nested.attrs.contains_key("Inherit"));
    }

    #[test]
    fn resolve_is_idempotent_over_a_real_parent_chain() {
        let templates = template_set(vec![(
            "HediffDef",
            "Base",
            r#"<HediffDef Name="Base"><label>base</label></HediffDef>"#,
        )]);
        let raw =
            xml::parse(r#"<HediffDef ParentName="Base"><label>child</label></HediffDef>"#).unwrap();

        let once = resolve(&raw, &templates).unwrap();
        // `once` no longer has a `parent_name`, so resolving it again
        // must be a genuine no-op.
        let twice = resolve(&once, &templates).unwrap();
        assert_eq!(once, twice);
    }

    #[test]
    fn resolve_inherited_only_is_none_without_a_parent_and_excludes_raws_own_fields() {
        let templates = TemplateSet::default();
        let no_parent = xml::parse("<HediffDef><label>x</label></HediffDef>").unwrap();
        assert!(
            resolve_inherited_only(&no_parent, &templates)
                .unwrap()
                .is_none()
        );

        let templates = template_set(vec![(
            "HediffDef",
            "Base",
            r#"<HediffDef Name="Base"><label>from base</label></HediffDef>"#,
        )]);
        let raw = xml::parse(
            r#"<HediffDef ParentName="Base"><label>from raw</label><own>1</own></HediffDef>"#,
        )
        .unwrap();

        let inherited = resolve_inherited_only(&raw, &templates).unwrap().unwrap();
        let label = inherited.get(&"label".parse().unwrap()).unwrap();
        assert_eq!(label.content, Content::Text("from base".to_string()));
        assert!(inherited.get(&"own".parse().unwrap()).is_none());
    }

    #[test]
    fn missing_parent_is_an_error_not_a_silent_skip() {
        let raw =
            xml::parse(r#"<HediffDef ParentName="Nope"><label>x</label></HediffDef>"#).unwrap();
        let templates = TemplateSet::default();

        let error = resolve(&raw, &templates).unwrap_err();
        assert_eq!(
            error,
            InheritError::MissingParent {
                def_type: "HediffDef".to_string(),
                name: "Nope".to_string()
            }
        );
    }

    #[test]
    fn a_self_referencing_cycle_is_reported() {
        let templates = template_set(vec![(
            "HediffDef",
            "Cyclic",
            r#"<HediffDef Name="Cyclic" ParentName="Cyclic"><label>x</label></HediffDef>"#,
        )]);
        let raw =
            xml::parse(r#"<HediffDef ParentName="Cyclic"><label>y</label></HediffDef>"#).unwrap();

        let error = resolve(&raw, &templates).unwrap_err();
        assert!(
            matches!(error, InheritError::Cycle { chain } if chain == vec!["Cyclic".to_string(), "Cyclic".to_string()])
        );
    }

    #[test]
    fn the_bionic_heart_chain_resolves_per_the_worked_example() {
        // The worked example: Example Bionics Fork's BionicHeart, resolved
        // through addedPartExampleSynth -> AddedBodyPartBase ->
        // ImplantHediffBase.
        let templates = template_set(vec![
            (
                "HediffDef",
                "ImplantHediffBase",
                r#"<HediffDef Name="ImplantHediffBase" Abstract="True">
                     <hediffClass>Hediff_Implant</hediffClass>
                     <defaultLabelColor>(0.6, 0.6, 1.0)</defaultLabelColor>
                     <isBad>false</isBad>
                     <priceImpact>true</priceImpact>
                     <countsAsAddedPartOrImplant>true</countsAsAddedPartOrImplant>
                     <allowMothballIfLowPriorityWorldPawn>true</allowMothballIfLowPriorityWorldPawn>
                   </HediffDef>"#,
            ),
            (
                "HediffDef",
                "AddedBodyPartBase",
                r#"<HediffDef Name="AddedBodyPartBase" ParentName="ImplantHediffBase" Abstract="True">
                     <hediffClass>Hediff_AddedPart</hediffClass>
                     <priceImpact>true</priceImpact>
                   </HediffDef>"#,
            ),
            (
                "HediffDef",
                "addedPartExampleSynth",
                r#"<HediffDef Name="addedPartExampleSynth" ParentName="AddedBodyPartBase" Abstract="True">
                     <defaultLabelColor>(188,39,242)</defaultLabelColor>
                     <comps>
                       <li MayRequire="example.bodyframework" Class="ExampleBody.Hediffs.HediffCompProperties_MaxHPAdjust">
                         <scaleAdjustment>0.20</scaleAdjustment>
                       </li>
                     </comps>
                   </HediffDef>"#,
            ),
        ]);
        let raw = xml::parse(
            r#"<HediffDef ParentName="addedPartExampleSynth">
                 <defName>BionicHeart</defName>
                 <label>synthetic heart</label>
               </HediffDef>"#,
        )
        .unwrap();

        let resolved = resolve(&raw, &templates).unwrap();

        insta::assert_debug_snapshot!("bionic_heart_bionics_resolved", &resolved);
    }

    // -- proptest laws --------------------------------------------------

    fn arb_leaf() -> impl Strategy<Value = FieldNode> {
        ("[a-z]{1,6}", "[a-zA-Z0-9]{1,6}").prop_map(|(tag, text)| FieldNode {
            tag,
            attrs: BTreeMap::new(),
            content: Content::Text(text),
        })
    }

    proptest! {
        /// A tree with no `ParentName` resolves to itself, and resolving
        /// its own (already-parentless) result again changes nothing.
        #[test]
        fn resolve_is_idempotent_when_there_is_no_parent(leaves in proptest::collection::vec(arb_leaf(), 0..5)) {
            let raw = FieldTree {
                root: FieldNode { tag: "ThingDef".to_string(), attrs: BTreeMap::new(), content: Content::Children(leaves) },
                parent_name: None,
                name: None,
            };
            let templates = TemplateSet::default();
            let once = resolve(&raw, &templates).unwrap();
            let twice = resolve(&once, &templates).unwrap();
            prop_assert_eq!(once, twice);
        }

        /// A child `li` list of length `n` over a parent list of length
        /// `m` resolves to `m + n` items, parent's items first.
        #[test]
        fn li_lists_append_with_parent_items_first(parent_count in 0usize..5, child_count in 0usize..5) {
            fn li_list(prefix: &str, count: usize) -> FieldNode {
                let items = (0..count).map(|index| {
                    let mut attrs = BTreeMap::new();
                    attrs.insert("Class".to_string(), format!("{prefix}{index}"));
                    FieldNode { tag: "li".to_string(), attrs, content: Content::Empty }
                }).collect();
                FieldNode { tag: "items".to_string(), attrs: BTreeMap::new(), content: Content::Children(items) }
            }
            let mut by_name = BTreeMap::new();
            by_name.insert(("ThingDef".to_string(), "Base".to_string()),
                FieldTree { root: FieldNode { tag: "ThingDef".to_string(), attrs: BTreeMap::new(), content: Content::Children(vec![li_list("p", parent_count)]) }, parent_name: None, name: None });
            let templates = TemplateSet::new(by_name);
            let raw = FieldTree {
                root: FieldNode { tag: "ThingDef".to_string(), attrs: BTreeMap::new(), content: Content::Children(vec![li_list("c", child_count)]) },
                parent_name: Some("Base".to_string()),
                name: None,
            };

            let resolved = resolve(&raw, &templates).unwrap();
            let items = resolved.get(&"items".parse().unwrap());
            match items {
                None => prop_assert_eq!(parent_count + child_count, 0),
                Some(node) => {
                    let Content::Children(children) = &node.content else { unreachable!() };
                    prop_assert_eq!(children.len(), parent_count + child_count);
                    for (index, item) in children.iter().take(parent_count).enumerate() {
                        prop_assert_eq!(item.attrs.get("Class").unwrap(), &format!("p{index}"));
                    }
                }
            }
        }
    }
}
