//! [`ResolveDefGraphic`]: what texture a def shows under the selected
//! order. Gathers the facts [`super::slots`] needs from the effective def
//! machinery ([`InspectDef`]), then runs the pure resolution and caches it
//! on the session.

use std::collections::BTreeSet;

use rim_analyzer::analysis::SourceIndex;
use rim_analyzer::domain::{LoadOrder, Selector};
use rim_analyzer::extract::graphics::{GraphicClass, StuffAppearance};
use rim_merge::tree::{Content, FieldTree};
use rim_resolve::domain::{DefKey, DefRef};

use super::model::DefGraphic;
use super::slots::{DefSubject, SlotFacts, child, child_text};
use super::views::TextureView;
use crate::Session;
use crate::ports::DefSourceReader;
use crate::use_cases::{InspectDef, InspectDefError};

const BODY_TYPE_DEF: &str = "BodyTypeDef";
const APPAREL_LAYER_DEF: &str = "ApparelLayerDef";
const STUFF_APPEARANCE_DEF: &str = "StuffAppearanceDef";
const PAWN_KIND_DEF: &str = "PawnKindDef";

/// Why a def's graphic could not be resolved.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ResolveDefGraphicError {
    /// The def (or a def it needs) could not be inspected.
    #[error(transparent)]
    Inspect(#[from] InspectDefError),
}

/// Facts about other defs, gathered for one resolution.
#[derive(Default)]
struct GatheredFacts {
    body_types: Vec<String>,
    utility_layers: BTreeSet<String>,
    stuff_appearances: Vec<StuffAppearance>,
    race_kinds: Vec<(String, FieldTree)>,
}

/// Resolves what a def shows. Read-only apart from the session cache.
pub struct ResolveDefGraphic<Reader> {
    inspect: InspectDef<Reader>,
}

impl<Reader: DefSourceReader> ResolveDefGraphic<Reader> {
    /// Builds the use case from the def-source port.
    #[must_use]
    pub fn new(reader: Reader) -> Self {
        Self {
            inspect: InspectDef::new(reader),
        }
    }

    /// Answers `def_ref` under [`Session::selected`]'s order, from the
    /// session cache when it is there. The cache is cleared whenever the
    /// order changes and is bounded (see `Session::def_graphic`).
    ///
    /// # Errors
    ///
    /// [`ResolveDefGraphicError::Inspect`] when the def, or a def its rules
    /// read, fails to inspect for a reason other than not existing.
    pub fn execute<'a>(
        &self,
        session: &'a mut Session,
        def_ref: &DefRef,
    ) -> Result<&'a DefGraphic, ResolveDefGraphicError> {
        let source = session.selected();
        if session.def_graphic(source, def_ref).is_none() {
            let graphic = self.resolve(session, def_ref)?;
            session.cache_def_graphic(source, def_ref.clone(), graphic);
        }
        Ok(session
            .def_graphic(source, def_ref)
            .unwrap_or_else(|| unreachable!("resolved and cached just above")))
    }

    fn resolve(
        &self,
        session: &mut Session,
        def_ref: &DefRef,
    ) -> Result<DefGraphic, ResolveDefGraphicError> {
        let tree = self
            .inspect
            .execute(session, def_ref)?
            .effective
            .resolved
            .clone();
        let def_name = def_ref.key.def_name.as_str();
        let facts = self.gather(session, def_name, &tree)?;
        let order = session.orders().get(session.selected());
        let kinds: Vec<DefSubject<'_>> = facts
            .race_kinds
            .iter()
            .map(|(name, kind_tree)| DefSubject {
                name,
                tree: kind_tree,
            })
            .collect();
        let slot_facts = SlotFacts {
            body_types: &facts.body_types,
            utility_layers: &facts.utility_layers,
            stuff_appearances: &facts.stuff_appearances,
            race_kinds: &kinds,
        };
        let view = TextureView {
            textures: &session.sources().textures,
            order,
            stuff_appearances: &facts.stuff_appearances,
        };
        Ok(super::resolve_def_graphic(
            &DefSubject {
                name: def_name,
                tree: &tree,
            },
            &slot_facts,
            &view,
        ))
    }

    /// Reads only what this def's rules can use: body types and layers for
    /// apparel, the appearance list for `Graphic_Appearances`, the kinds
    /// for a race with no graphic of its own.
    fn gather(
        &self,
        session: &mut Session,
        def_name: &str,
        tree: &FieldTree,
    ) -> Result<GatheredFacts, ResolveDefGraphicError> {
        let root = &tree.root;
        let mut facts = GatheredFacts::default();
        if child(root, "apparel").is_some() {
            facts.body_types = self.registered_names(session, BODY_TYPE_DEF);
            facts.utility_layers = self.utility_layers(session)?;
        }
        if uses_appearances(tree) {
            facts.stuff_appearances = self.stuff_appearances(session)?;
        }
        if is_race_without_graphic(tree) {
            facts.race_kinds = self.race_kinds(session, def_name)?;
        }
        Ok(facts)
    }

    fn registered_names(&self, session: &Session, def_type: &str) -> Vec<String> {
        let order = session.orders().get(session.selected());
        registration_order(session.sources(), order, def_type)
    }

    fn utility_layers(
        &self,
        session: &mut Session,
    ) -> Result<BTreeSet<String>, ResolveDefGraphicError> {
        let mut layers = BTreeSet::new();
        for name in self.registered_names(session, APPAREL_LAYER_DEF) {
            let Some(tree) = self.effective_tree(session, APPAREL_LAYER_DEF, &name)? else {
                continue;
            };
            if child_text(&tree.root, "isUtilityLayer")
                .is_some_and(|value| value.eq_ignore_ascii_case("true"))
            {
                layers.insert(name);
            }
        }
        Ok(layers)
    }

    fn stuff_appearances(
        &self,
        session: &mut Session,
    ) -> Result<Vec<StuffAppearance>, ResolveDefGraphicError> {
        let mut appearances = Vec::new();
        for name in self.registered_names(session, STUFF_APPEARANCE_DEF) {
            let Some(tree) = self.effective_tree(session, STUFF_APPEARANCE_DEF, &name)? else {
                continue;
            };
            let path_prefix = child_text(&tree.root, "pathPrefix").map(str::to_owned);
            appearances.push(StuffAppearance {
                def_name: name,
                path_prefix,
            });
        }
        Ok(appearances)
    }

    fn race_kinds(
        &self,
        session: &mut Session,
        race_name: &str,
    ) -> Result<Vec<(String, FieldTree)>, ResolveDefGraphicError> {
        let names = session
            .sources()
            .kinds_by_race
            .get(race_name)
            .cloned()
            .unwrap_or_default();
        let mut kinds = Vec::new();
        for name in names {
            if let Some(tree) = self.effective_tree(session, PAWN_KIND_DEF, &name)? {
                kinds.push((name, tree));
            }
        }
        Ok(kinds)
    }

    /// The effective tree of a related def; `None` when it no longer exists
    /// (a gated-off or stale raw declaration), an error for anything else.
    fn effective_tree(
        &self,
        session: &mut Session,
        def_type: &str,
        def_name: &str,
    ) -> Result<Option<FieldTree>, ResolveDefGraphicError> {
        let def_ref = DefRef::new(
            DefKey {
                def_type: def_type.to_owned(),
                def_name: def_name.to_owned(),
            },
            Selector::DefName,
        );
        match self.inspect.execute(session, &def_ref) {
            Ok(inspection) => Ok(Some(inspection.effective.resolved.clone())),
            Err(InspectDefError::NotFound(_)) => Ok(None),
            Err(other) => Err(other.into()),
        }
    }
}

/// Whether `graphicData` names `Graphic_Appearances`.
fn uses_appearances(tree: &FieldTree) -> bool {
    child(&tree.root, "graphicData")
        .and_then(|data| child_text(data, "graphicClass"))
        .is_some_and(|class| GraphicClass::classify(class) == GraphicClass::Appearances)
}

/// A `race` block and no `graphicData`: the def borrows its kinds' look.
fn is_race_without_graphic(tree: &FieldTree) -> bool {
    let has_race =
        child(&tree.root, "race").is_some_and(|race| matches!(race.content, Content::Children(_)));
    has_race && child(&tree.root, "graphicData").is_none()
}

/// Names of every `def_type` def, in the order the game registers them:
/// by owner position in `order`, then by file, then by document position.
/// A name defined by several mods keeps its earliest registration. Owners
/// outside `order` (inactive) are skipped.
fn registration_order(sources: &SourceIndex, order: &LoadOrder, def_type: &str) -> Vec<String> {
    let mut found: Vec<(usize, &std::path::Path, &[u32], &str)> = sources
        .defs
        .iter()
        .filter(|((_, (found_type, _)), _)| found_type == def_type)
        .filter_map(|((owner, (_, name)), entries)| {
            let position = order.position(owner)?;
            let first = entries.first()?;
            Some((
                position,
                &*first.locator.file,
                first.locator.element_path.as_slice(),
                name.as_str(),
            ))
        })
        .collect();
    found.sort();
    let mut seen = BTreeSet::new();
    found
        .into_iter()
        .filter(|(_, _, _, name)| seen.insert(*name))
        .map(|(_, _, _, name)| name.to_owned())
        .collect()
}
