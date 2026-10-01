//! Tests for the use cases over a `Session`: fact gathering, the cache,
//! and the key-allowlisted texture read.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use rim_analyzer::analysis::TextureIndex;
use rim_analyzer::domain::{
    Conflict, DefEntry, LoadOrder, ModId, Selector, TemplateEntry, UndecodableTexture,
};
use rim_resolve::domain::{DefKey, DefRef, OrderSource, PairRule, Rule, RuleOrigin};

use super::*;
use crate::Session;
use crate::ports::{DefSourceError, TextureBytes, TextureFormat};
use crate::test_support::{
    FakeAssetLocator, InMemoryDefSourceReader, locator, session_with_sources_and_mods,
};
use crate::use_cases::{InspectDefError, PreviewUnreadable};

/// One synthetic install: mods in load order, their defs and templates,
/// texture ownership, and report conflicts.
struct Install {
    mods: Vec<&'static str>,
    defs: Vec<(&'static str, &'static str, &'static str, String)>,
    templates: Vec<(&'static str, &'static str, &'static str, String)>,
    loose: BTreeMap<String, Vec<ModId>>,
    non_loose: BTreeSet<String>,
    is_core_trusted: bool,
    kinds_by_race: BTreeMap<String, Vec<String>>,
    conflicts: Vec<Conflict>,
}

impl Install {
    fn new(mods: &[&'static str]) -> Self {
        Self {
            mods: mods.to_vec(),
            defs: Vec::new(),
            templates: Vec::new(),
            loose: BTreeMap::new(),
            non_loose: BTreeSet::new(),
            is_core_trusted: true,
            kinds_by_race: BTreeMap::new(),
            conflicts: Vec::new(),
        }
    }

    fn def(
        mut self,
        owner: &'static str,
        def_type: &'static str,
        name: &'static str,
        xml: &str,
    ) -> Self {
        self.defs.push((owner, def_type, name, xml.to_owned()));
        self
    }

    fn template(
        mut self,
        owner: &'static str,
        def_type: &'static str,
        name: &'static str,
        xml: &str,
    ) -> Self {
        self.templates.push((owner, def_type, name, xml.to_owned()));
        self
    }

    fn file(mut self, owner: &str, key: &str) -> Self {
        self.loose
            .entry(key.to_owned())
            .or_default()
            .push(ModId::new(owner));
        self
    }

    fn build(self) -> (Session, InMemoryDefSourceReader) {
        let mut sources = rim_analyzer::analysis::SourceIndex::default();
        let mut elements = BTreeMap::new();
        for (index, (owner, def_type, name, xml)) in self.defs.iter().enumerate() {
            let at = locator(&format!("def{index}.xml"), 0);
            let parent_name = parent_name_of(xml);
            sources.defs.insert(
                (
                    ModId::new(*owner),
                    ((*def_type).to_owned(), (*name).to_owned()),
                ),
                vec![DefEntry {
                    def_type: (*def_type).to_owned(),
                    def_name: (*name).to_owned(),
                    may_require: Vec::new(),
                    may_require_any_of: Vec::new(),
                    parent_name,
                    locator: at.clone(),
                }],
            );
            sources
                .owners_by_def
                .entry(((*def_type).to_owned(), (*name).to_owned()))
                .or_default()
                .push(ModId::new(*owner));
            elements.insert(at, xml.clone());
        }
        for (index, (owner, def_type, name, xml)) in self.templates.iter().enumerate() {
            let at = locator(&format!("template{index}.xml"), 0);
            sources
                .templates
                .entry(((*def_type).to_owned(), (*name).to_owned()))
                .or_default()
                .push((
                    ModId::new(*owner),
                    TemplateEntry {
                        graphic_class: None,
                        may_require: Vec::new(),
                        def_type: (*def_type).to_owned(),
                        name: (*name).to_owned(),
                        parent_name: None,
                        is_abstract: true,
                        locator: at.clone(),
                    },
                ));
            elements.insert(at, xml.clone());
        }
        sources.textures = TextureIndex::new(self.loose, self.non_loose, self.is_core_trusted);
        sources.kinds_by_race = self.kinds_by_race;
        let mut report = {
            let mut builder = rim_resolve::test_support::ReportBuilder::new();
            for id in &self.mods {
                builder = builder.mod_with(id, |entry| {
                    entry.loaded_folders = vec![PathBuf::from(format!("Mods/{id}"))];
                });
            }
            builder.build()
        };
        report.conflicts.extend(self.conflicts);
        let session = session_with_sources_and_mods(sources, report, &self.mods);
        (session, InMemoryDefSourceReader::new(elements))
    }
}

fn parent_name_of(xml: &str) -> Option<String> {
    let marker = "ParentName=\"";
    let start = xml.find(marker)? + marker.len();
    let end = xml[start..].find('"')? + start;
    Some(xml[start..end].to_owned())
}

fn def_ref(def_type: &str, name: &str) -> DefRef {
    DefRef::new(
        DefKey {
            def_type: def_type.to_owned(),
            def_name: name.to_owned(),
        },
        Selector::DefName,
    )
}

fn key(raw: &str) -> TextureKey {
    let Ok(key) = TextureKey::parse(raw) else {
        panic!("fixture key {raw}")
    };
    key
}

fn resolved(graphic: &DefGraphic) -> &GraphicSet {
    match graphic {
        DefGraphic::Resolved(set) => set,
        other => panic!("expected Resolved, got {other:?}"),
    }
}

const SINGLE_THING: &str = "<ThingDef><defName>X</defName><graphicData><texPath>Things/X</texPath><graphicClass>Graphic_Single</graphicClass></graphicData></ThingDef>";

// ---- resolution --------------------------------------------------------

#[test]
fn a_things_graphic_resolves_through_the_effective_def() {
    let (mut session, reader) = Install::new(&["a"])
        .def("a", "ThingDef", "X", SINGLE_THING)
        .file("a", "things/x")
        .build();

    let graphic = ResolveDefGraphic::new(reader)
        .execute(&mut session, &def_ref("ThingDef", "X"))
        .cloned();

    let Ok(graphic) = graphic else {
        panic!("resolve failed")
    };
    let face = resolved(&graphic).slots()[0].variants()[0]
        .faces
        .default_face()
        .clone();
    assert_eq!(face.key.as_str(), "things/x");
    assert_eq!(
        face.availability,
        Availability::Loose {
            owner: ModId::new("a")
        }
    );
}

#[test]
fn a_graphic_class_inherited_from_a_parent_template_is_followed() {
    let (mut session, reader) = Install::new(&["a"])
        .template(
            "a",
            "ThingDef",
            "Base",
            "<ThingDef Name=\"Base\" Abstract=\"True\"><graphicData><graphicClass>Graphic_Multi</graphicClass></graphicData></ThingDef>",
        )
        .def(
            "a",
            "ThingDef",
            "X",
            "<ThingDef ParentName=\"Base\"><defName>X</defName><graphicData><texPath>Things/X</texPath></graphicData></ThingDef>",
        )
        .file("a", "things/x_south")
        .build();

    let graphic = ResolveDefGraphic::new(reader).execute(&mut session, &def_ref("ThingDef", "X"));

    let Ok(graphic) = graphic else {
        panic!("resolve failed")
    };
    assert!(
        resolved(graphic).slots()[0].variants()[0]
            .faces
            .is_directional()
    );
}

#[test]
fn an_unknown_def_is_an_inspect_not_found() {
    let (mut session, reader) = Install::new(&["a"]).build();

    let result = ResolveDefGraphic::new(reader).execute(&mut session, &def_ref("ThingDef", "Nope"));

    assert!(matches!(
        result,
        Err(ResolveDefGraphicError::Inspect(InspectDefError::NotFound(
            _
        )))
    ));
}

const APPAREL: &str = "<ThingDef><defName>Hat</defName><apparel><wornGraphicPath>Things/Hat</wornGraphicPath><layers><li>Shell</li></layers></apparel></ThingDef>";

fn body_type(name: &str) -> String {
    format!("<BodyTypeDef><defName>{name}</defName></BodyTypeDef>")
}

fn body_labels(mods: &[&'static str]) -> Vec<VariantLabel> {
    let (mut session, reader) = Install::new(mods)
        .def("a", "BodyTypeDef", "Male", &body_type("Male"))
        .def("a", "BodyTypeDef", "Female", &body_type("Female"))
        .def("b", "BodyTypeDef", "Thin", &body_type("Thin"))
        .def("b", "ThingDef", "Hat", APPAREL)
        .build();
    let graphic = ResolveDefGraphic::new(reader).execute(&mut session, &def_ref("ThingDef", "Hat"));
    let Ok(graphic) = graphic else {
        panic!("resolve failed")
    };
    resolved(graphic).slots()[0]
        .variants()
        .iter()
        .map(|v| v.label.clone())
        .collect()
}

#[test]
fn apparel_variants_follow_body_type_registration_order_by_mod_position() {
    let in_order = body_labels(&["a", "b"]);
    let reversed = body_labels(&["b", "a"]);

    let names = |labels: Vec<VariantLabel>| -> Vec<String> {
        labels
            .into_iter()
            .filter_map(|label| match label {
                VariantLabel::BodyType(name) => Some(name),
                _ => None,
            })
            .collect()
    };
    assert_eq!(names(in_order), ["Male", "Female", "Thin"]);
    assert_eq!(names(reversed), ["Thin", "Male", "Female"]);
}

#[test]
fn a_utility_layer_read_from_its_def_exempts_the_apparel_from_the_suffix() {
    let pack = "<ThingDef><defName>Pack</defName><apparel><wornGraphicPath>Things/Pack</wornGraphicPath><layers><li>Backpack</li></layers></apparel></ThingDef>";
    let (mut session, reader) = Install::new(&["a"])
        .def("a", "BodyTypeDef", "Male", &body_type("Male"))
        .def(
            "a",
            "ApparelLayerDef",
            "Backpack",
            "<ApparelLayerDef><defName>Backpack</defName><isUtilityLayer>true</isUtilityLayer></ApparelLayerDef>",
        )
        .def("a", "ThingDef", "Pack", pack)
        .build();

    let graphic =
        ResolveDefGraphic::new(reader).execute(&mut session, &def_ref("ThingDef", "Pack"));

    let Ok(graphic) = graphic else {
        panic!("resolve failed")
    };
    assert_eq!(
        resolved(graphic).slots()[0].variants()[0].label,
        VariantLabel::Only
    );
}

#[test]
fn a_race_shows_the_kinds_the_source_index_lists_for_it() {
    let kind = "<PawnKindDef><defName>CowKind</defName><race>Cow</race><lifeStages><li><bodyGraphicData><texPath>Animals/Cow</texPath></bodyGraphicData></li></lifeStages></PawnKindDef>";
    let race = "<ThingDef><defName>Cow</defName><race><body>Quadruped</body></race></ThingDef>";
    let mut install = Install::new(&["a"])
        .def("a", "PawnKindDef", "CowKind", kind)
        .def("a", "ThingDef", "Cow", race)
        .file("a", "animals/cow_south");
    install
        .kinds_by_race
        .insert("Cow".to_owned(), vec!["CowKind".to_owned()]);
    let (mut session, reader) = install.build();

    let graphic = ResolveDefGraphic::new(reader).execute(&mut session, &def_ref("ThingDef", "Cow"));

    let Ok(graphic) = graphic else {
        panic!("resolve failed")
    };
    assert_eq!(
        resolved(graphic).slots()[0].source(),
        &SlotSource::RaceKind {
            kind: "CowKind".to_owned()
        }
    );
}

#[test]
fn appearances_are_read_from_the_stuff_appearance_defs() {
    let wall = "<ThingDef><defName>Wall</defName><graphicData><texPath>Things/Wall</texPath><graphicClass>Graphic_Appearances</graphicClass></graphicData></ThingDef>";
    let (mut session, reader) = Install::new(&["a"])
        .def(
            "a",
            "StuffAppearanceDef",
            "Smooth",
            "<StuffAppearanceDef><defName>Smooth</defName></StuffAppearanceDef>",
        )
        .def(
            "a",
            "StuffAppearanceDef",
            "Bricks",
            "<StuffAppearanceDef><defName>Bricks</defName></StuffAppearanceDef>",
        )
        .def("a", "ThingDef", "Wall", wall)
        .file("a", "things/wall/w_smooth")
        .file("a", "things/wall/w_bricks")
        .build();

    let graphic =
        ResolveDefGraphic::new(reader).execute(&mut session, &def_ref("ThingDef", "Wall"));

    let Ok(graphic) = graphic else {
        panic!("resolve failed")
    };
    let labels: Vec<VariantLabel> = resolved(graphic).slots()[0]
        .variants()
        .iter()
        .map(|v| v.label.clone())
        .collect();
    assert_eq!(
        labels,
        [
            VariantLabel::Appearance("smooth".to_owned()),
            VariantLabel::Appearance("bricks".to_owned())
        ]
    );
}

// ---- cache -------------------------------------------------------------

#[test]
fn a_resolution_is_cached_and_cleared_when_the_order_changes() {
    let (mut session, reader) = Install::new(&["a"])
        .def("a", "ThingDef", "X", SINGLE_THING)
        .build();
    let use_case = ResolveDefGraphic::new(reader);
    let reference = def_ref("ThingDef", "X");

    assert!(use_case.execute(&mut session, &reference).is_ok());
    assert_eq!(session.cached_def_graphic_count(), 1);
    assert!(
        session
            .def_graphic(OrderSource::Current, &reference)
            .is_some()
    );

    session.set_current_order(LoadOrder::new(vec![ModId::new("a")]));

    assert_eq!(session.cached_def_graphic_count(), 0);
}

#[test]
fn a_resolution_under_suggested_is_dropped_when_a_rule_resorts() {
    let (mut session, reader) = Install::new(&["a", "b"])
        .def("a", "ThingDef", "X", SINGLE_THING)
        .build();
    let use_case = ResolveDefGraphic::new(reader);
    let reference = def_ref("ThingDef", "X");
    session.select(OrderSource::Suggested);
    assert!(use_case.execute(&mut session, &reference).is_ok());
    assert!(
        session
            .def_graphic(OrderSource::Suggested, &reference)
            .is_some(),
        "the Suggested resolution is cached before the resort"
    );

    // A new pair rule recomputes the sort (`Session::recompute_sort`), which
    // may move the Suggested order the cached resolution was built against.
    session.upsert_rule(Rule::Pair(PairRule {
        after: ModId::new("a"),
        before: ModId::new("b"),
        origin: RuleOrigin::UserDecision,
        comment: None,
        overrides_declared: false,
    }));

    assert_eq!(session.cached_def_graphic_count(), 0);
    assert!(
        session
            .def_graphic(OrderSource::Suggested, &reference)
            .is_none()
    );
}

#[test]
fn a_decision_free_selection_change_keeps_each_orders_own_entry() {
    let (mut session, reader) = Install::new(&["a"])
        .def("a", "ThingDef", "X", SINGLE_THING)
        .build();
    let use_case = ResolveDefGraphic::new(reader);
    let reference = def_ref("ThingDef", "X");

    assert!(use_case.execute(&mut session, &reference).is_ok());
    session.select(OrderSource::Suggested);
    assert!(use_case.execute(&mut session, &reference).is_ok());

    assert_eq!(session.cached_def_graphic_count(), 2);
}

#[test]
fn the_cache_clears_whole_once_it_would_pass_1024_entries() {
    let (mut session, _reader) = Install::new(&["a"]).build();
    for index in 0..1024 {
        session.cache_def_graphic(
            OrderSource::Current,
            def_ref("ThingDef", &format!("D{index}")),
            DefGraphic::NoGraphic,
        );
    }
    assert_eq!(session.cached_def_graphic_count(), 1024);

    session.cache_def_graphic(
        OrderSource::Current,
        def_ref("ThingDef", "Extra"),
        DefGraphic::NoGraphic,
    );

    assert_eq!(session.cached_def_graphic_count(), 1);
}

// ---- reading -----------------------------------------------------------

fn png() -> TextureBytes {
    TextureBytes {
        format: TextureFormat::Png,
        bytes: vec![0x89, 0x50, 0x4E, 0x47],
    }
}

fn read(
    install: Install,
    locator: FakeAssetLocator,
    texture_key: &str,
) -> Result<DefTexture, ReadDefTextureError> {
    let (mut session, reader) = install.build();
    ReadDefTexture::new(reader, locator).execute(
        &mut session,
        &def_ref("ThingDef", "X"),
        &key(texture_key),
    )
}

fn thing_install() -> Install {
    Install::new(&["a"])
        .def("a", "ThingDef", "X", SINGLE_THING)
        .file("a", "things/x")
}

fn locator_for(path: &str) -> FakeAssetLocator {
    FakeAssetLocator::new(BTreeMap::from([(
        "things/x".to_owned(),
        PathBuf::from(path),
    )]))
}

#[test]
fn a_png_the_game_loads_is_shown_directly() {
    let locator = locator_for("Mods/a/Textures/Things/X.png")
        .with_bytes(PathBuf::from("Mods/a/Textures/Things/X.png"), png());

    let texture = read(thing_install(), locator, "things/x");

    assert_eq!(
        texture,
        Ok(DefTexture::Image {
            texture: png(),
            owner: ModId::new("a"),
            from: ImageSource::Direct
        })
    );
}

#[test]
fn a_dds_with_a_png_beside_it_shows_the_png_labelled_as_a_sibling() {
    let sibling = PathBuf::from("Mods/a/Textures/Things/X.png");
    let locator = locator_for("Mods/a/Textures/Things/X.dds")
        .with_non_dds("things/x", sibling.clone())
        .with_bytes(sibling, png());

    let texture = read(thing_install(), locator, "things/x");

    assert_eq!(
        texture,
        Ok(DefTexture::Image {
            texture: png(),
            owner: ModId::new("a"),
            from: ImageSource::PngSibling
        })
    );
}

#[test]
fn a_dds_with_no_image_copy_is_not_previewable_and_names_its_owner() {
    let locator = locator_for("Mods/a/Textures/Things/X.dds");

    let texture = read(thing_install(), locator, "things/x");

    assert_eq!(
        texture,
        Ok(DefTexture::DdsNotPreviewable {
            owner: ModId::new("a")
        })
    );
}

#[test]
fn a_dds_the_game_cannot_decode_is_reported_even_with_a_png_beside_it() {
    let mut install = thing_install();
    install
        .conflicts
        .push(Conflict::UndecodableTexture(UndecodableTexture {
            mod_id: ModId::new("a"),
            path: "things/x".to_owned(),
            width: 6,
            height: 6,
            fourcc: "DXT5".to_owned(),
            has_png_sibling: true,
        }));
    let sibling = PathBuf::from("Mods/a/Textures/Things/X.png");
    let locator = locator_for("Mods/a/Textures/Things/X.dds")
        .with_non_dds("things/x", sibling.clone())
        .with_bytes(sibling, png());

    let texture = read(install, locator, "things/x");

    assert_eq!(
        texture,
        Ok(DefTexture::UndecodableInGame {
            owner: ModId::new("a")
        })
    );
}

#[test]
fn a_key_the_def_did_not_resolve_to_is_refused() {
    let locator = FakeAssetLocator::new(BTreeMap::from([(
        "things/other".to_owned(),
        PathBuf::from("Mods/a/Textures/Things/Other.png"),
    )]))
    .with_bytes(PathBuf::from("Mods/a/Textures/Things/Other.png"), png());

    let result = read(
        thing_install().file("a", "things/other"),
        locator,
        "things/other",
    );

    assert_eq!(
        result,
        Err(ReadDefTextureError::KeyNotInGraphic(key("things/other")))
    );
}

#[test]
fn a_def_that_shows_nothing_refuses_every_key() {
    let (mut session, reader) = Install::new(&["a"])
        .def(
            "a",
            "ThingDef",
            "X",
            "<ThingDef><defName>X</defName></ThingDef>",
        )
        .build();

    let result = ReadDefTexture::new(reader, FakeAssetLocator::default()).execute(
        &mut session,
        &def_ref("ThingDef", "X"),
        &key("things/x"),
    );

    assert!(matches!(
        result,
        Err(ReadDefTextureError::KeyNotInGraphic(_))
    ));
}

#[test]
fn a_bundle_only_texture_is_not_viewable() {
    let mut install = Install::new(&["a"]).def("a", "ThingDef", "X", SINGLE_THING);
    install.non_loose.insert("things/x".to_owned());

    let texture = read(install, FakeAssetLocator::default(), "things/x");

    assert_eq!(
        texture,
        Ok(DefTexture::NotViewable {
            is_uncertain: false
        })
    );
}

#[test]
fn a_missing_texture_is_not_found_or_uncertain_by_index_trust() {
    let trusted = Install::new(&["a"]).def("a", "ThingDef", "X", SINGLE_THING);
    let mut untrusted = Install::new(&["a"]).def("a", "ThingDef", "X", SINGLE_THING);
    untrusted.is_core_trusted = false;

    assert_eq!(
        read(trusted, FakeAssetLocator::default(), "things/x"),
        Ok(DefTexture::NotFound)
    );
    assert_eq!(
        read(untrusted, FakeAssetLocator::default(), "things/x"),
        Ok(DefTexture::NotViewable { is_uncertain: true })
    );
}

#[test]
fn an_unreadable_file_reports_why() {
    let path = PathBuf::from("Mods/a/Textures/Things/X.png");
    let locator = locator_for("Mods/a/Textures/Things/X.png").with_read_error(
        path.clone(),
        DefSourceError::TooLarge {
            file: path,
            max_bytes: 1,
            actual_bytes: 2,
        },
    );

    let texture = read(thing_install(), locator, "things/x");

    assert_eq!(
        texture,
        Ok(DefTexture::Unreadable(PreviewUnreadable::TooLarge))
    );
}

#[test]
fn reading_for_an_unknown_def_is_an_inspect_not_found() {
    let (mut session, reader) = Install::new(&["a"]).build();

    let result = ReadDefTexture::new(reader, FakeAssetLocator::default()).execute(
        &mut session,
        &def_ref("ThingDef", "Nope"),
        &key("things/x"),
    );

    assert!(matches!(
        result,
        Err(ReadDefTextureError::Resolve(
            ResolveDefGraphicError::Inspect(InspectDefError::NotFound(_))
        ))
    ));
}
