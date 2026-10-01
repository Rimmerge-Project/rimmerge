//! From an effective def tree to the graphics it declares: which texture
//! paths, read with which `Graphic` class, in which slot. Pure over a
//! [`FieldTree`]; whether those textures exist is [`super::views`]' job.
//!
//! Every rule fires on a field path, never on the def's element tag, so a
//! mod's own def subclass is treated like the vanilla type it extends.

use std::collections::BTreeSet;

use rim_analyzer::extract::graphics::{FlipPolicy, GraphicClass, StuffAppearance};
use rim_merge::tree::{Content, FieldNode, FieldTree};

use super::key::TextureKey;
use super::model::{SlotSource, VariantLabel};

/// The game's default `graphicClass` for a pawn kind's body data when the
/// def names none (`PawnKindLifeStage.ResolveReferences`).
const PAWN_KIND_DEFAULT_CLASS: &str = "Graphic_Multi";
/// `RimWorld.ApparelGraphicRecordGetter`: apparel is always a Multi.
const APPAREL_CLASS: &str = "Graphic_Multi";
/// Apparel layers whose worn graphic takes no body-type suffix.
const HEADGEAR_LAYERS: [&str; 2] = ["Overhead", "EyeCover"];
/// `BaseContent.PlaceholderImagePath` / `PlaceholderGearImagePath`.
const PLACEHOLDER_PATHS: [&str; 2] = ["PlaceholderImage", "PlaceholderImage_Gear"];
/// The most probe slots one def may offer.
const MAX_PROBE_CANDIDATES: usize = 64;

/// One def being resolved.
#[derive(Debug, Clone, Copy)]
pub struct DefSubject<'a> {
    /// The def's `defName`.
    pub name: &'a str,
    /// Its effective tree: inheritance merged, patches replayed.
    pub tree: &'a FieldTree,
}

/// Facts from outside the def that its rules need.
#[derive(Debug, Clone, Copy)]
pub struct SlotFacts<'a> {
    /// Active `BodyTypeDef` names in registration order.
    pub body_types: &'a [String],
    /// `ApparelLayerDef` names whose `isUtilityLayer` is set.
    pub utility_layers: &'a BTreeSet<String>,
    /// Every `StuffAppearanceDef`, in `DefDatabase` order.
    pub stuff_appearances: &'a [StuffAppearance],
    /// Pawn kinds that may use this def as their race, by effective tree.
    /// A kind whose effective `race` names another def is ignored.
    pub race_kinds: &'a [DefSubject<'a>],
}

/// What to read for one variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphicSpec {
    /// How the engine reads the path.
    pub class: GraphicClass,
    /// The `graphicClass` text to show, when the def names one.
    pub class_label: Option<String>,
    /// The base texture key.
    pub base: TextureKey,
    /// Whether the engine may mirror a substituted face.
    pub flip: FlipPolicy,
}

impl GraphicSpec {
    fn named(class_name: &str, base: TextureKey, flip: FlipPolicy) -> Self {
        Self {
            class: GraphicClass::classify(class_name),
            class_label: Some(class_name.trim().to_owned()),
            base,
            flip,
        }
    }

    fn probe(field: &str, base: TextureKey) -> Self {
        Self {
            class: GraphicClass::Unmodelled(field.to_owned()),
            class_label: None,
            base,
            flip: FlipPolicy::Allowed,
        }
    }
}

/// One variant to expand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariantSpec {
    /// What distinguishes it.
    pub label: VariantLabel,
    /// What to read.
    pub graphic: GraphicSpec,
}

/// Which variant of a slot is shown first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefaultVariant {
    /// A fixed position (clamped to the slot).
    At(usize),
    /// The first variant a loose file serves, else the first.
    FirstLocated,
}

/// One slot to build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotSpec {
    /// Where it came from.
    pub source: SlotSource,
    /// Its variants; never empty.
    pub variants: Vec<VariantSpec>,
    /// Which one opens.
    pub default: DefaultVariant,
}

/// The slots a def declares, plus whether it is a humanlike race.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotPlan {
    /// Slots in display order: engine rules, relations, icons, probes.
    pub slots: Vec<SlotSpec>,
    /// `race/intelligence` is `Humanlike`: the game composes such a pawn
    /// from body, head and hair at runtime.
    pub is_humanlike_race: bool,
}

/// Plans every slot `subject` declares.
#[must_use]
pub fn plan_slots(subject: &DefSubject<'_>, facts: &SlotFacts<'_>) -> SlotPlan {
    let root = &subject.tree.root;
    let graphic = graphic_slot(root);
    let has_graphic = graphic.is_some();
    let mut slots: Vec<SlotSpec> = Vec::new();
    slots.extend(graphic);
    slots.extend(worn_slot(root, facts));
    slots.extend(pawn_kind_slot(root, SlotSource::LifeStage));
    if !has_graphic {
        slots.extend(race_kind_slots(subject, facts));
    }
    slots.extend(style_slot(root));
    slots.extend(simple_slot(
        root,
        "graphicPath",
        "Graphic_Multi",
        SlotSource::HeadType,
    ));
    slots.extend(simple_slot(
        root,
        "bodyNakedGraphicPath",
        "Graphic_Multi",
        SlotSource::BodyType,
    ));
    slots.extend(simple_slot(
        root,
        "texturePath",
        "Graphic_Terrain",
        SlotSource::Terrain,
    ));
    slots.extend(icon_slot(root));
    let mut taken: BTreeSet<TextureKey> = slots
        .iter()
        .flat_map(|slot| {
            slot.variants
                .iter()
                .map(|variant| variant.graphic.base.clone())
        })
        .collect();
    // A `graphicData` that names no class shows nothing in the game, so its
    // `texPath` must not resurface as a guess.
    taken.extend(
        child(root, "graphicData")
            .and_then(|data| child_text(data, "texPath"))
            .and_then(|path| TextureKey::parse(path).ok()),
    );
    slots.extend(probe_slots(root, &taken));
    SlotPlan {
        slots,
        is_humanlike_race: is_humanlike_race(root),
    }
}

pub(super) fn child<'a>(node: &'a FieldNode, tag: &str) -> Option<&'a FieldNode> {
    let Content::Children(children) = &node.content else {
        return None;
    };
    children.iter().find(|candidate| candidate.tag == tag)
}

fn text(node: &FieldNode) -> Option<&str> {
    match &node.content {
        Content::Text(value) if !value.trim().is_empty() => Some(value.trim()),
        Content::Text(_) | Content::Empty | Content::Children(_) => None,
    }
}

pub(super) fn child_text<'a>(node: &'a FieldNode, tag: &str) -> Option<&'a str> {
    child(node, tag).and_then(text)
}

fn list_items(node: &FieldNode) -> Vec<&FieldNode> {
    match &node.content {
        Content::Children(children) => children.iter().filter(|c| c.tag == "li").collect(),
        Content::Text(_) | Content::Empty => Vec::new(),
    }
}

fn is_true(value: Option<&str>) -> bool {
    value.is_some_and(|value| value.eq_ignore_ascii_case("true"))
}

fn flip_policy(data: &FieldNode) -> FlipPolicy {
    match child_text(data, "allowFlip") {
        Some(value) if value.eq_ignore_ascii_case("false") => FlipPolicy::Forbidden,
        Some(_) | None => FlipPolicy::Allowed,
    }
}

/// Reads one `GraphicData` node. `default_class` is the game's fallback
/// for a node that names none; without one, a node with no class shows
/// nothing (`GraphicData.Init`).
fn graphic_spec(data: &FieldNode, default_class: Option<&str>) -> Option<GraphicSpec> {
    let class = child_text(data, "graphicClass").or(default_class)?;
    let base = TextureKey::parse(child_text(data, "texPath")?).ok()?;
    Some(GraphicSpec::named(class, base, flip_policy(data)))
}

fn only(spec: GraphicSpec) -> VariantSpec {
    VariantSpec {
        label: VariantLabel::Only,
        graphic: spec,
    }
}

fn graphic_slot(root: &FieldNode) -> Option<SlotSpec> {
    let spec = graphic_spec(child(root, "graphicData")?, None)?;
    Some(SlotSpec {
        source: SlotSource::Graphic,
        variants: vec![only(spec)],
        default: DefaultVariant::At(0),
    })
}

/// `texturePath`-style single field: the path read with a fixed class.
fn simple_slot(
    root: &FieldNode,
    field: &str,
    class_name: &str,
    source: SlotSource,
) -> Option<SlotSpec> {
    let base = TextureKey::parse(child_text(root, field)?).ok()?;
    Some(SlotSpec {
        source,
        variants: vec![only(GraphicSpec::named(
            class_name,
            base,
            FlipPolicy::Allowed,
        ))],
        default: DefaultVariant::At(0),
    })
}

/// A hair, beard or tattoo: a top-level `texPath` on a def without
/// `graphicData`, unless `noGraphic` is set.
fn style_slot(root: &FieldNode) -> Option<SlotSpec> {
    if child(root, "graphicData").is_some() || is_true(child_text(root, "noGraphic")) {
        return None;
    }
    simple_slot(root, "texPath", "Graphic_Multi", SlotSource::StyleTexture)
}

fn icon_slot(root: &FieldNode) -> Option<SlotSpec> {
    let variants: Vec<VariantSpec> = ["uiIconPath", "iconPath"]
        .into_iter()
        .filter_map(|field| {
            let base = TextureKey::parse(child_text(root, field)?).ok()?;
            Some(VariantSpec {
                label: VariantLabel::Field(field.to_owned()),
                graphic: GraphicSpec::named("Graphic_Single", base, FlipPolicy::Allowed),
            })
        })
        .collect();
    (!variants.is_empty()).then_some(SlotSpec {
        source: SlotSource::Icon,
        variants,
        default: DefaultVariant::At(0),
    })
}

/// Whether the last of `apparel/layers` takes no body-type suffix.
fn is_worn_path_unsuffixed(apparel: &FieldNode, path: &str, facts: &SlotFacts<'_>) -> bool {
    if PLACEHOLDER_PATHS.contains(&path) {
        return true;
    }
    let last_layer = child(apparel, "layers")
        .and_then(|layers| list_items(layers).into_iter().rev().find_map(text));
    let Some(last_layer) = last_layer else {
        return false;
    };
    if HEADGEAR_LAYERS.contains(&last_layer) {
        return true;
    }
    let renders_as_pack = match child(apparel, "wornGraphicData") {
        Some(data) => is_true(child_text(data, "renderUtilityAsPack")),
        None => true,
    };
    facts.utility_layers.contains(last_layer) && renders_as_pack
}

fn worn_slot(root: &FieldNode, facts: &SlotFacts<'_>) -> Option<SlotSpec> {
    let apparel = child(root, "apparel")?;
    let mut paths: Vec<&str> = child(apparel, "wornGraphicPaths")
        .map(|list| list_items(list).into_iter().filter_map(text).collect())
        .unwrap_or_default();
    if paths.is_empty() {
        paths.extend(child_text(apparel, "wornGraphicPath"));
    }
    let count = paths.len();
    let mut variants = Vec::new();
    for (index, path) in paths.iter().enumerate() {
        if is_worn_path_unsuffixed(apparel, path, facts) {
            let label = if count == 1 {
                VariantLabel::Only
            } else {
                VariantLabel::WornPath {
                    index,
                    count,
                    body_type: None,
                }
            };
            variants.extend(worn_variant(path, label));
            continue;
        }
        for body_type in facts.body_types {
            let label = if count == 1 {
                VariantLabel::BodyType(body_type.clone())
            } else {
                VariantLabel::WornPath {
                    index,
                    count,
                    body_type: Some(body_type.clone()),
                }
            };
            variants.extend(worn_variant(&format!("{path}_{body_type}"), label));
        }
    }
    (!variants.is_empty()).then_some(SlotSpec {
        source: SlotSource::Worn,
        variants,
        default: DefaultVariant::FirstLocated,
    })
}

fn worn_variant(path: &str, label: VariantLabel) -> Option<VariantSpec> {
    let base = TextureKey::parse(path).ok()?;
    Some(VariantSpec {
        label,
        graphic: GraphicSpec::named(APPAREL_CLASS, base, FlipPolicy::Allowed),
    })
}

/// A pawn kind's life stages: each stage's body (and female) graphic, then
/// the alternates. The game shows the last stage when the animal is grown.
fn pawn_kind_slot(root: &FieldNode, source: SlotSource) -> Option<SlotSpec> {
    let stages = list_items(child(root, "lifeStages")?);
    let count = stages.len();
    let mut bodies = Vec::new();
    let mut females = Vec::new();
    for (index, stage) in stages.iter().enumerate() {
        let body = child(stage, "bodyGraphicData")
            .and_then(|data| graphic_spec(data, Some(PAWN_KIND_DEFAULT_CLASS)));
        let female = child(stage, "femaleGraphicData")
            .and_then(|data| graphic_spec(data, Some(PAWN_KIND_DEFAULT_CLASS)));
        bodies.extend(body.map(|graphic| VariantSpec {
            label: VariantLabel::LifeStage { index, count },
            graphic,
        }));
        females.extend(female.map(|graphic| VariantSpec {
            label: VariantLabel::Female { stage: index },
            graphic,
        }));
    }
    let default = DefaultVariant::At(bodies.len().saturating_sub(1));
    let alternates = alternate_variants(root, bodies.last());
    let variants: Vec<VariantSpec> = bodies
        .into_iter()
        .chain(females)
        .chain(alternates)
        .collect();
    (!variants.is_empty()).then_some(SlotSpec {
        source,
        variants,
        default,
    })
}

/// `alternateGraphics/li/texPath`: each copies the adult body graphic's
/// class and flip policy (`AlternateGraphic.GetGraphic`).
fn alternate_variants(root: &FieldNode, adult_body: Option<&VariantSpec>) -> Vec<VariantSpec> {
    let Some(alternates) = child(root, "alternateGraphics") else {
        return Vec::new();
    };
    let (class_name, flip) = adult_body.map_or_else(
        || (PAWN_KIND_DEFAULT_CLASS.to_owned(), FlipPolicy::Allowed),
        |adult| {
            (
                adult
                    .graphic
                    .class_label
                    .clone()
                    .unwrap_or_else(|| PAWN_KIND_DEFAULT_CLASS.to_owned()),
                adult.graphic.flip,
            )
        },
    );
    list_items(alternates)
        .into_iter()
        .enumerate()
        .filter_map(|(index, alternate)| {
            let base = TextureKey::parse(child_text(alternate, "texPath")?).ok()?;
            Some(VariantSpec {
                label: VariantLabel::Alternate { index },
                graphic: GraphicSpec::named(&class_name, base, flip),
            })
        })
        .collect()
}

/// A race def with no graphic of its own shows the pawn kinds that use it.
fn race_kind_slots(subject: &DefSubject<'_>, facts: &SlotFacts<'_>) -> Vec<SlotSpec> {
    let is_race = child(&subject.tree.root, "race")
        .is_some_and(|race| matches!(race.content, Content::Children(_)));
    if !is_race {
        return Vec::new();
    }
    facts
        .race_kinds
        .iter()
        .filter(|kind| child_text(&kind.tree.root, "race") == Some(subject.name))
        .filter_map(|kind| {
            pawn_kind_slot(
                &kind.tree.root,
                SlotSource::RaceKind {
                    kind: kind.name.to_owned(),
                },
            )
        })
        .collect()
}

fn is_humanlike_race(root: &FieldNode) -> bool {
    child(root, "race")
        .and_then(|race| child_text(race, "intelligence"))
        .is_some_and(|value| value == "Humanlike")
}

/// Every text leaf that reads like a texture path and was not already
/// claimed by an engine slot, by field path.
fn probe_slots(root: &FieldNode, taken: &BTreeSet<TextureKey>) -> Vec<SlotSpec> {
    let mut candidates: Vec<(String, TextureKey)> = Vec::new();
    collect_probe_candidates(root, "", &mut candidates);
    candidates.sort();
    let mut seen: BTreeSet<TextureKey> = taken.clone();
    candidates
        .into_iter()
        .filter(|(_, key)| seen.insert(key.clone()))
        .take(MAX_PROBE_CANDIDATES)
        .map(|(field, key)| SlotSpec {
            variants: vec![only(GraphicSpec::probe(&field, key))],
            source: SlotSource::Probe { field },
            default: DefaultVariant::At(0),
        })
        .collect()
}

fn collect_probe_candidates(
    node: &FieldNode,
    parent_path: &str,
    out: &mut Vec<(String, TextureKey)>,
) {
    match &node.content {
        Content::Children(children) => {
            for child_node in children {
                let path = if parent_path.is_empty() {
                    child_node.tag.clone()
                } else {
                    format!("{parent_path}/{}", child_node.tag)
                };
                collect_probe_candidates(child_node, &path, out);
            }
        }
        Content::Text(value) => {
            let value = value.trim();
            let looks_like_path = value.contains('/') && !value.contains(char::is_whitespace);
            if looks_like_path && let Ok(key) = TextureKey::parse(value) {
                out.push((parent_path.to_owned(), key));
            }
        }
        Content::Empty => {}
    }
}
