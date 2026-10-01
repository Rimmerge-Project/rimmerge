//! The def-graphic value types: what a def shows, as texture keys with an
//! availability, never as bytes. Invalid shapes (a Multi with three faces,
//! a slot with no variant, a default pointing nowhere) cannot be built.

use rim_analyzer::domain::ModId;
use rim_analyzer::extract::graphics::Facing;

use super::key::TextureKey;

/// Whether the engine finds a texture, and where it would come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Availability {
    /// A loose file exists; `owner` is the last-loaded shipper under the
    /// selected order, the one whose file the engine uses.
    Loose {
        /// The mod whose file wins.
        owner: ModId,
    },
    /// Served only by an asset bundle or Core's built-in resources: it
    /// exists, but cannot be shown.
    BaseGameOrBundle,
    /// Nowhere, and the Core index is trusted: the engine shows its error
    /// texture.
    NotFound,
    /// Nowhere, but the Core index is not trusted, so it may be a
    /// built-in texture this tool cannot see.
    Unknown,
}

impl Availability {
    /// Whether a loose file serves this face.
    #[must_use]
    pub fn is_loose(&self) -> bool {
        matches!(self, Self::Loose { .. })
    }
}

/// One texture the engine would draw for one direction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Face {
    /// The texture's key.
    pub key: TextureKey,
    /// Whether the engine draws it mirrored.
    pub is_mirrored: bool,
    /// Whether and where the engine finds it.
    pub availability: Availability,
}

/// The faces of one variant. A `Multi` always has all four.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Faces {
    /// One texture for every direction.
    Single(Face),
    /// A texture per direction.
    Multi {
        /// North.
        north: Face,
        /// East.
        east: Face,
        /// South.
        south: Face,
        /// West.
        west: Face,
    },
}

impl Faces {
    /// The face for `facing`; a `Single` shows the same face for all.
    #[must_use]
    pub fn face(&self, facing: Facing) -> &Face {
        match self {
            Self::Single(face) => face,
            Self::Multi {
                north,
                east,
                south,
                west,
            } => match facing {
                Facing::North => north,
                Facing::East => east,
                Facing::South => south,
                Facing::West => west,
            },
        }
    }

    /// The default view of a variant: south.
    #[must_use]
    pub fn default_face(&self) -> &Face {
        self.face(Facing::South)
    }

    /// Every face, in north, east, south, west order for a `Multi`.
    #[must_use]
    pub fn all(&self) -> Vec<&Face> {
        match self {
            Self::Single(face) => vec![face],
            Self::Multi {
                north,
                east,
                south,
                west,
            } => vec![north, east, south, west],
        }
    }

    /// Whether the viewer needs a facing control.
    #[must_use]
    pub fn is_directional(&self) -> bool {
        matches!(self, Self::Multi { .. })
    }

    /// Whether at least one face is served by a loose file.
    #[must_use]
    pub fn is_located(&self) -> bool {
        match self {
            Self::Single(face) => face.availability.is_loose(),
            Self::Multi {
                north,
                east,
                south,
                west,
            } => [north, east, south, west]
                .iter()
                .any(|face| face.availability.is_loose()),
        }
    }
}

/// What distinguishes one variant of a slot from its siblings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VariantLabel {
    /// The slot has only this variant.
    Only,
    /// Apparel worn on this body type.
    BodyType(String),
    /// A life stage of a pawn kind (0-based `index` of `count`).
    LifeStage {
        /// The stage.
        index: usize,
        /// How many stages the kind has.
        count: usize,
    },
    /// The female graphic of a life stage.
    Female {
        /// The stage.
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
        /// How many members were expanded.
        count: usize,
        /// The member's file name.
        name: String,
    },
    /// A member of a stack-count collection (0 is the single item).
    Stack {
        /// The member's position.
        index: usize,
        /// How many members were expanded.
        count: usize,
    },
    /// A material appearance of a `Graphic_Appearances` graphic.
    Appearance(String),
    /// One of several `wornGraphicPaths`, optionally on one body type.
    WornPath {
        /// The path's position.
        index: usize,
        /// How many paths the apparel lists.
        count: usize,
        /// The body type, when the path takes a body-type suffix.
        body_type: Option<String>,
    },
    /// A named field (an icon path).
    Field(String),
}

/// One variant: a label and the faces it shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphicVariant {
    /// What distinguishes the variant.
    pub label: VariantLabel,
    /// What it shows.
    pub faces: Faces,
}

/// Where a slot's texture came from, in the order slots are listed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlotSource {
    /// `graphicData`.
    Graphic,
    /// An apparel's worn graphic.
    Worn,
    /// A pawn kind's life stages.
    LifeStage,
    /// A pawn kind that uses this race.
    RaceKind {
        /// The kind's `defName`.
        kind: String,
    },
    /// A style item's `texPath` (hair, beard, tattoo).
    StyleTexture,
    /// A head type's `graphicPath`.
    HeadType,
    /// A body type's `bodyNakedGraphicPath`.
    BodyType,
    /// A terrain's `texturePath`.
    Terrain,
    /// `uiIconPath` / `iconPath`.
    Icon,
    /// Any other field whose text resolves to textures — approximate.
    Probe {
        /// The field path, `/`-separated (`li` for list items).
        field: String,
    },
}

impl SlotSource {
    /// Whether this slot is a guess from the field's text rather than an
    /// engine rule.
    #[must_use]
    pub fn is_probe(&self) -> bool {
        matches!(self, Self::Probe { .. })
    }
}

/// Why a [`GraphicSlot`] or [`GraphicSet`] could not be built.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum GraphicModelError {
    /// A slot needs at least one variant.
    #[error("a graphic slot has no variants")]
    NoVariants,
    /// The default variant is past the end of the slot.
    #[error("default variant {index} is out of range")]
    DefaultVariantOutOfRange {
        /// The offending index.
        index: usize,
    },
    /// A set needs at least one slot.
    #[error("a graphic set has no slots")]
    NoSlots,
    /// The default slot is past the end of the set.
    #[error("default slot {index} is out of range")]
    DefaultSlotOutOfRange {
        /// The offending index.
        index: usize,
    },
}

/// One source of texture for a def, with its variants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphicSlot {
    source: SlotSource,
    graphic_class: Option<String>,
    is_layout_inferred: bool,
    variants: Vec<GraphicVariant>,
    default_variant: usize,
    truncated: usize,
}

impl GraphicSlot {
    /// Builds a slot.
    ///
    /// # Errors
    ///
    /// [`GraphicModelError`] for an empty variant list or an out-of-range
    /// default.
    pub fn new(
        source: SlotSource,
        graphic_class: Option<String>,
        is_layout_inferred: bool,
        variants: Vec<GraphicVariant>,
        default_variant: usize,
    ) -> Result<Self, GraphicModelError> {
        if variants.is_empty() {
            return Err(GraphicModelError::NoVariants);
        }
        if default_variant >= variants.len() {
            return Err(GraphicModelError::DefaultVariantOutOfRange {
                index: default_variant,
            });
        }
        Ok(Self {
            source,
            graphic_class,
            is_layout_inferred,
            variants,
            default_variant,
            truncated: 0,
        })
    }

    /// Records how many members were left out of the variant list.
    #[must_use]
    pub fn with_truncated(mut self, truncated: usize) -> Self {
        self.truncated = truncated;
        self
    }

    /// Where the slot came from.
    #[must_use]
    pub fn source(&self) -> &SlotSource {
        &self.source
    }

    /// The `graphicClass` text, when the slot has one.
    #[must_use]
    pub fn graphic_class(&self) -> Option<&str> {
        self.graphic_class.as_deref()
    }

    /// Whether the layout is guessed from file names (an unmodelled class).
    #[must_use]
    pub fn is_layout_inferred(&self) -> bool {
        self.is_layout_inferred
    }

    /// The variants; never empty.
    #[must_use]
    pub fn variants(&self) -> &[GraphicVariant] {
        &self.variants
    }

    /// Index of the variant shown first; always in range.
    #[must_use]
    pub fn default_variant(&self) -> usize {
        self.default_variant
    }

    /// Collection members that were not expanded.
    #[must_use]
    pub fn truncated(&self) -> usize {
        self.truncated
    }

    /// Whether any variant has a loose face.
    #[must_use]
    pub fn is_located(&self) -> bool {
        self.variants
            .iter()
            .any(|variant| variant.faces.is_located())
    }
}

/// A slot and a variant within it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewRef {
    /// Index into the set's slots.
    pub slot: usize,
    /// Index into that slot's variants.
    pub variant: usize,
}

/// Everything a def shows. Never empty, and its default view always points
/// at a real variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphicSet {
    slots: Vec<GraphicSlot>,
    default_view: ViewRef,
    truncated_slots: usize,
}

impl GraphicSet {
    /// Builds a set whose default view is `default_slot` at that slot's
    /// own default variant.
    ///
    /// # Errors
    ///
    /// [`GraphicModelError`] for an empty slot list or an out-of-range
    /// default slot.
    pub fn new(
        slots: Vec<GraphicSlot>,
        default_slot: usize,
        truncated_slots: usize,
    ) -> Result<Self, GraphicModelError> {
        if slots.is_empty() {
            return Err(GraphicModelError::NoSlots);
        }
        let Some(slot) = slots.get(default_slot) else {
            return Err(GraphicModelError::DefaultSlotOutOfRange {
                index: default_slot,
            });
        };
        let default_view = ViewRef {
            slot: default_slot,
            variant: slot.default_variant(),
        };
        Ok(Self {
            slots,
            default_view,
            truncated_slots,
        })
    }

    /// The slots, in display order; never empty.
    #[must_use]
    pub fn slots(&self) -> &[GraphicSlot] {
        &self.slots
    }

    /// The view shown first.
    #[must_use]
    pub fn default_view(&self) -> ViewRef {
        self.default_view
    }

    /// Slots that were not listed because of the per-def bound.
    #[must_use]
    pub fn truncated_slots(&self) -> usize {
        self.truncated_slots
    }
}

/// What resolving a def's graphic found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DefGraphic {
    /// At least one slot.
    Resolved(GraphicSet),
    /// The def shows no texture of its own.
    NoGraphic,
    /// A humanlike race: the game assembles it from body, head and hair at
    /// runtime, so there is no single texture to show.
    ComposedAtRuntime,
}
