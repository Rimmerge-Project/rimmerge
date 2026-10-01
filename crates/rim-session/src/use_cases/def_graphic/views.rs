//! From planned slots to what the engine would show: expands each spec's
//! `Graphic` class over the texture index and decides, per face, whether
//! the engine finds it. Pure over a [`TextureIndex`] and a load order.

use rim_analyzer::analysis::TextureIndex;
use rim_analyzer::domain::{LoadOrder, ModId};
use rim_analyzer::extract::graphics::{
    AppearanceMember, CollectionKind, CollectionMember, ExpandRequest, FaceKey, GraphicClass,
    GraphicLayout, MultiFaces, StuffAppearance, expand, expand_appearances,
};

use super::key::TextureKey;
use super::model::{
    Availability, Face, Faces, GraphicSet, GraphicSlot, GraphicVariant, SlotSource, VariantLabel,
};
use super::slots::{DefaultVariant, GraphicSpec, SlotSpec, VariantSpec};

/// The most slots one def lists; the rest are counted, not dropped silently.
pub const MAX_SLOTS: usize = 16;

/// What views are resolved against.
#[derive(Debug, Clone, Copy)]
pub struct TextureView<'a> {
    /// Which keys exist and who ships them.
    pub textures: &'a TextureIndex,
    /// The selected order: the last-loaded shipper of a key wins.
    pub order: &'a LoadOrder,
    /// Every `StuffAppearanceDef`, for `Graphic_Appearances`.
    pub stuff_appearances: &'a [StuffAppearance],
}

/// Whether the engine finds `key`, under the selected order.
#[must_use]
pub fn availability(key: &TextureKey, view: &TextureView<'_>) -> Availability {
    let owners = view.textures.loose_owners(key.as_str());
    if let Some(owner) = last_loaded(owners, view.order) {
        return Availability::Loose {
            owner: owner.clone(),
        };
    }
    if view.textures.is_non_loose(key.as_str()) {
        return Availability::BaseGameOrBundle;
    }
    if view.textures.is_core_index_trusted() {
        Availability::NotFound
    } else {
        Availability::Unknown
    }
}

/// The owner loaded last in `order`; an owner missing from the order counts
/// as loaded before every listed one, and ties keep the later shipper.
fn last_loaded<'a>(owners: &'a [ModId], order: &LoadOrder) -> Option<&'a ModId> {
    owners.iter().max_by_key(|owner| order.position(owner))
}

fn face(face_key: &FaceKey, view: &TextureView<'_>) -> Option<Face> {
    let key = TextureKey::parse(&face_key.key).ok()?;
    Some(Face {
        availability: availability(&key, view),
        key,
        is_mirrored: face_key.is_mirrored,
    })
}

fn multi_faces(faces: &MultiFaces, view: &TextureView<'_>) -> Option<Faces> {
    Some(Faces::Multi {
        north: face(&faces.north, view)?,
        east: face(&faces.east, view)?,
        south: face(&faces.south, view)?,
        west: face(&faces.west, view)?,
    })
}

/// A face standing for a graphic that resolves to no file at all: the base
/// key itself, so the viewer can say it was not found.
fn absent(key: &TextureKey, view: &TextureView<'_>) -> Faces {
    Faces::Single(Face {
        availability: availability(key, view),
        key: key.clone(),
        is_mirrored: false,
    })
}

fn member_faces(
    member: &CollectionMember,
    base: &TextureKey,
    view: &TextureView<'_>,
) -> Option<Faces> {
    match member {
        CollectionMember::Single(key) => face(key, view).map(Faces::Single),
        CollectionMember::Multi(faces) => multi_faces(faces, view),
        CollectionMember::Missing(key) => Some(absent(
            &TextureKey::parse(key).unwrap_or_else(|_| base.clone()),
            view,
        )),
    }
}

fn member_name(member: &CollectionMember, base: &TextureKey) -> String {
    let key = match member {
        CollectionMember::Single(face_key) => face_key.key.as_str(),
        CollectionMember::Multi(faces) => faces.south.key.as_str(),
        CollectionMember::Missing(key) => key.as_str(),
    };
    key.strip_prefix(base.as_str())
        .map_or(key, |rest| rest.trim_start_matches('/'))
        .to_owned()
}

/// The variants one spec expands to, and how many members were cut.
struct Expanded {
    variants: Vec<GraphicVariant>,
    truncated: usize,
}

fn expand_spec(spec: &VariantSpec, view: &TextureView<'_>) -> Option<Expanded> {
    let graphic = &spec.graphic;
    let is_open = spec.label == VariantLabel::Only;
    if graphic.class == GraphicClass::Appearances {
        let members =
            expand_appearances(graphic.base.as_str(), view.stuff_appearances, view.textures);
        return Some(appearance_variants(&members, graphic, is_open, view));
    }
    match layout_of(graphic, view) {
        GraphicLayout::Single(key) => one(spec, Faces::Single(face(&key, view)?)),
        GraphicLayout::Multi(faces) => one(spec, multi_faces(&faces, view)?),
        GraphicLayout::Missing => one(spec, absent(&graphic.base, view)),
        GraphicLayout::Collection { members, truncated } => {
            collection_variants(spec, &members, truncated, view)
        }
    }
}

fn one(spec: &VariantSpec, faces: Faces) -> Option<Expanded> {
    Some(Expanded {
        variants: vec![GraphicVariant {
            label: spec.label.clone(),
            faces,
        }],
        truncated: 0,
    })
}

/// A collection under an open label lists every member; under a specific
/// label (a life stage) only member 0 — the engine's no-thing preview.
fn collection_variants(
    spec: &VariantSpec,
    members: &[CollectionMember],
    truncated: usize,
    view: &TextureView<'_>,
) -> Option<Expanded> {
    let base = &spec.graphic.base;
    if spec.label != VariantLabel::Only {
        let first = members.first()?;
        return one(spec, member_faces(first, base, view)?);
    }
    let count = members.len();
    let is_stack = matches!(
        spec.graphic.class,
        GraphicClass::Collection(CollectionKind::StackCount)
    );
    let variants = members
        .iter()
        .enumerate()
        .filter_map(|(index, member)| {
            let label = if is_stack {
                VariantLabel::Stack { index, count }
            } else {
                VariantLabel::Member {
                    index,
                    count,
                    name: member_name(member, base),
                }
            };
            Some(GraphicVariant {
                label,
                faces: member_faces(member, base, view)?,
            })
        })
        .collect();
    Some(Expanded {
        variants,
        truncated,
    })
}

fn appearance_variants(
    members: &[AppearanceMember],
    graphic: &GraphicSpec,
    is_open: bool,
    view: &TextureView<'_>,
) -> Expanded {
    let take = if is_open { members.len() } else { 1 };
    let variants = members
        .iter()
        .take(take)
        .filter_map(|appearance| {
            Some(GraphicVariant {
                label: if is_open {
                    VariantLabel::Appearance(appearance.def_name.clone())
                } else {
                    VariantLabel::Only
                },
                faces: member_faces(&appearance.member, &graphic.base, view)?,
            })
        })
        .collect();
    Expanded {
        variants,
        truncated: 0,
    }
}

fn default_index(default: DefaultVariant, variants: &[GraphicVariant]) -> usize {
    match default {
        DefaultVariant::At(index) => index.min(variants.len().saturating_sub(1)),
        DefaultVariant::FirstLocated => variants
            .iter()
            .position(|variant| variant.faces.is_located())
            .unwrap_or(0),
    }
}

/// Builds one slot. `None` when no variant could be read, or when a probe
/// slot resolves to no file at all.
fn build_slot(spec: &SlotSpec, view: &TextureView<'_>) -> Option<GraphicSlot> {
    let first = spec.variants.first()?;
    if spec.source.is_probe() && layout_of(&first.graphic, view) == GraphicLayout::Missing {
        return None;
    }
    let expanded: Vec<Expanded> = spec
        .variants
        .iter()
        .filter_map(|variant| expand_spec(variant, view))
        .collect();
    let truncated = expanded.iter().map(|part| part.truncated).sum();
    let variants: Vec<GraphicVariant> = expanded
        .into_iter()
        .flat_map(|part| part.variants)
        .collect();
    let default = default_index(spec.default, &variants);
    GraphicSlot::new(
        spec.source.clone(),
        first.graphic.class_label.clone(),
        first.graphic.class.is_inferred(),
        variants,
        default,
    )
    .ok()
    .map(|slot| slot.with_truncated(truncated))
}

fn layout_of(graphic: &GraphicSpec, view: &TextureView<'_>) -> GraphicLayout {
    expand(
        &ExpandRequest {
            class: &graphic.class,
            base_key: graphic.base.as_str(),
            flip: graphic.flip,
        },
        view.textures,
    )
}

/// Builds the set for `specs`, or `None` when nothing could be shown.
#[must_use]
pub fn build_set(specs: &[SlotSpec], view: &TextureView<'_>) -> Option<GraphicSet> {
    let mut slots: Vec<GraphicSlot> = specs
        .iter()
        .filter_map(|spec| build_slot(spec, view))
        .collect();
    let truncated_slots = slots.len().saturating_sub(MAX_SLOTS);
    slots.truncate(MAX_SLOTS);
    let default_slot = default_slot(&slots);
    GraphicSet::new(slots, default_slot, truncated_slots).ok()
}

/// The first engine slot a loose file serves, else the first engine slot,
/// else the first slot: a probe is never the default while an engine slot
/// exists.
fn default_slot(slots: &[GraphicSlot]) -> usize {
    let is_engine = |slot: &GraphicSlot| !matches!(slot.source(), SlotSource::Probe { .. });
    slots
        .iter()
        .position(|slot| is_engine(slot) && slot.is_located())
        .or_else(|| slots.iter().position(is_engine))
        .unwrap_or(0)
}
