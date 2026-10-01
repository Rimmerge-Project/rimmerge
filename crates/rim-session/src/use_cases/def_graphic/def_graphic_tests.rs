use std::collections::{BTreeMap, BTreeSet};

use rim_analyzer::analysis::TextureIndex;
use rim_analyzer::domain::{LoadOrder, ModId};
use rim_analyzer::extract::graphics::{Facing, StuffAppearance};
use rim_merge::tree::FieldTree;

use super::*;

fn tree(xml: &str) -> FieldTree {
    rim_merge::xml::parse(xml).unwrap_or_else(|error| panic!("fixture xml: {error:?}\n{xml}"))
}

/// A synthetic install: texture owners, an order, and the facts a def's
/// rules need from outside itself.
struct Fixture {
    loose: BTreeMap<String, Vec<ModId>>,
    non_loose: BTreeSet<String>,
    is_core_trusted: bool,
    order: Vec<ModId>,
    body_types: Vec<String>,
    utility_layers: BTreeSet<String>,
    appearances: Vec<StuffAppearance>,
    kinds: Vec<(String, FieldTree)>,
}

impl Fixture {
    fn new() -> Self {
        Self {
            loose: BTreeMap::new(),
            non_loose: BTreeSet::new(),
            is_core_trusted: true,
            order: vec![ModId::new("a"), ModId::new("b")],
            body_types: vec!["Male".to_owned(), "Female".to_owned()],
            utility_layers: BTreeSet::from(["Backpack".to_owned()]),
            appearances: Vec::new(),
            kinds: Vec::new(),
        }
    }

    fn with_files(mut self, owner: &str, keys: &[&str]) -> Self {
        for key in keys {
            self.loose
                .entry((*key).to_owned())
                .or_default()
                .push(ModId::new(owner));
        }
        self
    }

    fn with_bundle(mut self, key: &str) -> Self {
        self.non_loose.insert(key.to_owned());
        self
    }

    fn with_kind(mut self, name: &str, xml: &str) -> Self {
        self.kinds.push((name.to_owned(), tree(xml)));
        self
    }

    fn resolve(&self, name: &str, xml: &str) -> DefGraphic {
        let def = tree(xml);
        let textures = TextureIndex::new(
            self.loose.clone(),
            self.non_loose.clone(),
            self.is_core_trusted,
        );
        let order = LoadOrder::new(self.order.clone());
        let kinds: Vec<DefSubject<'_>> = self
            .kinds
            .iter()
            .map(|(kind_name, kind_tree)| DefSubject {
                name: kind_name,
                tree: kind_tree,
            })
            .collect();
        let facts = SlotFacts {
            body_types: &self.body_types,
            utility_layers: &self.utility_layers,
            stuff_appearances: &self.appearances,
            race_kinds: &kinds,
        };
        let view = TextureView {
            textures: &textures,
            order: &order,
            stuff_appearances: &self.appearances,
        };
        resolve_def_graphic(&DefSubject { name, tree: &def }, &facts, &view)
    }

    fn set(&self, name: &str, xml: &str) -> GraphicSet {
        match self.resolve(name, xml) {
            DefGraphic::Resolved(set) => set,
            other => panic!("expected Resolved, got {other:?}"),
        }
    }
}

fn thing(graphic_data: &str) -> String {
    format!("<ThingDef><defName>X</defName><graphicData>{graphic_data}</graphicData></ThingDef>")
}

fn key_of(faces: &Faces, facing: Facing) -> String {
    faces.face(facing).key.to_string()
}

fn labels(slot: &GraphicSlot) -> Vec<VariantLabel> {
    slot.variants().iter().map(|v| v.label.clone()).collect()
}

fn sources(set: &GraphicSet) -> Vec<SlotSource> {
    set.slots()
        .iter()
        .map(|slot| slot.source().clone())
        .collect()
}

// ---- value types -------------------------------------------------------

#[test]
fn texture_key_normalizes_case_and_separators() {
    let key = TextureKey::parse("Things\\Item\\Foo");
    assert_eq!(key.map(|k| k.to_string()), Ok("things/item/foo".to_owned()));
}

#[test]
fn texture_key_rejects_every_unsafe_shape() {
    let long = "a".repeat(TextureKey::MAX_BYTES + 1);
    assert_eq!(TextureKey::parse(""), Err(TextureKeyError::Empty));
    assert_eq!(TextureKey::parse("/a"), Err(TextureKeyError::LeadingSlash));
    assert_eq!(
        TextureKey::parse("a/../b"),
        Err(TextureKeyError::RelativeSegment)
    );
    assert_eq!(
        TextureKey::parse("a//b"),
        Err(TextureKeyError::EmptySegment)
    );
    assert_eq!(TextureKey::parse("a/"), Err(TextureKeyError::EmptySegment));
    assert_eq!(
        TextureKey::parse("a/{x}"),
        Err(TextureKeyError::FormatCharacter)
    );
    assert_eq!(
        TextureKey::parse("a[0]"),
        Err(TextureKeyError::FormatCharacter)
    );
    assert_eq!(
        TextureKey::parse(&long),
        Err(TextureKeyError::TooLong(TextureKey::MAX_BYTES + 1))
    );
}

fn variant(label: VariantLabel) -> GraphicVariant {
    let key = TextureKey::parse("a/b");
    let Ok(key) = key else { panic!("fixture key") };
    GraphicVariant {
        label,
        faces: Faces::Single(Face {
            key,
            is_mirrored: false,
            availability: Availability::NotFound,
        }),
    }
}

fn slot_of(count: usize, default: usize) -> Result<GraphicSlot, GraphicModelError> {
    let variants = (0..count).map(|_| variant(VariantLabel::Only)).collect();
    GraphicSlot::new(SlotSource::Graphic, None, false, variants, default)
}

#[test]
fn a_slot_rejects_no_variants_and_an_out_of_range_default() {
    assert_eq!(slot_of(0, 0), Err(GraphicModelError::NoVariants));
    assert_eq!(
        slot_of(2, 2),
        Err(GraphicModelError::DefaultVariantOutOfRange { index: 2 })
    );
    assert!(slot_of(2, 1).is_ok());
}

#[test]
fn a_set_rejects_no_slots_and_an_out_of_range_default() {
    assert_eq!(
        GraphicSet::new(Vec::new(), 0, 0),
        Err(GraphicModelError::NoSlots)
    );
    let Ok(slot) = slot_of(2, 1) else {
        panic!("slot")
    };
    assert_eq!(
        GraphicSet::new(vec![slot.clone()], 1, 0),
        Err(GraphicModelError::DefaultSlotOutOfRange { index: 1 })
    );
    let set = GraphicSet::new(vec![slot], 0, 3);
    assert_eq!(
        set.map(|s| (s.default_view(), s.truncated_slots())),
        Ok((
            ViewRef {
                slot: 0,
                variant: 1
            },
            3
        ))
    );
}

// ---- graphicData -------------------------------------------------------

#[test]
fn graphic_data_multi_resolves_four_faces_and_defaults_to_south() {
    let fx = Fixture::new().with_files(
        "a",
        &[
            "things/x_north",
            "things/x_east",
            "things/x_south",
            "things/x_west",
        ],
    );
    let set = fx.set(
        "X",
        &thing("<texPath>Things/X</texPath><graphicClass>Graphic_Multi</graphicClass>"),
    );

    let slot = &set.slots()[0];
    assert_eq!(slot.source(), &SlotSource::Graphic);
    assert_eq!(slot.graphic_class(), Some("Graphic_Multi"));
    let faces = &slot.variants()[0].faces;
    assert!(faces.is_directional());
    assert_eq!(faces.default_face().key.as_str(), "things/x_south");
    assert_eq!(key_of(faces, Facing::West), "things/x_west");
    assert_eq!(
        set.default_view(),
        ViewRef {
            slot: 0,
            variant: 0
        }
    );
}

#[test]
fn a_missing_west_face_is_the_east_texture_mirrored() {
    let fx = Fixture::new().with_files("a", &["things/x_north", "things/x_east", "things/x_south"]);
    let set = fx.set(
        "X",
        &thing("<texPath>things/x</texPath><graphicClass>Graphic_Multi</graphicClass>"),
    );

    let west = set.slots()[0].variants()[0]
        .faces
        .face(Facing::West)
        .clone();
    assert_eq!(west.key.as_str(), "things/x_east");
    assert!(west.is_mirrored);
}

#[test]
fn allow_flip_false_keeps_the_substituted_face_unmirrored() {
    let fx = Fixture::new().with_files("a", &["things/x_north", "things/x_east", "things/x_south"]);
    let set = fx.set(
        "X",
        &thing(
            "<texPath>things/x</texPath><graphicClass>Graphic_Multi</graphicClass><allowFlip>false</allowFlip>",
        ),
    );

    let west = set.slots()[0].variants()[0]
        .faces
        .face(Facing::West)
        .clone();
    assert!(!west.is_mirrored);
}

#[test]
fn graphic_single_has_no_facing_control() {
    let fx = Fixture::new().with_files("a", &["things/x"]);
    let set = fx.set(
        "X",
        &thing("<texPath>things/x</texPath><graphicClass>Graphic_Single</graphicClass>"),
    );

    assert!(!set.slots()[0].variants()[0].faces.is_directional());
}

#[test]
fn graphic_data_without_a_class_shows_nothing() {
    let fx = Fixture::new().with_files("a", &["things/x"]);

    assert_eq!(
        fx.resolve("X", &thing("<texPath>things/x</texPath>")),
        DefGraphic::NoGraphic
    );
}

#[test]
fn a_multi_with_no_file_keeps_its_base_key_as_not_found() {
    let fx = Fixture::new();
    let set = fx.set(
        "X",
        &thing("<texPath>things/x</texPath><graphicClass>Graphic_Multi</graphicClass>"),
    );

    let face = set.slots()[0].variants()[0].faces.default_face().clone();
    assert_eq!(face.key.as_str(), "things/x");
    assert_eq!(face.availability, Availability::NotFound);
}

#[test]
fn a_random_collection_lists_each_member_in_name_order() {
    let fx = Fixture::new().with_files("a", &["things/p/b", "things/p/a", "things/p/a_m"]);
    let set = fx.set(
        "X",
        &thing("<texPath>things/p</texPath><graphicClass>Graphic_Random</graphicClass>"),
    );

    let slot = &set.slots()[0];
    assert_eq!(
        labels(slot),
        vec![
            VariantLabel::Member {
                index: 0,
                count: 2,
                name: "a".to_owned()
            },
            VariantLabel::Member {
                index: 1,
                count: 2,
                name: "b".to_owned()
            },
        ]
    );
    assert_eq!(slot.truncated(), 0);
}

#[test]
fn a_stack_count_collection_labels_members_by_stack_position() {
    let fx = Fixture::new().with_files("a", &["things/p/s_a", "things/p/s_b", "things/p/s_c"]);
    let set = fx.set(
        "X",
        &thing("<texPath>things/p</texPath><graphicClass>Verse.Graphic_StackCount</graphicClass>"),
    );

    assert_eq!(
        labels(&set.slots()[0]),
        vec![
            VariantLabel::Stack { index: 0, count: 3 },
            VariantLabel::Stack { index: 1, count: 3 },
            VariantLabel::Stack { index: 2, count: 3 },
        ]
    );
    assert_eq!(set.default_view().variant, 0);
}

#[test]
fn a_collection_beyond_the_bound_reports_what_it_dropped() {
    let keys: Vec<String> = (0..70).map(|i| format!("things/p/m{i:03}")).collect();
    let refs: Vec<&str> = keys.iter().map(String::as_str).collect();
    let fx = Fixture::new().with_files("a", &refs);
    let set = fx.set(
        "X",
        &thing("<texPath>things/p</texPath><graphicClass>Graphic_Random</graphicClass>"),
    );

    assert_eq!(set.slots()[0].variants().len(), 64);
    assert_eq!(set.slots()[0].truncated(), 6);
}

#[test]
fn an_unmodelled_class_is_laid_out_from_the_files_and_flagged() {
    let fx = Fixture::new().with_files("a", &["things/x_south"]);
    let set = fx.set(
        "X",
        &thing("<texPath>things/x</texPath><graphicClass>Example.Graphic_Custom</graphicClass>"),
    );

    let slot = &set.slots()[0];
    assert!(slot.is_layout_inferred());
    assert!(slot.variants()[0].faces.is_directional());
}

#[test]
fn appearances_list_one_variant_per_stuff_appearance() {
    let mut fx = Fixture::new().with_files("a", &["things/w/w_smooth", "things/w/w_bricks"]);
    fx.appearances = vec![
        StuffAppearance {
            def_name: "Smooth".to_owned(),
            path_prefix: None,
        },
        StuffAppearance {
            def_name: "Bricks".to_owned(),
            path_prefix: None,
        },
        StuffAppearance {
            def_name: "Planks".to_owned(),
            path_prefix: None,
        },
    ];
    let set = fx.set(
        "X",
        &thing("<texPath>things/w</texPath><graphicClass>Graphic_Appearances</graphicClass>"),
    );

    let slot = &set.slots()[0];
    assert_eq!(slot.variants().len(), 3);
    let planks = slot.variants()[2].faces.default_face().key.to_string();
    assert_eq!(planks, "things/w/w_smooth");
    assert_eq!(
        slot.variants()[1].label,
        VariantLabel::Appearance("bricks".to_owned())
    );
}

// ---- apparel -----------------------------------------------------------

fn apparel(worn: &str, layers: &[&str], extra: &str) -> String {
    let layers: String = layers.iter().map(|l| format!("<li>{l}</li>")).collect();
    format!(
        "<ThingDef><defName>X</defName><apparel><wornGraphicPath>{worn}</wornGraphicPath><layers>{layers}</layers>{extra}</apparel></ThingDef>"
    )
}

#[test]
fn apparel_has_one_variant_per_body_type_and_opens_on_the_first_located() {
    let fx =
        Fixture::new().with_files("a", &["things/hat_female_south", "things/hat_female_north"]);
    let mut fx = fx;
    fx.body_types = vec!["Male".to_owned(), "Female".to_owned()];
    let set = fx.set("X", &apparel("Things/Hat", &["Shell"], ""));

    let slot = &set.slots()[0];
    assert_eq!(slot.source(), &SlotSource::Worn);
    assert_eq!(
        labels(slot),
        vec![
            VariantLabel::BodyType("Male".to_owned()),
            VariantLabel::BodyType("Female".to_owned())
        ]
    );
    assert_eq!(slot.default_variant(), 1);
    assert_eq!(
        key_of(&slot.variants()[0].faces, Facing::South),
        "things/hat_male"
    );
}

#[test]
fn apparel_without_any_located_body_type_opens_on_the_first_in_registration_order() {
    let set = Fixture::new().set("X", &apparel("Things/Hat", &["Shell"], ""));

    assert_eq!(set.slots()[0].default_variant(), 0);
    assert_eq!(
        set.slots()[0].variants()[0]
            .faces
            .default_face()
            .availability,
        Availability::NotFound
    );
}

#[test]
fn headgear_whose_last_layer_is_overhead_takes_no_body_type_suffix() {
    let set = Fixture::new()
        .with_files("a", &["things/hat_south"])
        .set("X", &apparel("Things/Hat", &["Middle", "Overhead"], ""));

    assert_eq!(labels(&set.slots()[0]), vec![VariantLabel::Only]);
    assert_eq!(
        key_of(&set.slots()[0].variants()[0].faces, Facing::South),
        "things/hat_south"
    );
}

#[test]
fn only_the_last_layer_decides_the_exemption() {
    let set = Fixture::new().set("X", &apparel("Things/Hat", &["Overhead", "Shell"], ""));

    assert_eq!(set.slots()[0].variants().len(), 2);
}

#[test]
fn eye_cover_is_exempt_too() {
    let set = Fixture::new().set("X", &apparel("Things/Glasses", &["EyeCover"], ""));

    assert_eq!(labels(&set.slots()[0]), vec![VariantLabel::Only]);
}

#[test]
fn a_pack_rendered_utility_layer_is_exempt_unless_the_data_opts_out() {
    let packed = Fixture::new().set("X", &apparel("Things/Pack", &["Backpack"], ""));
    assert_eq!(labels(&packed.slots()[0]), vec![VariantLabel::Only]);

    let not_packed = Fixture::new().set(
        "X",
        &apparel(
            "Things/Pack",
            &["Backpack"],
            "<wornGraphicData><renderUtilityAsPack>false</renderUtilityAsPack></wornGraphicData>",
        ),
    );
    assert_eq!(not_packed.slots()[0].variants().len(), 2);
}

#[test]
fn a_placeholder_worn_path_is_exempt() {
    let set = Fixture::new().set("X", &apparel("PlaceholderImage", &["Shell"], ""));

    assert_eq!(labels(&set.slots()[0]), vec![VariantLabel::Only]);
}

#[test]
fn several_worn_paths_become_labelled_variants() {
    let xml = "<ThingDef><defName>X</defName><apparel><wornGraphicPaths><li>Things/A</li><li>Things/B</li></wornGraphicPaths><layers><li>Overhead</li></layers></apparel></ThingDef>";
    let set = Fixture::new().set("X", xml);

    assert_eq!(
        labels(&set.slots()[0]),
        vec![
            VariantLabel::WornPath {
                index: 0,
                count: 2,
                body_type: None
            },
            VariantLabel::WornPath {
                index: 1,
                count: 2,
                body_type: None
            },
        ]
    );
}

#[test]
fn suffixed_apparel_with_no_body_types_has_no_worn_slot() {
    let mut fx = Fixture::new();
    fx.body_types.clear();

    assert_eq!(
        fx.resolve("X", &apparel("Things/Hat", &["Shell"], "")),
        DefGraphic::NoGraphic
    );
}

#[test]
fn the_worn_slot_follows_the_ground_graphic() {
    let xml = "<ThingDef><defName>X</defName><graphicData><texPath>things/g</texPath><graphicClass>Graphic_Single</graphicClass></graphicData><apparel><wornGraphicPath>Things/Hat</wornGraphicPath><layers><li>Overhead</li></layers></apparel></ThingDef>";
    let set = Fixture::new().set("X", xml);

    assert_eq!(sources(&set), vec![SlotSource::Graphic, SlotSource::Worn]);
}

// ---- pawn kinds --------------------------------------------------------

const KIND: &str = "<PawnKindDef><defName>Cow</defName><race>Cow</race><lifeStages>\
<li><bodyGraphicData><texPath>animals/calf</texPath></bodyGraphicData></li>\
<li><bodyGraphicData><texPath>animals/cow</texPath><graphicClass>Graphic_Multi</graphicClass></bodyGraphicData>\
<femaleGraphicData><texPath>animals/cow_f</texPath></femaleGraphicData></li>\
</lifeStages><alternateGraphics><li><texPath>animals/cow_alt</texPath></li></alternateGraphics></PawnKindDef>";

#[test]
fn a_pawn_kind_opens_on_its_adult_stage() {
    let fx = Fixture::new().with_files("a", &["animals/calf_south", "animals/cow_south"]);
    let set = fx.set("Cow", KIND);

    let slot = &set.slots()[0];
    assert_eq!(slot.source(), &SlotSource::LifeStage);
    assert_eq!(slot.default_variant(), 1);
    assert_eq!(
        labels(slot),
        vec![
            VariantLabel::LifeStage { index: 0, count: 2 },
            VariantLabel::LifeStage { index: 1, count: 2 },
            VariantLabel::Female { stage: 1 },
            VariantLabel::Alternate { index: 0 },
        ]
    );
    assert_eq!(
        key_of(&slot.variants()[1].faces, Facing::South),
        "animals/cow_south"
    );
}

#[test]
fn a_stage_without_a_graphic_class_is_read_as_multi() {
    let fx = Fixture::new().with_files("a", &["animals/calf_south"]);
    let set = fx.set("Cow", KIND);

    assert!(set.slots()[0].variants()[0].faces.is_directional());
}

#[test]
fn an_alternate_takes_the_adult_stages_class() {
    let fx = Fixture::new().with_files("a", &["animals/cow_alt_south"]);
    let set = fx.set("Cow", KIND);

    let alternate = &set.slots()[0].variants()[3];
    assert!(alternate.faces.is_directional());
    assert_eq!(
        alternate.faces.default_face().key.as_str(),
        "animals/cow_alt_south"
    );
}

// ---- style items, head, body, terrain, icon ----------------------------

#[test]
fn a_style_item_texture_is_a_multi() {
    let fx = Fixture::new().with_files("a", &["hair/bob_south"]);
    let set = fx.set(
        "Bob",
        "<HairDef><defName>Bob</defName><texPath>Hair/Bob</texPath></HairDef>",
    );

    assert_eq!(sources(&set), vec![SlotSource::StyleTexture]);
    assert!(set.slots()[0].variants()[0].faces.is_directional());
}

#[test]
fn a_no_graphic_style_item_has_no_texture_slot() {
    let xml = "<HairDef><defName>Bald</defName><noGraphic>true</noGraphic><texPath>Hair/Bald</texPath></HairDef>";

    assert_eq!(Fixture::new().resolve("Bald", xml), DefGraphic::NoGraphic);
}

#[test]
fn head_body_and_terrain_fields_each_make_their_slot() {
    let fx = Fixture::new().with_files("a", &["heads/h_south", "bodies/b_south", "terrain/t"]);

    let head = fx.set(
        "H",
        "<HeadTypeDef><defName>H</defName><graphicPath>Heads/H</graphicPath></HeadTypeDef>",
    );
    let body = fx.set("B", "<BodyTypeDef><defName>B</defName><bodyNakedGraphicPath>Bodies/B</bodyNakedGraphicPath></BodyTypeDef>");
    let terrain = fx.set(
        "T",
        "<TerrainDef><defName>T</defName><texturePath>Terrain/T</texturePath></TerrainDef>",
    );

    assert_eq!(sources(&head), vec![SlotSource::HeadType]);
    assert!(head.slots()[0].variants()[0].faces.is_directional());
    assert_eq!(sources(&body), vec![SlotSource::BodyType]);
    assert_eq!(sources(&terrain), vec![SlotSource::Terrain]);
    assert!(!terrain.slots()[0].variants()[0].faces.is_directional());
    assert!(terrain.slots()[0].variants()[0].faces.is_located());
}

#[test]
fn icon_fields_make_one_labelled_slot() {
    let xml = "<ThingDef><defName>X</defName><uiIconPath>Ui/X</uiIconPath><iconPath>Ui/Y</iconPath></ThingDef>";
    let set = Fixture::new().with_files("a", &["ui/x"]).set("X", xml);

    assert_eq!(sources(&set), vec![SlotSource::Icon]);
    assert_eq!(
        labels(&set.slots()[0]),
        vec![
            VariantLabel::Field("uiIconPath".to_owned()),
            VariantLabel::Field("iconPath".to_owned())
        ]
    );
}

// ---- race -> kinds, humanlike ------------------------------------------

const RACE: &str = "<ThingDef><defName>Cow</defName><race><body>Quadruped</body></race></ThingDef>";

#[test]
fn a_race_shows_the_pawn_kinds_that_use_it() {
    let fx = Fixture::new()
        .with_files("a", &["animals/calf_south", "animals/cow_south"])
        .with_kind("Cow", KIND);
    let set = fx.set("Cow", RACE);

    assert_eq!(
        sources(&set),
        vec![SlotSource::RaceKind {
            kind: "Cow".to_owned()
        }]
    );
    assert_eq!(set.slots()[0].default_variant(), 1);
}

#[test]
fn a_kind_whose_effective_race_names_another_def_is_dropped() {
    let other = KIND.replace("<race>Cow</race>", "<race>Horse</race>");
    let fx = Fixture::new()
        .with_files("a", &["animals/cow_south"])
        .with_kind("Cow", &other);

    assert_eq!(fx.resolve("Cow", RACE), DefGraphic::NoGraphic);
}

#[test]
fn a_def_with_its_own_graphic_does_not_borrow_its_kinds() {
    let xml = "<ThingDef><defName>Cow</defName><graphicData><texPath>animals/own</texPath><graphicClass>Graphic_Single</graphicClass></graphicData><race><body>Quadruped</body></race></ThingDef>";
    let fx = Fixture::new().with_kind("Cow", KIND);

    assert_eq!(sources(&fx.set("Cow", xml)), vec![SlotSource::Graphic]);
}

const HUMANLIKE: &str = "<ThingDef><defName>Human</defName><race><intelligence>Humanlike</intelligence></race></ThingDef>";

#[test]
fn a_humanlike_race_with_nothing_to_show_is_composed_at_runtime() {
    assert_eq!(
        Fixture::new().resolve("Human", HUMANLIKE),
        DefGraphic::ComposedAtRuntime
    );
}

#[test]
fn a_non_humanlike_race_with_nothing_to_show_has_no_graphic() {
    assert_eq!(Fixture::new().resolve("Cow", RACE), DefGraphic::NoGraphic);
}

#[test]
fn a_humanlike_race_with_a_resolving_probe_slot_is_shown_not_composed() {
    let xml = "<ThingDef><defName>Human</defName><race><intelligence>Humanlike</intelligence></race><modExtensions><li><bodies>Custom/Bodies</bodies></li></modExtensions></ThingDef>";
    let fx = Fixture::new().with_files("a", &["custom/bodies/b_south"]);

    let set = fx.set("Human", xml);

    assert!(set.slots()[0].source().is_probe());
}

// ---- probe -------------------------------------------------------------

#[test]
fn a_probe_groups_a_folder_valued_field_and_is_labelled_with_it() {
    let xml = "<ThingDef><defName>X</defName><comps><li><folder>Things/Fox</folder></li></comps></ThingDef>";
    let fx = Fixture::new().with_files(
        "a",
        &[
            "things/fox/pup_east",
            "things/fox/pup_south",
            "things/fox/old",
        ],
    );
    let set = fx.set("X", xml);

    let slot = &set.slots()[0];
    assert_eq!(
        slot.source(),
        &SlotSource::Probe {
            field: "comps/li/folder".to_owned()
        }
    );
    assert!(slot.is_layout_inferred());
    assert_eq!(slot.variants().len(), 2);
}

#[test]
fn a_probe_ignores_format_strings_whitespace_and_unresolved_values() {
    let xml = "<ThingDef><defName>X</defName><a>Things/{x}</a><b>some words/here</b><c>Things/Nowhere</c></ThingDef>";
    let fx = Fixture::new().with_files("a", &["things/x", "some words/here"]);

    assert_eq!(fx.resolve("X", xml), DefGraphic::NoGraphic);
}

#[test]
fn a_probe_never_repeats_a_key_an_engine_slot_already_shows() {
    let xml = "<ThingDef><defName>X</defName><graphicData><texPath>things/x</texPath><graphicClass>Graphic_Single</graphicClass></graphicData><other>Things/X</other></ThingDef>";
    let set = Fixture::new().with_files("a", &["things/x"]).set("X", xml);

    assert_eq!(sources(&set), vec![SlotSource::Graphic]);
}

#[test]
fn a_probe_is_not_the_default_while_an_engine_slot_exists() {
    let xml = "<ThingDef><defName>X</defName><graphicData><texPath>things/missing</texPath><graphicClass>Graphic_Single</graphicClass></graphicData><other>Things/Found</other></ThingDef>";
    let set = Fixture::new()
        .with_files("a", &["things/found"])
        .set("X", xml);

    assert_eq!(set.slots().len(), 2);
    assert_eq!(set.default_view().slot, 0);
}

#[test]
fn slots_beyond_the_bound_are_counted() {
    let leaves: String = (0..20)
        .map(|i| format!("<f{i}>things/t{i}</f{i}>"))
        .collect();
    let keys: Vec<String> = (0..20).map(|i| format!("things/t{i}")).collect();
    let refs: Vec<&str> = keys.iter().map(String::as_str).collect();
    let xml = format!("<ThingDef><defName>X</defName>{leaves}</ThingDef>");

    let set = Fixture::new().with_files("a", &refs).set("X", &xml);

    assert_eq!(set.slots().len(), MAX_SLOTS);
    assert_eq!(set.truncated_slots(), 4);
}

// ---- availability ------------------------------------------------------

fn view_of<'a>(textures: &'a TextureIndex, order: &'a LoadOrder) -> TextureView<'a> {
    TextureView {
        textures,
        order,
        stuff_appearances: &[],
    }
}

fn key(raw: &str) -> TextureKey {
    let Ok(key) = TextureKey::parse(raw) else {
        panic!("fixture key {raw}")
    };
    key
}

#[test]
fn the_last_loaded_shipper_wins_and_switching_the_order_flips_it() {
    let loose = BTreeMap::from([("t/x".to_owned(), vec![ModId::new("a"), ModId::new("b")])]);
    let textures = TextureIndex::new(loose, BTreeSet::new(), true);
    let current = LoadOrder::new(vec![ModId::new("a"), ModId::new("b")]);
    let suggested = LoadOrder::new(vec![ModId::new("b"), ModId::new("a")]);

    let under_current = availability(&key("t/x"), &view_of(&textures, &current));
    let under_suggested = availability(&key("t/x"), &view_of(&textures, &suggested));

    assert_eq!(
        under_current,
        Availability::Loose {
            owner: ModId::new("b")
        }
    );
    assert_eq!(
        under_suggested,
        Availability::Loose {
            owner: ModId::new("a")
        }
    );
}

#[test]
fn a_bundle_only_key_is_base_game_or_bundle() {
    let fx = Fixture::new().with_bundle("t/core");
    let set = fx.set(
        "X",
        &thing("<texPath>t/core</texPath><graphicClass>Graphic_Single</graphicClass>"),
    );

    assert_eq!(
        set.slots()[0].variants()[0]
            .faces
            .default_face()
            .availability,
        Availability::BaseGameOrBundle
    );
}

#[test]
fn a_missing_key_is_not_found_with_a_trusted_core_index_and_unknown_without() {
    let xml = thing("<texPath>t/none</texPath><graphicClass>Graphic_Single</graphicClass>");
    let trusted = Fixture::new().set("X", &xml);
    let mut untrusted = Fixture::new();
    untrusted.is_core_trusted = false;
    let untrusted = untrusted.set("X", &xml);

    let first = |set: &GraphicSet| {
        set.slots()[0].variants()[0]
            .faces
            .default_face()
            .availability
            .clone()
    };
    assert_eq!(first(&trusted), Availability::NotFound);
    assert_eq!(first(&untrusted), Availability::Unknown);
}
