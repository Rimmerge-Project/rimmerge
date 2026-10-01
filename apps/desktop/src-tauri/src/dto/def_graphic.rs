//! DTOs for `resolve_def_graphic` and `read_def_texture`: what texture a
//! def shows, as texture keys with an availability, and the bytes behind
//! one key. Nothing here carries an absolute path.
//!
//! Every tagged enum is `kind`-tagged; a struct variant's own multi-word
//! fields carry their own `#[serde(rename)]` (the tag's `rename_all` only
//! renames variants). A variant that would need a field called `kind`
//! (a pawn kind's name) names it `def_name` instead, since it would
//! collide with the tag.

use rim_analyzer::extract::graphics::Facing;
use rim_session::use_cases::{
    Availability, DefGraphic, DefTexture, Face, Faces, GraphicSet, GraphicSlot, GraphicVariant,
    ImageSource, SlotSource, VariantLabel, ViewRef,
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::mod_info::PreviewUnreadableReasonDto;
use super::texture::{TextureFormatDto, encode_data_url};

/// `resolve_def_graphic`'s request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ResolveDefGraphicRequestDto {
    /// The def's canonical ref text (see `DefRef`'s `Display`).
    pub def_ref: String,
}

/// `read_def_texture`'s request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ReadDefTextureRequestDto {
    /// The def's canonical ref text.
    pub def_ref: String,
    /// A texture key taken from that def's resolved graphic; any other key
    /// is refused.
    pub texture_key: String,
}

/// A direction of a directional graphic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum FacingDto {
    /// North.
    North,
    /// East.
    East,
    /// South.
    South,
    /// West.
    West,
}

impl From<Facing> for FacingDto {
    fn from(value: Facing) -> Self {
        match value {
            Facing::North => Self::North,
            Facing::East => Self::East,
            Facing::South => Self::South,
            Facing::West => Self::West,
        }
    }
}

/// Whether the engine finds a texture, and where it would come from.
/// Mirrors [`Availability`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum FaceAvailabilityDto {
    /// A loose file exists; `owner` is the mod whose file the game uses.
    Loose {
        /// The owning mod's id.
        owner: String,
    },
    /// Served by an asset bundle or the game's built-in resources.
    BaseGameOrBundle,
    /// Nowhere: the game shows its error texture.
    NotFound,
    /// Nowhere, but it may be a built-in texture this tool cannot see.
    Unknown,
}

impl From<&Availability> for FaceAvailabilityDto {
    fn from(value: &Availability) -> Self {
        match value {
            Availability::Loose { owner } => Self::Loose {
                owner: owner.as_str().to_string(),
            },
            Availability::BaseGameOrBundle => Self::BaseGameOrBundle,
            Availability::NotFound => Self::NotFound,
            Availability::Unknown => Self::Unknown,
        }
    }
}

/// One texture the game would draw for one direction. Mirrors [`Face`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct GraphicFaceDto {
    /// The texture's key: the argument of `read_def_texture`.
    pub texture_key: String,
    /// The direction this face is drawn for; `null` for a graphic with one
    /// texture for every direction.
    pub facing: Option<FacingDto>,
    /// Whether the game draws it mirrored.
    pub is_mirrored: bool,
    /// Whether and where the game finds it.
    pub availability: FaceAvailabilityDto,
}

impl GraphicFaceDto {
    fn new(face: &Face, facing: Option<Facing>) -> Self {
        Self {
            texture_key: face.key.as_str().to_string(),
            facing: facing.map(Into::into),
            is_mirrored: face.is_mirrored,
            availability: (&face.availability).into(),
        }
    }
}

/// The faces of one variant. Mirrors [`Faces`]: a `multi` always has all
/// four directions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum GraphicFacesDto {
    /// One texture for every direction.
    Single {
        /// The texture.
        face: GraphicFaceDto,
    },
    /// A texture per direction.
    Multi {
        /// North.
        north: GraphicFaceDto,
        /// East.
        east: GraphicFaceDto,
        /// South.
        south: GraphicFaceDto,
        /// West.
        west: GraphicFaceDto,
    },
}

impl From<&Faces> for GraphicFacesDto {
    fn from(value: &Faces) -> Self {
        match value {
            Faces::Single(face) => Self::Single {
                face: GraphicFaceDto::new(face, None),
            },
            Faces::Multi {
                north,
                east,
                south,
                west,
            } => Self::Multi {
                north: GraphicFaceDto::new(north, Some(Facing::North)),
                east: GraphicFaceDto::new(east, Some(Facing::East)),
                south: GraphicFaceDto::new(south, Some(Facing::South)),
                west: GraphicFaceDto::new(west, Some(Facing::West)),
            },
        }
    }
}

/// What distinguishes one variant of a slot from its siblings. Mirrors
/// [`VariantLabel`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum VariantLabelDto {
    /// The slot has only this variant.
    Only,
    /// Apparel worn on this body type.
    BodyType {
        /// The body type's def name.
        name: String,
    },
    /// A life stage of a pawn kind.
    LifeStage {
        /// The stage, 0-based.
        index: usize,
        /// How many stages the kind has.
        count: usize,
    },
    /// The female graphic of a life stage.
    Female {
        /// The stage, 0-based.
        stage: usize,
    },
    /// An alternate graphic of a pawn kind.
    Alternate {
        /// The alternate's position.
        index: usize,
    },
    /// A member of a random-style collection.
    Member {
        /// The member's position.
        index: usize,
        /// How many members were listed.
        count: usize,
        /// The member's file name.
        name: String,
    },
    /// A member of a stack-count collection (0 is the single item).
    Stack {
        /// The member's position.
        index: usize,
        /// How many members were listed.
        count: usize,
    },
    /// A material appearance.
    Appearance {
        /// The appearance's def name.
        name: String,
    },
    /// One of several worn graphic paths.
    WornPath {
        /// The path's position.
        index: usize,
        /// How many paths the apparel lists.
        count: usize,
        /// The body type, when the path takes a body-type suffix.
        #[serde(rename = "bodyType")]
        body_type: Option<String>,
    },
    /// A named field (an icon path).
    Field {
        /// The field's name.
        name: String,
    },
}

impl From<&VariantLabel> for VariantLabelDto {
    fn from(value: &VariantLabel) -> Self {
        match value {
            VariantLabel::Only => Self::Only,
            VariantLabel::BodyType(name) => Self::BodyType { name: name.clone() },
            VariantLabel::LifeStage { index, count } => Self::LifeStage {
                index: *index,
                count: *count,
            },
            VariantLabel::Female { stage } => Self::Female { stage: *stage },
            VariantLabel::Alternate { index } => Self::Alternate { index: *index },
            VariantLabel::Member { index, count, name } => Self::Member {
                index: *index,
                count: *count,
                name: name.clone(),
            },
            VariantLabel::Stack { index, count } => Self::Stack {
                index: *index,
                count: *count,
            },
            VariantLabel::Appearance(name) => Self::Appearance { name: name.clone() },
            VariantLabel::WornPath {
                index,
                count,
                body_type,
            } => Self::WornPath {
                index: *index,
                count: *count,
                body_type: body_type.clone(),
            },
            VariantLabel::Field(name) => Self::Field { name: name.clone() },
        }
    }
}

/// One variant: a label and the faces it shows. Mirrors [`GraphicVariant`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct GraphicVariantDto {
    /// What distinguishes the variant.
    pub label: VariantLabelDto,
    /// What it shows.
    pub faces: GraphicFacesDto,
    /// Whether at least one face is served by a loose file.
    pub is_located: bool,
}

impl From<&GraphicVariant> for GraphicVariantDto {
    fn from(value: &GraphicVariant) -> Self {
        Self {
            label: (&value.label).into(),
            faces: (&value.faces).into(),
            is_located: value.faces.is_located(),
        }
    }
}

/// Where a slot's texture came from. Mirrors [`SlotSource`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum SlotSourceDto {
    /// `graphicData`.
    Graphic,
    /// An apparel's worn graphic.
    Worn,
    /// A pawn kind's life stages.
    LifeStage,
    /// A pawn kind that uses this race.
    RaceKind {
        /// The pawn kind's def name. Not called `kind`: that is the tag.
        #[serde(rename = "defName")]
        def_name: String,
    },
    /// A style item's texture path.
    StyleTexture,
    /// A head type's graphic path.
    HeadType,
    /// A body type's naked graphic path.
    BodyType,
    /// A terrain's texture path.
    Terrain,
    /// An icon path.
    Icon,
    /// Any other field whose text resolves to textures; approximate.
    Probe {
        /// The field path, `/`-separated.
        field: String,
    },
}

impl From<&SlotSource> for SlotSourceDto {
    fn from(value: &SlotSource) -> Self {
        match value {
            SlotSource::Graphic => Self::Graphic,
            SlotSource::Worn => Self::Worn,
            SlotSource::LifeStage => Self::LifeStage,
            SlotSource::RaceKind { kind } => Self::RaceKind {
                def_name: kind.clone(),
            },
            SlotSource::StyleTexture => Self::StyleTexture,
            SlotSource::HeadType => Self::HeadType,
            SlotSource::BodyType => Self::BodyType,
            SlotSource::Terrain => Self::Terrain,
            SlotSource::Icon => Self::Icon,
            SlotSource::Probe { field } => Self::Probe {
                field: field.clone(),
            },
        }
    }
}

/// One source of texture for a def, with its variants. Mirrors
/// [`GraphicSlot`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct GraphicSlotDto {
    /// Where the slot came from.
    pub source: SlotSourceDto,
    /// The `graphicClass` text, when the slot has one.
    pub graphic_class: Option<String>,
    /// Whether the layout is guessed from file names (an unmodelled class).
    pub is_layout_inferred: bool,
    /// The variants; never empty.
    pub variants: Vec<GraphicVariantDto>,
    /// Index of the variant shown first; always in range.
    pub default_variant: usize,
    /// Collection members that were not listed.
    pub truncated: usize,
    /// Whether any variant has a loose face.
    pub is_located: bool,
}

impl From<&GraphicSlot> for GraphicSlotDto {
    fn from(value: &GraphicSlot) -> Self {
        Self {
            source: value.source().into(),
            graphic_class: value.graphic_class().map(str::to_string),
            is_layout_inferred: value.is_layout_inferred(),
            variants: value.variants().iter().map(Into::into).collect(),
            default_variant: value.default_variant(),
            truncated: value.truncated(),
            is_located: value.is_located(),
        }
    }
}

/// A slot and a variant within it. Mirrors [`ViewRef`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ViewRefDto {
    /// Index into the set's slots.
    pub slot: usize,
    /// Index into that slot's variants.
    pub variant: usize,
}

impl From<ViewRef> for ViewRefDto {
    fn from(value: ViewRef) -> Self {
        Self {
            slot: value.slot,
            variant: value.variant,
        }
    }
}

/// `resolve_def_graphic`'s response. Mirrors [`DefGraphic`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum DefGraphicDto {
    /// At least one slot.
    Resolved {
        /// The slots, in display order; never empty.
        slots: Vec<GraphicSlotDto>,
        /// The view shown first.
        #[serde(rename = "defaultView")]
        default_view: ViewRefDto,
        /// Slots left out because of the per-def bound.
        #[serde(rename = "truncatedSlots")]
        truncated_slots: usize,
    },
    /// The def shows no texture of its own.
    NoGraphic,
    /// A humanlike race: the game assembles it at runtime.
    ComposedAtRuntime,
}

impl From<&GraphicSet> for DefGraphicDto {
    fn from(value: &GraphicSet) -> Self {
        Self::Resolved {
            slots: value.slots().iter().map(Into::into).collect(),
            default_view: value.default_view().into(),
            truncated_slots: value.truncated_slots(),
        }
    }
}

impl From<&DefGraphic> for DefGraphicDto {
    fn from(value: &DefGraphic) -> Self {
        match value {
            DefGraphic::Resolved(set) => set.into(),
            DefGraphic::NoGraphic => Self::NoGraphic,
            DefGraphic::ComposedAtRuntime => Self::ComposedAtRuntime,
        }
    }
}

/// Whether a shown image is the game's own file. Mirrors [`ImageSource`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum ImageSourceDto {
    /// The very file the game loads.
    Direct,
    /// A PNG/JPEG beside the `.dds` the game loads instead.
    PngSibling,
}

impl From<ImageSource> for ImageSourceDto {
    fn from(value: ImageSource) -> Self {
        match value {
            ImageSource::Direct => Self::Direct,
            ImageSource::PngSibling => Self::PngSibling,
        }
    }
}

/// `read_def_texture`'s response. Mirrors [`DefTexture`]; an image carries
/// its bytes as a `data:` URL and never a path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum DefTextureDto {
    /// An image to show.
    Image {
        /// A `data:image/<format>;base64,...` URL.
        #[serde(rename = "dataUrl")]
        data_url: String,
        /// The sniffed image format.
        format: TextureFormatDto,
        /// The file's raw byte count (before base64 encoding).
        bytes: usize,
        /// The mod whose file this is.
        owner: String,
        /// Whether it is the game's own file or a sibling copy.
        from: ImageSourceDto,
    },
    /// The game loads a `.dds` and no image copy exists to show instead.
    DdsNotPreviewable {
        /// The mod shipping the `.dds`.
        owner: String,
    },
    /// The `.dds` is one the game cannot decode (it shows a placeholder).
    UndecodableInGame {
        /// The mod shipping the `.dds`.
        owner: String,
    },
    /// Built into the game or an asset bundle; never read.
    NotViewable {
        /// Whether it may be a built-in texture this tool cannot see.
        #[serde(rename = "isUncertain")]
        is_uncertain: bool,
    },
    /// No texture file anywhere: the game shows its error texture.
    NotFound,
    /// The file exists but cannot be shown.
    Unreadable {
        /// Why the file couldn't be read back.
        reason: PreviewUnreadableReasonDto,
    },
}

impl From<&DefTexture> for DefTextureDto {
    fn from(value: &DefTexture) -> Self {
        match value {
            DefTexture::Image {
                texture,
                owner,
                from,
            } => {
                let format: TextureFormatDto = texture.format.into();
                Self::Image {
                    data_url: encode_data_url(format, &texture.bytes),
                    format,
                    bytes: texture.bytes.len(),
                    owner: owner.as_str().to_string(),
                    from: (*from).into(),
                }
            }
            DefTexture::DdsNotPreviewable { owner } => Self::DdsNotPreviewable {
                owner: owner.as_str().to_string(),
            },
            DefTexture::UndecodableInGame { owner } => Self::UndecodableInGame {
                owner: owner.as_str().to_string(),
            },
            DefTexture::NotViewable { is_uncertain } => Self::NotViewable {
                is_uncertain: *is_uncertain,
            },
            DefTexture::NotFound => Self::NotFound,
            DefTexture::Unreadable(reason) => Self::Unreadable {
                reason: reason.into(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use rim_analyzer::domain::ModId;
    use rim_session::ports::{TextureBytes, TextureFormat};
    use rim_session::use_cases::{PreviewUnreadable, TextureKey};
    use serde_json::json;

    use super::*;

    fn face(key: &str, availability: Availability) -> Face {
        Face {
            key: TextureKey::parse(key).expect("valid key"),
            is_mirrored: false,
            availability,
        }
    }

    fn loose(owner: &str) -> Availability {
        Availability::Loose {
            owner: ModId::new(owner),
        }
    }

    fn wire<T: Serialize>(value: &T) -> serde_json::Value {
        serde_json::to_value(value).expect("serializable")
    }

    #[test]
    fn single_face_has_no_facing_and_carries_its_availability() {
        let faces = Faces::Single(face("things/x", loose("a")));

        let dto: GraphicFacesDto = (&faces).into();

        assert_eq!(
            wire(&dto),
            json!({
                "kind": "single",
                "face": {
                    "textureKey": "things/x",
                    "facing": null,
                    "isMirrored": false,
                    "availability": { "kind": "loose", "owner": "a" },
                },
            })
        );
    }

    #[test]
    fn multi_faces_name_each_direction() {
        let faces = Faces::Multi {
            north: face("t/n", Availability::NotFound),
            east: Face {
                is_mirrored: true,
                ..face("t/e", Availability::Unknown)
            },
            south: face("t/s", Availability::BaseGameOrBundle),
            west: face("t/w", loose("b")),
        };

        let json = wire(&GraphicFacesDto::from(&faces));

        assert_eq!(json["kind"], "multi");
        assert_eq!(json["north"]["facing"], "north");
        assert_eq!(json["north"]["availability"], json!({ "kind": "notFound" }));
        assert_eq!(json["east"]["facing"], "east");
        assert_eq!(json["east"]["isMirrored"], true);
        assert_eq!(json["east"]["availability"], json!({ "kind": "unknown" }));
        assert_eq!(json["south"]["facing"], "south");
        assert_eq!(
            json["south"]["availability"],
            json!({ "kind": "baseGameOrBundle" })
        );
        assert_eq!(json["west"]["facing"], "west");
        assert_eq!(json["west"]["availability"]["owner"], "b");
    }

    #[test]
    fn every_variant_label_serializes_camel_case_fields() {
        let cases = [
            (VariantLabel::Only, json!({ "kind": "only" })),
            (
                VariantLabel::BodyType("Thin".to_string()),
                json!({ "kind": "bodyType", "name": "Thin" }),
            ),
            (
                VariantLabel::LifeStage { index: 1, count: 3 },
                json!({ "kind": "lifeStage", "index": 1, "count": 3 }),
            ),
            (
                VariantLabel::Female { stage: 2 },
                json!({ "kind": "female", "stage": 2 }),
            ),
            (
                VariantLabel::Alternate { index: 4 },
                json!({ "kind": "alternate", "index": 4 }),
            ),
            (
                VariantLabel::Member {
                    index: 0,
                    count: 2,
                    name: "a_0".to_string(),
                },
                json!({ "kind": "member", "index": 0, "count": 2, "name": "a_0" }),
            ),
            (
                VariantLabel::Stack { index: 0, count: 3 },
                json!({ "kind": "stack", "index": 0, "count": 3 }),
            ),
            (
                VariantLabel::Appearance("Planks".to_string()),
                json!({ "kind": "appearance", "name": "Planks" }),
            ),
            (
                VariantLabel::WornPath {
                    index: 0,
                    count: 2,
                    body_type: Some("Hulk".to_string()),
                },
                json!({ "kind": "wornPath", "index": 0, "count": 2, "bodyType": "Hulk" }),
            ),
            (
                VariantLabel::WornPath {
                    index: 1,
                    count: 2,
                    body_type: None,
                },
                json!({ "kind": "wornPath", "index": 1, "count": 2, "bodyType": null }),
            ),
            (
                VariantLabel::Field("uiIconPath".to_string()),
                json!({ "kind": "field", "name": "uiIconPath" }),
            ),
        ];

        for (label, expected) in cases {
            assert_eq!(wire(&VariantLabelDto::from(&label)), expected, "{label:?}");
        }
    }

    #[test]
    fn every_slot_source_serializes_and_the_race_kind_avoids_the_tag_name() {
        let cases = [
            (SlotSource::Graphic, json!({ "kind": "graphic" })),
            (SlotSource::Worn, json!({ "kind": "worn" })),
            (SlotSource::LifeStage, json!({ "kind": "lifeStage" })),
            (
                SlotSource::RaceKind {
                    kind: "Colonist".to_string(),
                },
                json!({ "kind": "raceKind", "defName": "Colonist" }),
            ),
            (SlotSource::StyleTexture, json!({ "kind": "styleTexture" })),
            (SlotSource::HeadType, json!({ "kind": "headType" })),
            (SlotSource::BodyType, json!({ "kind": "bodyType" })),
            (SlotSource::Terrain, json!({ "kind": "terrain" })),
            (SlotSource::Icon, json!({ "kind": "icon" })),
            (
                SlotSource::Probe {
                    field: "a/li".to_string(),
                },
                json!({ "kind": "probe", "field": "a/li" }),
            ),
        ];

        for (source, expected) in cases {
            assert_eq!(wire(&SlotSourceDto::from(&source)), expected, "{source:?}");
        }
    }

    fn one_slot_set() -> GraphicSet {
        let variant = GraphicVariant {
            label: VariantLabel::Only,
            faces: Faces::Single(face("things/x", loose("a"))),
        };
        let slot = GraphicSlot::new(
            SlotSource::Graphic,
            Some("Graphic_Single".to_string()),
            false,
            vec![variant],
            0,
        )
        .expect("a valid slot");
        GraphicSet::new(vec![slot], 0, 2).expect("a valid set")
    }

    #[test]
    fn resolved_graphic_serializes_slots_default_view_and_truncation() {
        let set = one_slot_set();

        let json = wire(&DefGraphicDto::from(&DefGraphic::Resolved(set)));

        assert_eq!(json["kind"], "resolved");
        assert_eq!(json["defaultView"], json!({ "slot": 0, "variant": 0 }));
        assert_eq!(json["truncatedSlots"], 2);
        let slot = &json["slots"][0];
        assert_eq!(slot["graphicClass"], "Graphic_Single");
        assert_eq!(slot["isLayoutInferred"], false);
        assert_eq!(slot["defaultVariant"], 0);
        assert_eq!(slot["truncated"], 0);
        assert_eq!(slot["isLocated"], true);
        assert_eq!(slot["variants"][0]["isLocated"], true);
        assert_eq!(slot["variants"][0]["label"], json!({ "kind": "only" }));
    }

    #[test]
    fn no_graphic_and_composed_at_runtime_are_bare_kinds() {
        assert_eq!(
            wire(&DefGraphicDto::from(&DefGraphic::NoGraphic)),
            json!({ "kind": "noGraphic" })
        );
        assert_eq!(
            wire(&DefGraphicDto::from(&DefGraphic::ComposedAtRuntime)),
            json!({ "kind": "composedAtRuntime" })
        );
    }

    #[test]
    fn a_slot_without_a_loose_face_is_not_located() {
        let variant = GraphicVariant {
            label: VariantLabel::Only,
            faces: Faces::Single(face("things/x", Availability::NotFound)),
        };
        let slot = GraphicSlot::new(SlotSource::Graphic, None, true, vec![variant], 0)
            .expect("a valid slot");

        let dto = GraphicSlotDto::from(&slot);

        assert!(!dto.is_located);
        assert!(dto.is_layout_inferred);
        assert_eq!(dto.graphic_class, None);
    }

    #[test]
    fn every_def_texture_outcome_serializes_with_no_path() {
        let png = TextureBytes {
            format: TextureFormat::Png,
            bytes: vec![0x89, 0x50],
        };
        let cases = [
            (
                DefTexture::Image {
                    texture: png,
                    owner: ModId::new("a"),
                    from: ImageSource::PngSibling,
                },
                json!({
                    "kind": "image",
                    "dataUrl": "data:image/png;base64,iVA=",
                    "format": "png",
                    "bytes": 2,
                    "owner": "a",
                    "from": "pngSibling",
                }),
            ),
            (
                DefTexture::DdsNotPreviewable {
                    owner: ModId::new("a"),
                },
                json!({ "kind": "ddsNotPreviewable", "owner": "a" }),
            ),
            (
                DefTexture::UndecodableInGame {
                    owner: ModId::new("a"),
                },
                json!({ "kind": "undecodableInGame", "owner": "a" }),
            ),
            (
                DefTexture::NotViewable { is_uncertain: true },
                json!({ "kind": "notViewable", "isUncertain": true }),
            ),
            (DefTexture::NotFound, json!({ "kind": "notFound" })),
            (
                DefTexture::Unreadable(PreviewUnreadable::TooLarge),
                json!({ "kind": "unreadable", "reason": "tooLarge" }),
            ),
            (
                DefTexture::Unreadable(PreviewUnreadable::UnsupportedFormat),
                json!({ "kind": "unreadable", "reason": "unsupportedFormat" }),
            ),
            (
                DefTexture::Unreadable(PreviewUnreadable::Io("C:/secret/path".to_string())),
                json!({ "kind": "unreadable", "reason": "io" }),
            ),
        ];

        for (texture, expected) in cases {
            assert_eq!(
                wire(&DefTextureDto::from(&texture)),
                expected,
                "{texture:?}"
            );
        }
    }

    #[test]
    fn a_direct_jpeg_image_reports_direct() {
        let texture = DefTexture::Image {
            texture: TextureBytes {
                format: TextureFormat::Jpeg,
                bytes: vec![0xFF, 0xD8, 0xFF],
            },
            owner: ModId::new("b"),
            from: ImageSource::Direct,
        };

        let json = wire(&DefTextureDto::from(&texture));

        assert_eq!(json["from"], "direct");
        assert!(
            json["dataUrl"]
                .as_str()
                .is_some_and(|url| url.starts_with("data:image/jpeg;base64,"))
        );
    }

    #[test]
    fn requests_reject_unknown_fields() {
        let result: Result<ReadDefTextureRequestDto, _> = serde_json::from_value(json!({
            "defRef": "ThingDef/X",
            "textureKey": "things/x",
            "path": "C:/x",
        }));

        assert!(result.is_err());
    }

    #[test]
    fn requests_read_camel_case_fields() {
        let request: ReadDefTextureRequestDto = serde_json::from_value(json!({
            "defRef": "ThingDef/X",
            "textureKey": "things/x",
        }))
        .expect("valid request");

        assert_eq!(request.def_ref, "ThingDef/X");
        assert_eq!(request.texture_key, "things/x");
    }
}
