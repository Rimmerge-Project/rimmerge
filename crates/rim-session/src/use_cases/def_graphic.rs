//! What texture a def shows: the def's effective tree is planned into
//! slots ([`slots`]), each slot is expanded over the texture index into
//! faces with an availability ([`views`]). Pure: no ports, no `Session`.
//! Reading the image bytes is a separate step.

use rim_analyzer::domain::LoadOrder;
use rim_analyzer::extract::graphics::StuffAppearance;

mod key;
mod model;
mod read;
mod resolve;
mod slots;
mod views;

pub use key::{TextureKey, TextureKeyError};
pub use model::{
    Availability, DefGraphic, Face, Faces, GraphicModelError, GraphicSet, GraphicSlot,
    GraphicVariant, SlotSource, VariantLabel, ViewRef,
};
pub use read::{DefTexture, ImageSource, ReadDefTexture, ReadDefTextureError};
pub use resolve::{ResolveDefGraphic, ResolveDefGraphicError};
pub use slots::{
    DefSubject, DefaultVariant, GraphicSpec, SlotFacts, SlotPlan, SlotSpec, VariantSpec, plan_slots,
};
pub use views::{MAX_SLOTS, TextureView, availability, build_set};

// Sibling-file tests; `#[path]` keeps the module named `tests`.
#[cfg(test)]
#[path = "def_graphic/def_graphic_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "def_graphic/use_case_tests.rs"]
mod use_case_tests;

/// Resolves what `subject` shows under `view`: its slots, `NoGraphic`, or
/// `ComposedAtRuntime` for a humanlike race that declares nothing it could
/// show (the game builds such a pawn from body, head and hair).
#[must_use]
pub fn resolve_def_graphic(
    subject: &DefSubject<'_>,
    facts: &SlotFacts<'_>,
    view: &TextureView<'_>,
) -> DefGraphic {
    let plan = plan_slots(subject, facts);
    match build_set(&plan.slots, view) {
        Some(set) => DefGraphic::Resolved(set),
        None if plan.is_humanlike_race => DefGraphic::ComposedAtRuntime,
        None => DefGraphic::NoGraphic,
    }
}

/// The inputs of [`resolve_def_graphic`] that do not depend on the def,
/// borrowed from whoever owns them.
#[derive(Debug, Clone, Copy)]
pub struct GraphicEnvironment<'a> {
    /// Facts from outside the def.
    pub facts: SlotFacts<'a>,
    /// The texture index.
    pub textures: &'a rim_analyzer::analysis::TextureIndex,
    /// The selected order.
    pub order: &'a LoadOrder,
}

impl<'a> GraphicEnvironment<'a> {
    /// The [`TextureView`] over this environment.
    #[must_use]
    pub fn view(&self, stuff_appearances: &'a [StuffAppearance]) -> TextureView<'a> {
        TextureView {
            textures: self.textures,
            order: self.order,
            stuff_appearances,
        }
    }
}
