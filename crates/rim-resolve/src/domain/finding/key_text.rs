//! The `FindingKey` text form: `Display`, `FromStr`, and the per-part format/parse helpers.

use std::collections::BTreeSet;
use std::fmt;
use std::str::FromStr;

use rim_analyzer::domain::{EdgeKind, InheritanceProblemKind, ModId, ModReferenceKind, Selector};

use super::key::{DefKey, FindingKey};
use crate::domain::rule::{Placement, RuleOrigin};
use crate::domain::tag::Tag;

/// [`FindingKey`]'s canonical text form failed to parse.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FindingKeyParseError {
    /// The leading `kind` segment wasn't a recognized finding kind.
    #[error(
        "unrecognized finding key kind: {0:?} (if this key predates a renamed finding kind, \
         remove this one decision -- or the whole file, if it's the only entry -- and redo it)"
    )]
    UnknownKind(String),
    /// A recognized kind had the wrong number of `:`-separated fields.
    #[error("finding key {kind:?} expects {expected} fields, found {found}")]
    WrongFieldCount {
        /// The finding kind whose arity didn't match.
        kind: String,
        /// How many fields that kind expects.
        expected: usize,
        /// How many fields were actually present.
        found: usize,
    },
    /// A `[a,b,c]` list field was missing its brackets.
    #[error("malformed set/list field {0:?}: expected [item,item,...]")]
    MalformedList(String),
    /// A `<def_type>/<def_name>` field had no `/`.
    #[error("malformed def key {0:?}: expected <def_type>/<def_name>")]
    MalformedDefKey(String),
    /// The edge kind field wasn't one of [`EdgeKind`]'s variants.
    #[error("unrecognized edge kind {0:?}")]
    UnknownEdgeKind(String),
    /// The selector field wasn't one of [`Selector`]'s variants.
    #[error("unrecognized selector {0:?}")]
    UnknownSelector(String),
    /// The problem-kind field wasn't one of
    /// [`InheritanceProblemKind`]'s variants.
    #[error("unrecognized inheritance problem kind {0:?}")]
    UnknownInheritanceProblemKind(String),
    /// The reference-kind field wasn't one of [`ModReferenceKind`]'s
    /// variants.
    #[error("unrecognized mod reference kind {0:?}")]
    UnknownModReferenceKind(String),
    /// The rule origin field wasn't one of [`RuleOrigin`]'s variants.
    #[error("unrecognized rule origin {0:?}")]
    UnknownRuleOrigin(String),
    /// The placement field wasn't one of [`Placement`]'s variants.
    #[error("unrecognized placement {0:?}")]
    UnknownPlacement(String),
    /// A tag field failed its own validation.
    #[error("invalid field {0:?}")]
    InvalidField(String),
}

const SUB_PATH_NONE_TOKEN: &str = "-";

fn format_ids<'a>(ids: impl IntoIterator<Item = &'a ModId>) -> String {
    let joined = ids
        .into_iter()
        .map(ModId::as_str)
        .collect::<Vec<_>>()
        .join(",");
    format!("[{joined}]")
}

fn edge_kind_str(kind: EdgeKind) -> &'static str {
    match kind {
        EdgeKind::AssemblyRef => "assembly_ref",
        EdgeKind::ForceLoadAfter => "force_load_after",
        EdgeKind::ForceLoadBefore => "force_load_before",
        EdgeKind::LoadAfter => "load_after",
        EdgeKind::LoadBefore => "load_before",
        EdgeKind::ModDependency => "mod_dependency",
        EdgeKind::FindMod => "find_mod",
        EdgeKind::IfModActive => "if_mod_active",
        EdgeKind::PatchTargetsDef => "patch_targets_def",
        EdgeKind::MayRequire => "may_require",
        EdgeKind::PatchInjectedNode => "patch_injected_node",
        EdgeKind::AssemblyVersionPrecedence => "assembly_version_precedence",
        EdgeKind::UsesType => "uses_type",
        EdgeKind::ParentTemplate => "parent_template",
        EdgeKind::PatchRemovedNode => "patch_removed_node",
        EdgeKind::RetextureAfterOwner => "retexture_after_owner",
        EdgeKind::DefOverrideAfterOrigin => "def_override_after_origin",
        EdgeKind::PatchSelectsInjectedNode => "patch_selects_injected_node",
        EdgeKind::PatchInvalidatesPredicate => "patch_invalidates_predicate",
        EdgeKind::PatchRemovedNodeCosmetic => "patch_removed_node_cosmetic",
        EdgeKind::ReplaceDiscardsAddition => "replace_discards_addition",
    }
}

fn parse_edge_kind(text: &str) -> Result<EdgeKind, FindingKeyParseError> {
    match text {
        "assembly_ref" => Ok(EdgeKind::AssemblyRef),
        "force_load_after" => Ok(EdgeKind::ForceLoadAfter),
        "force_load_before" => Ok(EdgeKind::ForceLoadBefore),
        "load_after" => Ok(EdgeKind::LoadAfter),
        "load_before" => Ok(EdgeKind::LoadBefore),
        "mod_dependency" => Ok(EdgeKind::ModDependency),
        "find_mod" => Ok(EdgeKind::FindMod),
        "if_mod_active" => Ok(EdgeKind::IfModActive),
        "patch_targets_def" => Ok(EdgeKind::PatchTargetsDef),
        "may_require" => Ok(EdgeKind::MayRequire),
        "patch_injected_node" => Ok(EdgeKind::PatchInjectedNode),
        "assembly_version_precedence" => Ok(EdgeKind::AssemblyVersionPrecedence),
        "uses_type" => Ok(EdgeKind::UsesType),
        "parent_template" => Ok(EdgeKind::ParentTemplate),
        "patch_removed_node" => Ok(EdgeKind::PatchRemovedNode),
        "retexture_after_owner" => Ok(EdgeKind::RetextureAfterOwner),
        "def_override_after_origin" => Ok(EdgeKind::DefOverrideAfterOrigin),
        "patch_selects_injected_node" => Ok(EdgeKind::PatchSelectsInjectedNode),
        "patch_invalidates_predicate" => Ok(EdgeKind::PatchInvalidatesPredicate),
        "patch_removed_node_cosmetic" => Ok(EdgeKind::PatchRemovedNodeCosmetic),
        "replace_discards_addition" => Ok(EdgeKind::ReplaceDiscardsAddition),
        other => Err(FindingKeyParseError::UnknownEdgeKind(other.to_string())),
    }
}

fn selector_str(selector: Selector) -> &'static str {
    match selector {
        Selector::DefName => "def_name",
        Selector::NameAttr => "name_attr",
    }
}

fn parse_selector(text: &str) -> Result<Selector, FindingKeyParseError> {
    match text {
        "def_name" => Ok(Selector::DefName),
        "name_attr" => Ok(Selector::NameAttr),
        other => Err(FindingKeyParseError::UnknownSelector(other.to_string())),
    }
}

fn inheritance_problem_kind_str(kind: InheritanceProblemKind) -> &'static str {
    match kind {
        InheritanceProblemKind::MissingParent => "missing",
        InheritanceProblemKind::ParentTypeMismatch => "type_mismatch",
    }
}

fn parse_inheritance_problem_kind(
    text: &str,
) -> Result<InheritanceProblemKind, FindingKeyParseError> {
    match text {
        "missing" => Ok(InheritanceProblemKind::MissingParent),
        "type_mismatch" => Ok(InheritanceProblemKind::ParentTypeMismatch),
        other => Err(FindingKeyParseError::UnknownInheritanceProblemKind(
            other.to_string(),
        )),
    }
}

fn mod_reference_kind_str(kind: ModReferenceKind) -> &'static str {
    match kind {
        ModReferenceKind::FindModName => "findmod",
        ModReferenceKind::MayRequireId => "mayrequire",
    }
}

fn parse_mod_reference_kind(text: &str) -> Result<ModReferenceKind, FindingKeyParseError> {
    match text {
        "findmod" => Ok(ModReferenceKind::FindModName),
        "mayrequire" => Ok(ModReferenceKind::MayRequireId),
        other => Err(FindingKeyParseError::UnknownModReferenceKind(
            other.to_string(),
        )),
    }
}

fn rule_origin_str(origin: RuleOrigin) -> &'static str {
    match origin {
        RuleOrigin::UserDecision => "user_decision",
        RuleOrigin::RimSortUser => "rim_sort_user",
        RuleOrigin::RimSortCommunity => "rim_sort_community",
        RuleOrigin::SteamDb => "steam_db",
    }
}

fn parse_rule_origin(text: &str) -> Result<RuleOrigin, FindingKeyParseError> {
    match text {
        "user_decision" => Ok(RuleOrigin::UserDecision),
        "rim_sort_user" => Ok(RuleOrigin::RimSortUser),
        "rim_sort_community" => Ok(RuleOrigin::RimSortCommunity),
        "steam_db" => Ok(RuleOrigin::SteamDb),
        other => Err(FindingKeyParseError::UnknownRuleOrigin(other.to_string())),
    }
}

fn placement_str(placement: Placement) -> &'static str {
    match placement {
        Placement::Top => "top",
        Placement::Bottom => "bottom",
    }
}

fn parse_placement(text: &str) -> Result<Placement, FindingKeyParseError> {
    match text {
        "top" => Ok(Placement::Top),
        "bottom" => Ok(Placement::Bottom),
        other => Err(FindingKeyParseError::UnknownPlacement(other.to_string())),
    }
}

fn parse_ids(field: &str) -> Result<BTreeSet<ModId>, FindingKeyParseError> {
    let inner = field
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .ok_or_else(|| FindingKeyParseError::MalformedList(field.to_string()))?;
    if inner.is_empty() {
        return Ok(BTreeSet::new());
    }
    Ok(inner.split(',').map(ModId::new).collect())
}

/// Parses a `[a,b]` field into an order-preserving pair, without the
/// dedup/reorder a [`BTreeSet`] would apply — `LikelyDuplicateMod` and
/// `IncompatiblePair` render whatever order their tuple holds.
fn parse_pair(field: &str) -> Result<(ModId, ModId), FindingKeyParseError> {
    let inner = field
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .ok_or_else(|| FindingKeyParseError::MalformedList(field.to_string()))?;
    let mut parts = inner.split(',');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(a), Some(b), None) if !a.is_empty() && !b.is_empty() => {
            Ok((ModId::new(a), ModId::new(b)))
        }
        _ => Err(FindingKeyParseError::MalformedList(field.to_string())),
    }
}

fn parse_def_key(field: &str) -> Result<DefKey, FindingKeyParseError> {
    field
        .split_once('/')
        .map(|(def_type, def_name)| DefKey {
            def_type: def_type.to_string(),
            def_name: def_name.to_string(),
        })
        .ok_or_else(|| FindingKeyParseError::MalformedDefKey(field.to_string()))
}

fn format_sub_path(sub_path: Option<&str>) -> &str {
    sub_path.unwrap_or(SUB_PATH_NONE_TOKEN)
}

fn parse_sub_path(field: &str) -> Option<String> {
    if field == SUB_PATH_NONE_TOKEN {
        None
    } else {
        Some(field.to_string())
    }
}

/// Splits `fields` into exactly `N` elements, or a
/// [`FindingKeyParseError::WrongFieldCount`] naming `kind`.
fn take_n<const N: usize>(
    kind: &str,
    fields: &[&str],
) -> Result<[String; N], FindingKeyParseError> {
    <[&str; N]>::try_from(fields)
        .map(|array| array.map(str::to_string))
        .map_err(|_| FindingKeyParseError::WrongFieldCount {
            kind: kind.to_string(),
            expected: N,
            found: fields.len(),
        })
}

impl fmt::Display for FindingKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EdgeDropped {
                after,
                before,
                kind,
            } => {
                write!(f, "edge_dropped:{after}:{before}:{}", edge_kind_str(*kind))
            }
            Self::DeclarationQuestioned {
                declared_after,
                declared_before,
                relation_kind,
            } => write!(
                f,
                "declaration_questioned:{declared_after}:{declared_before}:{}",
                edge_kind_str(*relation_kind)
            ),
            Self::DeclarationOverridden {
                declared_after,
                declared_before,
                kind,
            } => write!(
                f,
                "declaration_overridden:{declared_after}:{declared_before}:{}",
                edge_kind_str(*kind)
            ),
            Self::AnyOfChoice { after, assembly } => {
                write!(f, "any_of_choice:{after}:{assembly}")
            }
            Self::DefOverride { key, owners } => {
                write!(f, "def_override:{key}:{}", format_ids(owners))
            }
            Self::PatchCollision {
                key,
                selector,
                sub_path,
                mods,
            } => write!(
                f,
                "patch_collision:{key}:{}:{}:{}",
                selector_str(*selector),
                format_sub_path(sub_path.as_deref()),
                format_ids(mods)
            ),
            Self::TextureOverride {
                texture_path,
                owners,
            } => {
                write!(f, "texture_override:{texture_path}:{}", format_ids(owners))
            }
            Self::DuplicateAssembly {
                assembly_name,
                owners,
            } => write!(
                f,
                "duplicate_assembly:{assembly_name}:{}",
                format_ids(owners)
            ),
            Self::DuplicateTemplateName { name, owners } => {
                write!(f, "duplicate_template_name:{name}:{}", format_ids(owners))
            }
            Self::KeyedTranslationCollision { pair } => {
                write!(f, "keyed_translation_collision:[{},{}]", pair.0, pair.1)
            }
            Self::SoundOverride { path, owners } => {
                write!(f, "sound_override:{path}:{}", format_ids(owners))
            }
            Self::UndeclaredTypeDependency {
                user,
                provider,
                type_name,
            } => write!(
                f,
                "undeclared_type_dependency:{user}:{provider}:{type_name}"
            ),
            Self::RuntimePatchCollision {
                target_type,
                target_method,
                owners,
            } => write!(
                f,
                "runtime_patch_collision:{target_type}:{target_method}:{}",
                format_ids(owners)
            ),
            Self::TranspilerCollision {
                target_type,
                target_method,
                owners,
            } => write!(
                f,
                "transpiler_collision:{target_type}:{target_method}:{}",
                format_ids(owners)
            ),
            Self::RuleOverruled {
                after,
                before,
                origin,
            } => write!(
                f,
                "rule_overruled:{after}:{before}:{}",
                rule_origin_str(*origin)
            ),
            Self::PlacementOverruled {
                mod_id,
                placement,
                origin,
            } => write!(
                f,
                "placement_overruled:{mod_id}:{}:{}",
                placement_str(*placement),
                rule_origin_str(*origin)
            ),
            Self::PlacementQuestioned {
                mod_id,
                placement,
                relation,
            } => write!(
                f,
                "placement_questioned:{mod_id}:{}:{}",
                placement_str(*placement),
                edge_kind_str(*relation)
            ),
            Self::PlacementOrderingOverridden {
                mod_id,
                pinned,
                placement,
            } => write!(
                f,
                "placement_ordering_overridden:{mod_id}:{pinned}:{}",
                placement_str(*placement)
            ),
            Self::PlacementPromotesDependents { mod_id, placement } => write!(
                f,
                "placement_promotes_dependents:{mod_id}:{}",
                placement_str(*placement)
            ),
            Self::LikelyDuplicateMod { pair } => {
                write!(f, "likely_duplicate_mod:[{},{}]", pair.0, pair.1)
            }
            Self::MissingMod { mod_id } => write!(f, "missing_mod:{mod_id}"),
            Self::MissingDependency { mod_id, dependency } => {
                write!(f, "missing_dependency:{mod_id}:{dependency}")
            }
            Self::IncompatiblePair { pair } => {
                write!(f, "incompatible_pair:[{},{}]", pair.0, pair.1)
            }
            Self::UnsupportedVersion { mod_id } => write!(f, "unsupported_version:{mod_id}"),
            Self::UndeclaredHardDependency { after, before } => {
                write!(f, "undeclared_hard_dependency:{after}:{before}")
            }
            Self::LazyReferenceViolated { after, before } => {
                write!(f, "lazy_reference_violated:{after}:{before}")
            }
            Self::TagInferred { mod_id, tag } => write!(f, "tag_inferred:{mod_id}:{tag}"),
            Self::MissingTexturePath {
                referrer,
                def,
                field,
                path,
            } => {
                write!(f, "missing_texture_path:{referrer}:{def}:{field}:{path}")
            }
            Self::PatchWillFail {
                mod_id,
                def_key,
                selector,
                operation,
            } => write!(
                f,
                "patch_will_fail:{mod_id}:{def_key}:{}:{operation}",
                selector_str(*selector)
            ),
            Self::ContributesNothing { mod_id } => write!(f, "contributes_nothing:{mod_id}"),
            Self::UndecodableTexture { mod_id, path } => {
                write!(f, "undecodable_texture:{mod_id}:{path}")
            }
            Self::BrokenInheritance {
                mod_id,
                parent_name,
                problem_kind,
            } => write!(
                f,
                "broken_inheritance:{mod_id}:{}:{parent_name}",
                inheritance_problem_kind_str(*problem_kind)
            ),
            Self::NearMissModReference {
                referrer,
                kind,
                written,
            } => write!(
                f,
                "near_miss_mod_reference:{referrer}:{}:{written}",
                mod_reference_kind_str(*kind)
            ),
            Self::DiscardedAddition {
                replacer,
                adder,
                def,
                path,
            } => write!(f, "discarded_addition:{replacer}:{adder}:{def}:{path}"),
            Self::DanglingDefReference { name } => write!(f, "dangling_def_reference:{name}"),
        }
    }
}

impl FromStr for FindingKey {
    type Err = FindingKeyParseError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let mut fields = text.split(':');
        let kind = fields
            .next()
            .ok_or_else(|| FindingKeyParseError::UnknownKind(text.to_string()))?;
        let rest: Vec<&str> = fields.collect();

        match kind {
            "edge_dropped" => {
                let [after, before, edge_kind] = take_n::<3>(kind, &rest)?;
                Ok(Self::EdgeDropped {
                    after: ModId::new(after),
                    before: ModId::new(before),
                    kind: parse_edge_kind(&edge_kind)?,
                })
            }
            "declaration_questioned" => {
                let [declared_after, declared_before, relation_kind] = take_n::<3>(kind, &rest)?;
                Ok(Self::DeclarationQuestioned {
                    declared_after: ModId::new(declared_after),
                    declared_before: ModId::new(declared_before),
                    relation_kind: parse_edge_kind(&relation_kind)?,
                })
            }
            "declaration_overridden" => {
                let [declared_after, declared_before, edge_kind] = take_n::<3>(kind, &rest)?;
                Ok(Self::DeclarationOverridden {
                    declared_after: ModId::new(declared_after),
                    declared_before: ModId::new(declared_before),
                    kind: parse_edge_kind(&edge_kind)?,
                })
            }
            "any_of_choice" => {
                let [after, assembly] = take_n::<2>(kind, &rest)?;
                Ok(Self::AnyOfChoice {
                    after: ModId::new(after),
                    assembly,
                })
            }
            "def_override" => {
                let [key, owners] = take_n::<2>(kind, &rest)?;
                Ok(Self::DefOverride {
                    key: parse_def_key(&key)?,
                    owners: parse_ids(&owners)?,
                })
            }
            "patch_collision" => {
                let [key, selector, sub_path, mods] = take_n::<4>(kind, &rest)?;
                Ok(Self::PatchCollision {
                    key: parse_def_key(&key)?,
                    selector: parse_selector(&selector)?,
                    sub_path: parse_sub_path(&sub_path),
                    mods: parse_ids(&mods)?,
                })
            }
            "texture_override" => {
                let [texture_path, owners] = take_n::<2>(kind, &rest)?;
                Ok(Self::TextureOverride {
                    texture_path,
                    owners: parse_ids(&owners)?,
                })
            }
            "duplicate_assembly" => {
                let [assembly_name, owners] = take_n::<2>(kind, &rest)?;
                Ok(Self::DuplicateAssembly {
                    assembly_name,
                    owners: parse_ids(&owners)?,
                })
            }
            "duplicate_template_name" => {
                let [name, owners] = take_n::<2>(kind, &rest)?;
                Ok(Self::DuplicateTemplateName {
                    name,
                    owners: parse_ids(&owners)?,
                })
            }
            "keyed_translation_collision" => {
                let [pair] = take_n::<1>(kind, &rest)?;
                Ok(Self::KeyedTranslationCollision {
                    pair: parse_pair(&pair)?,
                })
            }
            "sound_override" => {
                let [path, owners] = take_n::<2>(kind, &rest)?;
                Ok(Self::SoundOverride {
                    path,
                    owners: parse_ids(&owners)?,
                })
            }
            "undeclared_type_dependency" => {
                let [user, provider, type_name] = take_n::<3>(kind, &rest)?;
                Ok(Self::UndeclaredTypeDependency {
                    user: ModId::new(user),
                    provider: ModId::new(provider),
                    type_name,
                })
            }
            "runtime_patch_collision" => {
                let [target_type, target_method, owners] = take_n::<3>(kind, &rest)?;
                Ok(Self::RuntimePatchCollision {
                    target_type,
                    target_method,
                    owners: parse_ids(&owners)?,
                })
            }
            "transpiler_collision" => {
                let [target_type, target_method, owners] = take_n::<3>(kind, &rest)?;
                Ok(Self::TranspilerCollision {
                    target_type,
                    target_method,
                    owners: parse_ids(&owners)?,
                })
            }
            "rule_overruled" => {
                let [after, before, origin] = take_n::<3>(kind, &rest)?;
                Ok(Self::RuleOverruled {
                    after: ModId::new(after),
                    before: ModId::new(before),
                    origin: parse_rule_origin(&origin)?,
                })
            }
            "placement_overruled" => {
                let [mod_id, placement, origin] = take_n::<3>(kind, &rest)?;
                Ok(Self::PlacementOverruled {
                    mod_id: ModId::new(mod_id),
                    placement: parse_placement(&placement)?,
                    origin: parse_rule_origin(&origin)?,
                })
            }
            "placement_questioned" => {
                let [mod_id, placement, relation] = take_n::<3>(kind, &rest)?;
                Ok(Self::PlacementQuestioned {
                    mod_id: ModId::new(mod_id),
                    placement: parse_placement(&placement)?,
                    relation: parse_edge_kind(&relation)?,
                })
            }
            "placement_ordering_overridden" => {
                let [mod_id, pinned, placement] = take_n::<3>(kind, &rest)?;
                Ok(Self::PlacementOrderingOverridden {
                    mod_id: ModId::new(mod_id),
                    pinned: ModId::new(pinned),
                    placement: parse_placement(&placement)?,
                })
            }
            "placement_promotes_dependents" => {
                let [mod_id, placement] = take_n::<2>(kind, &rest)?;
                Ok(Self::PlacementPromotesDependents {
                    mod_id: ModId::new(mod_id),
                    placement: parse_placement(&placement)?,
                })
            }
            "likely_duplicate_mod" => {
                let [pair] = take_n::<1>(kind, &rest)?;
                Ok(Self::LikelyDuplicateMod {
                    pair: parse_pair(&pair)?,
                })
            }
            "missing_mod" => {
                let [mod_id] = take_n::<1>(kind, &rest)?;
                Ok(Self::MissingMod {
                    mod_id: ModId::new(mod_id),
                })
            }
            "missing_dependency" => {
                let [mod_id, dependency] = take_n::<2>(kind, &rest)?;
                Ok(Self::MissingDependency {
                    mod_id: ModId::new(mod_id),
                    dependency: ModId::new(dependency),
                })
            }
            "incompatible_pair" => {
                let [pair] = take_n::<1>(kind, &rest)?;
                Ok(Self::IncompatiblePair {
                    pair: parse_pair(&pair)?,
                })
            }
            "unsupported_version" => {
                let [mod_id] = take_n::<1>(kind, &rest)?;
                Ok(Self::UnsupportedVersion {
                    mod_id: ModId::new(mod_id),
                })
            }
            "undeclared_hard_dependency" => {
                let [after, before] = take_n::<2>(kind, &rest)?;
                Ok(Self::UndeclaredHardDependency {
                    after: ModId::new(after),
                    before: ModId::new(before),
                })
            }
            "lazy_reference_violated" => {
                let [after, before] = take_n::<2>(kind, &rest)?;
                Ok(Self::LazyReferenceViolated {
                    after: ModId::new(after),
                    before: ModId::new(before),
                })
            }
            "tag_inferred" => {
                let [mod_id, tag] = take_n::<2>(kind, &rest)?;
                let tag =
                    Tag::new(tag.clone()).map_err(|_| FindingKeyParseError::InvalidField(tag))?;
                Ok(Self::TagInferred {
                    mod_id: ModId::new(mod_id),
                    tag,
                })
            }
            "missing_texture_path" => {
                let [referrer, def, field, path] = take_n::<4>(kind, &rest)?;
                Ok(Self::MissingTexturePath {
                    referrer: ModId::new(referrer),
                    def: parse_def_key(&def)?,
                    field,
                    path,
                })
            }
            "patch_will_fail" => {
                // `operation` (always the last
                // field) can itself contain a literal `:` — a real
                // `PatchOperationFindMod`'s own mod display names often do
                // (see `FindingKey::PatchWillFail`'s own doc
                // comment). `rest` above already
                // over-split on every one of those embedded colons, so
                // it's re-derived here from the original `text` with a
                // bounded split instead: exactly 5 pieces (the kind tag
                // plus 4 real fields), the last one capturing everything
                // remaining, embedded colons included.
                let bounded: Vec<&str> = text.splitn(5, ':').skip(1).collect();
                let [mod_id, def_key, selector, operation] = take_n::<4>(kind, &bounded)?;
                Ok(Self::PatchWillFail {
                    mod_id: ModId::new(mod_id),
                    def_key: parse_def_key(&def_key)?,
                    selector: parse_selector(&selector)?,
                    operation,
                })
            }
            "contributes_nothing" => {
                let [mod_id] = take_n::<1>(kind, &rest)?;
                Ok(Self::ContributesNothing {
                    mod_id: ModId::new(mod_id),
                })
            }
            "undecodable_texture" => {
                let [mod_id, path] = take_n::<2>(kind, &rest)?;
                Ok(Self::UndecodableTexture {
                    mod_id: ModId::new(mod_id),
                    path,
                })
            }
            "broken_inheritance" => {
                // `parent_name` (always the last field) is a `Name`
                // attribute value — vanishingly unlikely to carry a
                // literal `:`, but re-split from the original `text` with
                // a bounded count anyway, the same defensive shape
                // `patch_will_fail` uses for its own free-text tail.
                let bounded: Vec<&str> = text.splitn(4, ':').skip(1).collect();
                let [mod_id, problem_kind, parent_name] = take_n::<3>(kind, &bounded)?;
                Ok(Self::BrokenInheritance {
                    mod_id: ModId::new(mod_id),
                    parent_name,
                    problem_kind: parse_inheritance_problem_kind(&problem_kind)?,
                })
            }
            "near_miss_mod_reference" => {
                // `written` (always the last field) is a `FindMod`
                // display name or a `MayRequire` package id — a real
                // display name commonly carries a literal `:`
                // (`PatchWillFail`'s own doc comment gives examples), so
                // this re-splits from the original `text` with a bounded
                // count, exactly like that variant does.
                let bounded: Vec<&str> = text.splitn(4, ':').skip(1).collect();
                let [referrer, kind_text, written] = take_n::<3>(kind, &bounded)?;
                Ok(Self::NearMissModReference {
                    referrer: ModId::new(referrer),
                    kind: parse_mod_reference_kind(&kind_text)?,
                    written,
                })
            }
            "discarded_addition" => {
                // `path` (always the last field) is an XML sub_path's own
                // display text, which can in principle carry a `:` inside a
                // predicate value — bounded-split from the original `text`
                // the same defensive way `near_miss_mod_reference` does for
                // its own free-text tail.
                let bounded: Vec<&str> = text.splitn(5, ':').skip(1).collect();
                let [replacer, adder, def, path] = take_n::<4>(kind, &bounded)?;
                Ok(Self::DiscardedAddition {
                    replacer: ModId::new(replacer),
                    adder: ModId::new(adder),
                    def: Box::new(parse_def_key(&def)?),
                    path,
                })
            }
            "dangling_def_reference" => {
                // `name` (the only field) is a bare `defName` — bounded-split
                // from the original `text` the same defensive way every
                // other single-free-text-field kind above does, in case a
                // real `defName` somehow carries a literal `:`.
                let bounded: Vec<&str> = text.splitn(2, ':').skip(1).collect();
                let [name] = take_n::<1>(kind, &bounded)?;
                Ok(Self::DanglingDefReference { name })
            }
            other => Err(FindingKeyParseError::UnknownKind(other.to_string())),
        }
    }
}
