//! Def-override scenarios: whole-def overrides, structural triggers, and the flat/tree bionic-heart
//! family.

use std::collections::{BTreeMap, BTreeSet};

use rim_analyzer::analysis::{IndexedPatchOp, SourceIndex};
use rim_analyzer::domain::{
    DefEntry, GeneratedMarker, ModId, PatchOp, Report, Selector, TemplateEntry, XmlLocator,
};

use crate::test_support::InMemoryDefSourceReader;
use crate::test_support::{locator, report_fixture};

// `pub(crate)`, not private: a test proving `InspectDef`'s own effective
// def genuinely agrees with an independent `rim_merge::effective::compute`
// call fed these same raw texts needs to parse them itself, from
// `use_cases::inspect_def`'s own test module.
pub(crate) const CORE_BIONIC_HEART_XML: &str = r#"<HediffDef ParentName="AddedBodyPartBase">
  <defName>BionicHeart</defName>
  <label>bionic heart</label>
  <labelNoun>a bionic heart</labelNoun>
  <description>An installed bionic heart. It has synthetic muscle fibers for a realistic heartbeat, plus a high-flow pump for rapid circulation during high stress. It is better than a biological heart in almost every way.</description>
  <descriptionHyperlinks><ThingDef>BionicHeart</ThingDef></descriptionHyperlinks>
  <spawnThingOnRemoved>BionicHeart</spawnThingOnRemoved>
  <addedPartProps>
    <solid>true</solid>
    <partEfficiency>1.25</partEfficiency>
    <betterThanNatural>true</betterThanNatural>
  </addedPartProps>
</HediffDef>"#;

pub(crate) const BIONICS_BIONIC_HEART_XML: &str = r#"<HediffDef ParentName="addedPartExampleSynth">
  <defName>BionicHeart</defName>
  <label>synthetic heart</label>
  <labelNoun>a synthetic heart</labelNoun>
  <description>An installed synthetic heart. It has synthetic muscle fibers for a realistic heartbeat, plus a high-flow pump for rapid circulation during high stress. It is better than a biological heart in almost every way.</description>
  <descriptionHyperlinks><ThingDef>BionicHeart</ThingDef></descriptionHyperlinks>
  <spawnThingOnRemoved>BionicHeart</spawnThingOnRemoved>
  <addedPartProps>
    <solid>true</solid>
    <partEfficiency>1.25</partEfficiency>
    <betterThanNatural>true</betterThanNatural>
  </addedPartProps>
</HediffDef>"#;

pub(crate) const IMPLANT_HEDIFF_BASE_XML: &str = r#"<HediffDef Name="ImplantHediffBase" Abstract="True">
  <hediffClass>Hediff_Implant</hediffClass>
  <defaultLabelColor>(0.6, 0.6, 1.0)</defaultLabelColor>
  <isBad>false</isBad>
  <priceImpact>true</priceImpact>
  <countsAsAddedPartOrImplant>true</countsAsAddedPartOrImplant>
  <allowMothballIfLowPriorityWorldPawn>true</allowMothballIfLowPriorityWorldPawn>
</HediffDef>"#;

pub(crate) const ADDED_BODY_PART_BASE_XML: &str = r#"<HediffDef Name="AddedBodyPartBase" ParentName="ImplantHediffBase" Abstract="True">
  <hediffClass>Hediff_AddedPart</hediffClass>
  <priceImpact>true</priceImpact>
</HediffDef>"#;

pub(crate) const ADDED_PART_SYNTHETIC_XML: &str = r#"<HediffDef Name="addedPartExampleSynth" ParentName="AddedBodyPartBase" Abstract="True">
  <defaultLabelColor>(188,39,242)</defaultLabelColor>
  <comps>
    <li MayRequire="example.bodyframework" Class="ExampleBody.Hediffs.HediffCompProperties_MaxHPAdjust">
      <scaleAdjustment>0.20</scaleAdjustment>
    </li>
  </comps>
</HediffDef>"#;

/// Everything [`bionic_heart_fixture`] builds.
pub struct BionicHeartFixture {
    /// A [`Report`] naming `ludeon.rimworld` and `example.bionicsfork`.
    pub report: Report,
    /// A [`SourceIndex`] locating both owners' `HediffDef/BionicHeart` and
    /// the template chain each climbs (`AddedBodyPartBase` ->
    /// `ImplantHediffBase` for Core, `addedPartExampleSynth` ->
    /// `AddedBodyPartBase` for Example Bionics Fork).
    pub sources: SourceIndex,
    /// A reader seeded with every element `sources`' locators name.
    pub reader: InMemoryDefSourceReader,
}

/// A worked example:
/// `HediffDef/BionicHeart`, defined by both `ludeon.rimworld` (Core) and
/// `example.bionicsfork` (Example Bionics Fork), with BIONICS's `ParentName` chain climbing
/// through its own `addedPartExampleSynth` template to Core's
/// `AddedBodyPartBase`/`ImplantHediffBase`. The XML text is copied
/// verbatim from `crates/rim-merge/tests/fixtures/xml/` (that crate's own
/// worked-example fixtures) rather than referenced by cross-crate path,
/// since `rim-merge`'s `tests/` directory is not part of its public API
/// and is not edited to add a re-export.
#[must_use]
pub fn bionic_heart_fixture() -> BionicHeartFixture {
    let core = ModId::new("ludeon.rimworld");
    let bionics = ModId::new("example.bionicsfork");

    let core_def_locator = locator("core_bionic.xml", 0);
    let bionics_def_locator = locator("bionics_bionic.xml", 0);
    let implant_locator = locator("core_hediff_bases.xml", 0);
    let added_body_locator = locator("core_hediff_bases.xml", 1);
    let synthetic_locator = locator("bionics_hediff_base.xml", 0);

    let mut sources = SourceIndex::default();
    sources.defs.insert(
        (
            core.clone(),
            ("HediffDef".to_string(), "BionicHeart".to_string()),
        ),
        vec![DefEntry {
            def_type: "HediffDef".to_string(),
            def_name: "BionicHeart".to_string(),
            may_require: Vec::new(),
            may_require_any_of: Vec::new(),
            parent_name: Some("AddedBodyPartBase".to_string()),
            locator: core_def_locator.clone(),
        }],
    );
    sources.defs.insert(
        (
            bionics.clone(),
            ("HediffDef".to_string(), "BionicHeart".to_string()),
        ),
        vec![DefEntry {
            def_type: "HediffDef".to_string(),
            def_name: "BionicHeart".to_string(),
            may_require: Vec::new(),
            may_require_any_of: Vec::new(),
            parent_name: Some("addedPartExampleSynth".to_string()),
            locator: bionics_def_locator.clone(),
        }],
    );
    // The inverse `owners_by_def` map — `InspectDef`'s own existence check reads
    // this, not `defs` directly, matching what a real scan always builds
    // alongside it.
    sources.owners_by_def.insert(
        ("HediffDef".to_string(), "BionicHeart".to_string()),
        vec![core.clone(), bionics.clone()],
    );
    sources.templates.insert(
        ("HediffDef".to_string(), "ImplantHediffBase".to_string()),
        vec![(
            core.clone(),
            TemplateEntry {
                graphic_class: None,
                may_require: Vec::new(),
                def_type: "HediffDef".to_string(),
                name: "ImplantHediffBase".to_string(),
                parent_name: None,
                is_abstract: true,
                locator: implant_locator.clone(),
            },
        )],
    );
    sources.templates.insert(
        ("HediffDef".to_string(), "AddedBodyPartBase".to_string()),
        vec![(
            core.clone(),
            TemplateEntry {
                graphic_class: None,
                may_require: Vec::new(),
                def_type: "HediffDef".to_string(),
                name: "AddedBodyPartBase".to_string(),
                parent_name: Some("ImplantHediffBase".to_string()),
                is_abstract: true,
                locator: added_body_locator.clone(),
            },
        )],
    );
    sources.templates.insert(
        ("HediffDef".to_string(), "addedPartExampleSynth".to_string()),
        vec![(
            bionics.clone(),
            TemplateEntry {
                graphic_class: None,
                may_require: Vec::new(),
                def_type: "HediffDef".to_string(),
                name: "addedPartExampleSynth".to_string(),
                parent_name: Some("AddedBodyPartBase".to_string()),
                is_abstract: true,
                locator: synthetic_locator.clone(),
            },
        )],
    );

    let mut elements = BTreeMap::new();
    elements.insert(core_def_locator, CORE_BIONIC_HEART_XML.to_string());
    elements.insert(bionics_def_locator, BIONICS_BIONIC_HEART_XML.to_string());
    elements.insert(implant_locator, IMPLANT_HEDIFF_BASE_XML.to_string());
    elements.insert(added_body_locator, ADDED_BODY_PART_BASE_XML.to_string());
    elements.insert(synthetic_locator, ADDED_PART_SYNTHETIC_XML.to_string());

    BionicHeartFixture {
        report: report_fixture(&["ludeon.rimworld", "example.bionicsfork"]),
        sources,
        reader: InMemoryDefSourceReader::new(elements),
    }
}

/// [`bionic_heart_fixture`], but with a real `Conflict::DefOverride` for
/// `HediffDef/BionicHeart` pushed onto the report (the plain fixture's
/// report never has one — its own tests decide the merge directly,
/// bypassing the ledger's own finding extraction). Merge-first suggestion
/// tests need the ledger to actually
/// *produce* the finding — with no `winner_declares_relation`/
/// `same_author`/`overrides_vanilla`/`shadows_framework` signal, it
/// suggests `Accept` at confidence 60 with a `Merge` alternative (the
/// "unexplained" row in `ledger::suggest::def_override`), which is
/// exactly the shape the merge-first re-derivation applies to.
#[must_use]
pub fn bionic_heart_fixture_with_conflict() -> BionicHeartFixture {
    let mut fixture = bionic_heart_fixture();
    fixture.report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("ludeon.rimworld")
        .mod_("example.bionicsfork")
        .def_override(
            "HediffDef",
            "BionicHeart",
            &["ludeon.rimworld", "example.bionicsfork"],
        )
        .build();
    fixture
}

/// BIONICS's own registration of a template named `"AddedBodyPartBase"` —
/// [`bionic_heart_fixture_flat`]'s own duplicate-`Name` trick. It carries
/// the `addedPartExampleSynth` template's own content
/// (`defaultLabelColor`/`comps`), **plus** [`ADDED_BODY_PART_BASE_XML`]'s
/// own `hediffClass`/`priceImpact` folded in directly: in
/// [`bionic_heart_fixture`]'s two-hop chain (`addedPartExampleSynth` ->
/// `AddedBodyPartBase`) BIONICS inherits Core's `hediffClass` from the
/// *second* hop; with one hop (both owners share the literal
/// `ParentName` string, so BIONICS's own registration of that same Name
/// is as far as its own chain goes) that content has to be carried
/// forward directly, or `hediffClass` — genuinely `Unchanged` in the
/// two-hop fixture — would spuriously start differing (which
/// `bionic_heart_is_all_clean_merge_with_after_merge_and_in_game_naming_the_winner`
/// checks).
pub(crate) const BIONICS_ADDED_BODY_PART_BASE_XML: &str = r#"<HediffDef Name="AddedBodyPartBase" ParentName="ImplantHediffBase" Abstract="True">
  <hediffClass>Hediff_AddedPart</hediffClass>
  <priceImpact>true</priceImpact>
  <defaultLabelColor>(188,39,242)</defaultLabelColor>
  <comps>
    <li MayRequire="example.bodyframework" Class="ExampleBody.Hediffs.HediffCompProperties_MaxHPAdjust">
      <scaleAdjustment>0.20</scaleAdjustment>
    </li>
  </comps>
</HediffDef>"#;

/// [`bionic_heart_fixture`]'s structural-guard-free twin: the identical two owners
/// (`ludeon.rimworld`/`example.bionicsfork`) and the identical
/// `HediffDef/BionicHeart` def key — so every existing helper
/// (`bionic_heart_key`, a literal `DefKey { def_type: "HediffDef",
/// def_name: "BionicHeart" }`) still resolves against it unchanged.
///
/// **Not a no-inheritance fixture**: the structural guard fires on a
/// *differing* `ParentName`, never on merely *having* one, so this
/// fixture keeps real inheritance coverage.
/// Both owners' own `BionicHeart` declare the identical literal
/// string `ParentName="AddedBodyPartBase"` — `structural_change`'s own
/// `ParentName` check is a plain string comparison
/// (`owner.raw.parent_name != base_owner.raw.parent_name`), so identical
/// strings never trigger it, regardless of what those strings actually
/// resolve to.
///
/// What they resolve to still differs, on purpose, reusing
/// `bionic_heart_fixture`'s own real inheritance content: `sources.templates`
/// carries **two** registrants of `("HediffDef", "AddedBodyPartBase")` —
/// `ludeon.rimworld`'s own (plain, no `defaultLabelColor`/`comps`) and
/// `example.bionicsfork`'s own ([`BIONICS_ADDED_BODY_PART_BASE_XML`], byte-for-byte
/// the `addedPartExampleSynth` content). `def_sources::nearest_owner`
/// resolves a duplicated `Name`
/// **per asking child**, nearest registrant at or before that child's own
/// load position — `ludeon.rimworld` (position 0) only ever sees its own
/// registration; `example.bionicsfork` (position 1) sees both and picks its
/// own (`position 1 <= 1`, and it's the higher of the two) — so BIONICS's
/// `BionicHeart` still resolves `defaultLabelColor`/`comps` from its own
/// template exactly as it does through the separately-named
/// `addedPartExampleSynth` chain, while the literal `ParentName` string both
/// owners carry is identical. `label` still differs directly (`"bionic
/// heart"` on the base, `"synthetic heart"` on the winner) — with no
/// stored choice, every field auto-resolves to the winner's own already-
/// inherited value (a no-op, `MergeState::Complete { op_count: 0 }`); a
/// stored choice naming the base for `label` produces exactly one
/// `Replace` op (`MergeState::Complete { op_count: 1 }`), `defaultLabelColor`/
/// `comps` unaffected either way — the zero-op and one-op shapes, reachable
/// without the structural guard ever firing.
#[must_use]
pub fn bionic_heart_fixture_flat() -> BionicHeartFixture {
    let core = ModId::new("ludeon.rimworld");
    let bionics = ModId::new("example.bionicsfork");
    let core_locator = locator("core_bionic_flat.xml", 0);
    let bionics_locator = locator("bionics_bionic_flat.xml", 0);
    let implant_locator = locator("core_hediff_bases_flat.xml", 0);
    let core_added_body_locator = locator("core_hediff_bases_flat.xml", 1);
    let bionics_added_body_locator = locator("bionics_added_body_part_base_flat.xml", 0);

    let mut sources = SourceIndex::default();
    for (owner, entry_locator) in [(&core, &core_locator), (&bionics, &bionics_locator)] {
        sources.defs.insert(
            (
                owner.clone(),
                ("HediffDef".to_string(), "BionicHeart".to_string()),
            ),
            vec![DefEntry {
                def_type: "HediffDef".to_string(),
                def_name: "BionicHeart".to_string(),
                may_require: Vec::new(),
                may_require_any_of: Vec::new(),
                parent_name: Some("AddedBodyPartBase".to_string()),
                locator: entry_locator.clone(),
            }],
        );
    }
    sources.owners_by_def.insert(
        ("HediffDef".to_string(), "BionicHeart".to_string()),
        vec![core.clone(), bionics.clone()],
    );
    sources.templates.insert(
        ("HediffDef".to_string(), "ImplantHediffBase".to_string()),
        vec![(
            core.clone(),
            TemplateEntry {
                graphic_class: None,
                may_require: Vec::new(),
                def_type: "HediffDef".to_string(),
                name: "ImplantHediffBase".to_string(),
                parent_name: None,
                is_abstract: true,
                locator: implant_locator.clone(),
            },
        )],
    );
    // Two registrants of the *same* Name, on purpose — see this
    // function's own doc comment for why that's exactly what lets
    // `label(ParentName)` stay identical between owners while their
    // resolved templates still differ.
    sources.templates.insert(
        ("HediffDef".to_string(), "AddedBodyPartBase".to_string()),
        vec![
            (
                core.clone(),
                TemplateEntry {
                    graphic_class: None,
                    may_require: Vec::new(),
                    def_type: "HediffDef".to_string(),
                    name: "AddedBodyPartBase".to_string(),
                    parent_name: Some("ImplantHediffBase".to_string()),
                    is_abstract: true,
                    locator: core_added_body_locator.clone(),
                },
            ),
            (
                bionics.clone(),
                TemplateEntry {
                    graphic_class: None,
                    may_require: Vec::new(),
                    def_type: "HediffDef".to_string(),
                    name: "AddedBodyPartBase".to_string(),
                    parent_name: Some("ImplantHediffBase".to_string()),
                    is_abstract: true,
                    locator: bionics_added_body_locator.clone(),
                },
            ),
        ],
    );

    let mut elements = BTreeMap::new();
    elements.insert(core_locator,
        "<HediffDef ParentName=\"AddedBodyPartBase\"><defName>BionicHeart</defName><label>bionic heart</label></HediffDef>"
            .to_string());
    elements.insert(bionics_locator,
        "<HediffDef ParentName=\"AddedBodyPartBase\"><defName>BionicHeart</defName><label>synthetic heart</label></HediffDef>"
            .to_string());
    elements.insert(implant_locator, IMPLANT_HEDIFF_BASE_XML.to_string());
    elements.insert(
        core_added_body_locator,
        ADDED_BODY_PART_BASE_XML.to_string(),
    );
    elements.insert(
        bionics_added_body_locator,
        BIONICS_ADDED_BODY_PART_BASE_XML.to_string(),
    );

    BionicHeartFixture {
        report: report_fixture(&["ludeon.rimworld", "example.bionicsfork"]),
        sources,
        reader: InMemoryDefSourceReader::new(elements),
    }
}

/// [`bionic_heart_fixture_flat`], with a real `Conflict::DefOverride`
/// pushed onto the report — [`bionic_heart_fixture_with_conflict`]'s own
/// sibling, needed for the identical reason (merge-first suggestion tests
/// where the ledger must actually produce the finding).
#[must_use]
pub fn bionic_heart_fixture_flat_with_conflict() -> BionicHeartFixture {
    let mut fixture = bionic_heart_fixture_flat();
    fixture.report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("ludeon.rimworld")
        .mod_("example.bionicsfork")
        .def_override(
            "HediffDef",
            "BionicHeart",
            &["ludeon.rimworld", "example.bionicsfork"],
        )
        .build();
    fixture
}

/// Everything [`conflicting_wall_fixture`] builds.
pub struct ConflictingWallFixture {
    /// A [`Report`] naming `core.mod`, `a.mod`, and `b.mod`, with a
    /// `Conflict::DefOverride` for `ThingDef/Wall` across all three.
    pub report: Report,
    /// A [`SourceIndex`] locating each owner's own `ThingDef/Wall` (no
    /// `ParentName` chain at all — this fixture only needs a genuinely
    /// conflicting leaf field, not inheritance).
    pub sources: SourceIndex,
    /// A reader seeded with every element `sources`' locators name.
    pub reader: InMemoryDefSourceReader,
}

/// Three owners of `ThingDef/Wall`, each setting `<label>` to a different
/// value: `core.mod` (the base, `"wall"`), `a.mod` (`"stone wall"`), and
/// `b.mod` (`"brick wall"`, the winner). A genuine `DiffClass::Conflict`
/// field needs *three* owners — two owners differing from the base to
/// *different* values — which [`bionic_heart_fixture`]'s own two-owner
/// shape can never produce (see `rim_merge::diff::classify`); this is the
/// smallest fixture that can, for merge-first suggestion tests
/// exercising the
/// `MergeState::NeedsFieldInput` branch.
#[must_use]
pub fn conflicting_wall_fixture() -> ConflictingWallFixture {
    let core = ModId::new("core.mod");
    let mod_a = ModId::new("a.mod");
    let mod_b = ModId::new("b.mod");

    let core_locator = locator("core_wall.xml", 0);
    let a_locator = locator("a_wall.xml", 0);
    let b_locator = locator("b_wall.xml", 0);

    let mut sources = SourceIndex::default();
    for (owner, entry_locator) in [
        (&core, &core_locator),
        (&mod_a, &a_locator),
        (&mod_b, &b_locator),
    ] {
        sources.defs.insert(
            (owner.clone(), ("ThingDef".to_string(), "Wall".to_string())),
            vec![DefEntry {
                def_type: "ThingDef".to_string(),
                def_name: "Wall".to_string(),
                may_require: Vec::new(),
                may_require_any_of: Vec::new(),
                parent_name: None,
                locator: entry_locator.clone(),
            }],
        );
    }

    let mut elements = BTreeMap::new();
    elements.insert(
        core_locator,
        "<ThingDef><defName>Wall</defName><label>wall</label></ThingDef>".to_string(),
    );
    elements.insert(
        a_locator,
        "<ThingDef><defName>Wall</defName><label>stone wall</label></ThingDef>".to_string(),
    );
    elements.insert(
        b_locator,
        "<ThingDef><defName>Wall</defName><label>brick wall</label></ThingDef>".to_string(),
    );

    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("core.mod")
        .mod_("a.mod")
        .mod_("b.mod")
        .def_override("ThingDef", "Wall", &["core.mod", "a.mod", "b.mod"])
        .build();

    ConflictingWallFixture {
        report,
        sources,
        reader: InMemoryDefSourceReader::new(elements),
    }
}

/// Two owners of `ThingDef/Widget` (`core.mod` the base, `winner.mod` the
/// winner), sharing `report`/`sources`/`reader`'s own generic shape
/// — the three
/// `*_trigger_fixture` functions below all build one of these, each
/// differing from `core.mod`'s copy in exactly the one field its own name
/// says, and nothing else that could itself trigger the guard (no
/// `ParentName`, no other structural field), so each one isolates its own
/// trigger the same way `rim-merge`'s own `diff.rs` unit tests do for
/// `structural_change` itself — this module's job is only to prove
/// `PlanMerge` actually wires that predicate into a real preview, not to
/// re-verify the predicate's own per-field logic.
pub type StructuralTriggerFixture = ConflictingWallFixture;

fn structural_trigger_fixture(core_xml: &str, winner_xml: &str) -> StructuralTriggerFixture {
    let core = ModId::new("core.mod");
    let winner = ModId::new("winner.mod");
    let core_locator = locator("core_widget.xml", 0);
    let winner_locator = locator("winner_widget.xml", 0);

    let mut sources = SourceIndex::default();
    for (owner, entry_locator) in [(&core, &core_locator), (&winner, &winner_locator)] {
        sources.defs.insert(
            (
                owner.clone(),
                ("ThingDef".to_string(), "Widget".to_string()),
            ),
            vec![DefEntry {
                def_type: "ThingDef".to_string(),
                def_name: "Widget".to_string(),
                may_require: Vec::new(),
                may_require_any_of: Vec::new(),
                parent_name: None,
                locator: entry_locator.clone(),
            }],
        );
    }

    let mut elements = BTreeMap::new();
    elements.insert(core_locator, core_xml.to_string());
    elements.insert(winner_locator, winner_xml.to_string());

    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("core.mod")
        .mod_("winner.mod")
        .def_override("ThingDef", "Widget", &["core.mod", "winner.mod"])
        .build();

    StructuralTriggerFixture {
        report,
        sources,
        reader: InMemoryDefSourceReader::new(elements),
    }
}

/// The structural guard's `thingClass` trigger: `core.mod` declares `Building`, `winner.mod`
/// declares `Building_Door` — a genuine `FieldDiff` on `thingClass`
/// itself, the one trigger field `structural_change` reads off
/// `diff.fields` rather than an owner's raw tree directly.
#[must_use]
pub fn thing_class_trigger_fixture() -> StructuralTriggerFixture {
    structural_trigger_fixture(
        "<ThingDef><defName>Widget</defName><thingClass>Building</thingClass></ThingDef>",
        "<ThingDef><defName>Widget</defName><thingClass>Building_Door</thingClass></ThingDef>",
    )
}

/// The structural guard's root `Class` trigger: the def's own root element's `Class`
/// attribute differs (`ThingWithComps` vs. `Building`) — never a diffed
/// field at all (`rim_merge::xml::parse` never strips `Class`, but
/// `three_way` only ever diffs a def's *children*, not its own root
/// attributes), so `structural_change` reads it directly off each owner's
/// `raw.root.attrs`.
#[must_use]
pub fn root_class_trigger_fixture() -> StructuralTriggerFixture {
    structural_trigger_fixture(
        r#"<ThingDef Class="ThingWithComps"><defName>Widget</defName></ThingDef>"#,
        r#"<ThingDef Class="Building"><defName>Widget</defName></ThingDef>"#,
    )
}

/// The structural guard's comp `Class` trigger: both owners declare exactly one `comps/li`,
/// at the same list position, with different `Class` attributes
/// (`CompProperties_Refuelable` vs. `CompProperties_Power`) — compared by
/// position on each owner's own `resolved` tree (`structural_change`'s own
/// doc comment explains why position, not `ItemIdentity`, is the
/// correspondence rule here).
#[must_use]
pub fn comp_class_trigger_fixture() -> StructuralTriggerFixture {
    structural_trigger_fixture(
        "<ThingDef><defName>Widget</defName><comps><li Class=\"CompProperties_Refuelable\"><fuelCapacity>10</fuelCapacity></li></comps></ThingDef>",
        "<ThingDef><defName>Widget</defName><comps><li Class=\"CompProperties_Power\"><basePowerConsumption>50</basePowerConsumption></li></comps></ThingDef>",
    )
}

/// Everything [`clean_override_fixture`] builds.
pub struct CleanOverrideFixture {
    /// A [`Report`] naming `core.mod` and `winner.mod`, with a
    /// `Conflict::DefOverride` for `ThingDef/<def_name>`.
    pub report: Report,
    /// A [`SourceIndex`] locating each owner's own `ThingDef/<def_name>`.
    pub sources: SourceIndex,
    /// A reader seeded with every element `sources`' locators name.
    pub reader: InMemoryDefSourceReader,
}

/// A clean, three-owner `ThingDef/<def_name>` override where the mods
/// touch *different* fields: `core.mod` (the base) sets neither field at
/// all; `contributor` sets `<description>`, which neither `core.mod` nor
/// `winner` ever touches; `winner` (the last-loaded, and the only owner
/// [`FindingKey::DefOverride`] itself would call the winner) sets its own
/// `<label>`. `active_mods` must list `core.mod`, `contributor`, and
/// `winner` in exactly that load order — the real merge preview
/// re-derives base/winner from the session's actual load order, not from
/// this fixture's own `Conflict::DefOverride.owners` field, so a caller
/// building the session must pass that order through
/// [`session_with_sources_and_mods`] itself.
///
/// [`bionic_heart_fixture`]'s two-owner shape can *never* produce a
/// genuine op-count here: with exactly one non-base owner, that owner is
/// always also the winner, so every one-sided field's automatic result is
/// already what the winner's own raw file produces — a no-op
/// (`MergeState::Complete { op_count: 0 }`, see that fixture's own doc
/// comment). A one-sided field whose contributor is neither the base nor
/// the winner is what actually needs carrying forward with a real `Add`
/// op — `MergeState::Complete { op_count: 1 }` — the shape the "the mods
/// change different fields; merging keeps both" suggestion actually
/// describes, and what the zero-vs-nonzero-op-count tests need to tell a
/// *genuine* clean merge apart from a zero-op one.
#[must_use]
pub fn clean_override_fixture(
    def_name: &str,
    contributor: &str,
    winner: &str,
) -> CleanOverrideFixture {
    let core = ModId::new("core.mod");
    let contributor_id = ModId::new(contributor);
    let winner_id = ModId::new(winner);
    let core_locator = locator(&format!("core_{def_name}.xml"), 0);
    let contributor_locator = locator(&format!("{contributor}_{def_name}.xml"), 0);
    let winner_locator = locator(&format!("{winner}_{def_name}.xml"), 0);

    let mut sources = SourceIndex::default();
    for (owner, entry_locator) in [
        (&core, &core_locator),
        (&contributor_id, &contributor_locator),
        (&winner_id, &winner_locator),
    ] {
        sources.defs.insert(
            (
                owner.clone(),
                ("ThingDef".to_string(), def_name.to_string()),
            ),
            vec![DefEntry {
                def_type: "ThingDef".to_string(),
                def_name: def_name.to_string(),
                may_require: Vec::new(),
                may_require_any_of: Vec::new(),
                parent_name: None,
                locator: entry_locator.clone(),
            }],
        );
    }

    let mut elements = BTreeMap::new();
    elements.insert(
        core_locator,
        format!("<ThingDef><defName>{def_name}</defName></ThingDef>"),
    );
    elements.insert(contributor_locator,
        format!("<ThingDef><defName>{def_name}</defName><description>a plain {def_name}</description></ThingDef>"
        ));
    elements.insert(
        winner_locator,
        format!(
            "<ThingDef><defName>{def_name}</defName><label>a fancy {def_name}</label></ThingDef>"
        ),
    );

    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("core.mod")
        .mod_(contributor)
        .mod_(winner)
        .def_override("ThingDef", def_name, &["core.mod", contributor, winner])
        .build();

    CleanOverrideFixture {
        report,
        sources,
        reader: InMemoryDefSourceReader::new(elements),
    }
}

/// Everything [`identical_def_override_fixture`] builds.
pub struct IdenticalOverrideFixture {
    /// A [`Report`] naming every id in `owners`, with a
    /// `Conflict::DefOverride` for `ThingDef/<def_name>` carrying every
    /// direction flag `false`.
    pub report: Report,
    /// A [`SourceIndex`] locating each owner's own `ThingDef/<def_name>`.
    pub sources: SourceIndex,
    /// A reader seeded with every element `sources`' locators name, every
    /// one holding byte-identical XML text.
    pub reader: InMemoryDefSourceReader,
}

/// A `ThingDef/<def_name>` override where every owner's own raw copy is
/// byte-for-byte identical — the identical-copies acceptance case
/// ("a harmless flip it cannot otherwise call harmless"): order genuinely
/// cannot change the outcome. The report's own `Conflict::DefOverride`
/// carries every direction flag `false` (the analyzer found no declared
/// relation, no shared author, no lone non-vanilla owner, no shadowed
/// framework) — `rim_resolve::ledger::suggest::def_override_direction`'s
/// `Unknown` case, matching the more common of the two real buckets the
/// identical-copies check targets. A caller wanting the `SameAuthor` case instead (also eligible
/// — see that function's own doc comment) flips
/// `report.conflicts[0]`'s own `same_author` field directly after this
/// returns, rather than this fixture growing a second flag parameter for
/// one caller.
#[must_use]
pub fn identical_def_override_fixture(def_name: &str, owners: &[&str]) -> IdenticalOverrideFixture {
    let mut sources = SourceIndex::default();
    let mut elements = BTreeMap::new();
    let identical_xml = format!(
        "<ThingDef><defName>{def_name}</defName><label>a plain {def_name}</label></ThingDef>"
    );
    for &owner in owners {
        let entry_locator = locator(&format!("{owner}_{def_name}.xml"), 0);
        sources.defs.insert(
            (
                ModId::new(owner),
                ("ThingDef".to_string(), def_name.to_string()),
            ),
            vec![DefEntry {
                def_type: "ThingDef".to_string(),
                def_name: def_name.to_string(),
                may_require: Vec::new(),
                may_require_any_of: Vec::new(),
                parent_name: None,
                locator: entry_locator.clone(),
            }],
        );
        elements.insert(entry_locator, identical_xml.clone());
    }

    let mut builder = rim_resolve::test_support::ReportBuilder::new();
    for &owner in owners {
        builder = builder.mod_(owner);
    }
    let report = builder.def_override("ThingDef", def_name, owners).build();

    IdenticalOverrideFixture {
        report,
        sources,
        reader: InMemoryDefSourceReader::new(elements),
    }
}

/// Everything [`two_clean_overrides_fixture`] builds.
pub struct TwoCleanOverridesFixture {
    /// A [`Report`] naming `core.mod`, `extra_a.mod`, `a.mod`,
    /// `extra_b.mod`, and `b.mod`, with a `Conflict::DefOverride` for each
    /// of `ThingDef/WallA` (`core.mod`/`extra_a.mod`/`a.mod`),
    /// `ThingDef/WallB` (`core.mod`/`extra_b.mod`/`b.mod`), and
    /// `ThingDef/WallC` (`core.mod`/`a.mod`/`b.mod`, a genuine three-owner
    /// conflict).
    pub report: Report,
    /// A [`SourceIndex`] locating all three overrides.
    pub sources: SourceIndex,
    /// A reader seeded with every element `sources`' locators name.
    pub reader: InMemoryDefSourceReader,
    /// Every mod, in the load order a caller must pass to
    /// [`session_with_sources_and_mods`] for `WallA`/`WallB`'s
    /// contributor to actually land between their def's base and winner.
    pub active_mods: Vec<&'static str>,
}

/// Two independent, clean, three-owner `ThingDef` overrides — `WallA`
/// (`core.mod`/`extra_a.mod`/`a.mod`) and `WallB`
/// (`core.mod`/`extra_b.mod`/`b.mod`) — each shaped like
/// [`clean_override_fixture`] (a real `Add` op is needed, never a no-op:
/// `MergeState::Complete { op_count: 1 }`) — plus a third, genuinely
/// conflicting `WallC` across `core.mod`/`a.mod`/`b.mod` (shaped like
/// [`conflicting_wall_fixture`]: `MergeState::NeedsFieldInput`, which
/// never promotes to `Auto`). `extra_a.mod`/`extra_b.mod` exist solely to
/// be each other's def's middle (non-base, non-winner) contributor —
/// `WallC` never involves them.
///
/// **None of these three promote to `Auto`**:
/// `redecide_for_clean_merge`'s `Complete { op_count > 0 }` arm only
/// leads with `Merge` in the alternatives for a `DefOverride`, never
/// promotes it outright, so `WallA`/`WallB` stay at their own original
/// (`Unknown`, confidence 60) suggestion, same as `WallC`. This
/// fixture is what `use_cases::merge_coverage`'s own
/// `a_mixed_def_override_fixture_no_longer_promotes_any_row_to_merge_85`
/// test uses it for — proving exactly that. The fixture behind
/// `Session::findings`'s own paging/idempotency test is
/// [`two_zero_op_overrides_fixture`] instead, since a non-zero-op
/// `DefOverride` cannot be lazily promoted at all.
#[must_use]
pub fn two_clean_overrides_fixture() -> TwoCleanOverridesFixture {
    let core = ModId::new("core.mod");
    let mod_a = ModId::new("a.mod");
    let mod_b = ModId::new("b.mod");

    let mut sources = SourceIndex::default();
    let mut elements = BTreeMap::new();
    for (def_name, contributor, winner) in [
        ("WallA", "extra_a.mod", "a.mod"),
        ("WallB", "extra_b.mod", "b.mod"),
    ] {
        let contributor_id = ModId::new(contributor);
        let winner_id = ModId::new(winner);
        let core_locator = locator(&format!("core_{def_name}.xml"), 0);
        let contributor_locator = locator(&format!("{contributor}_{def_name}.xml"), 0);
        let winner_locator = locator(&format!("{winner}_{def_name}.xml"), 0);
        for (owner, entry_locator) in [
            (&core, &core_locator),
            (&contributor_id, &contributor_locator),
            (&winner_id, &winner_locator),
        ] {
            sources.defs.insert(
                (
                    owner.clone(),
                    ("ThingDef".to_string(), def_name.to_string()),
                ),
                vec![DefEntry {
                    def_type: "ThingDef".to_string(),
                    def_name: def_name.to_string(),
                    may_require: Vec::new(),
                    may_require_any_of: Vec::new(),
                    parent_name: None,
                    locator: entry_locator.clone(),
                }],
            );
        }
        elements.insert(
            core_locator,
            format!("<ThingDef><defName>{def_name}</defName></ThingDef>"),
        );
        elements.insert(contributor_locator,
            format!("<ThingDef><defName>{def_name}</defName><description>a plain {def_name}</description></ThingDef>"
            ));
        elements.insert(winner_locator,
            format!("<ThingDef><defName>{def_name}</defName><label>a fancy {def_name}</label></ThingDef>"
            ));
    }

    // `WallC`: all three of `core.mod`/`a.mod`/`b.mod` set `<label>` to a
    // different value — a genuine `DiffClass::Conflict` (see
    // `conflicting_wall_fixture`'s own doc comment for why that needs
    // three owners), so it stays `NeedsFieldInput`/`NeedsInput` forever
    // regardless of merge-first suggestions.
    for (owner, label) in [
        (&core, "wall c"),
        (&mod_a, "stone wall c"),
        (&mod_b, "brick wall c"),
    ] {
        let entry_locator = locator(&format!("{owner}_WallC.xml"), 0);
        sources.defs.insert(
            (owner.clone(), ("ThingDef".to_string(), "WallC".to_string())),
            vec![DefEntry {
                def_type: "ThingDef".to_string(),
                def_name: "WallC".to_string(),
                may_require: Vec::new(),
                may_require_any_of: Vec::new(),
                parent_name: None,
                locator: entry_locator.clone(),
            }],
        );
        elements.insert(
            entry_locator,
            format!("<ThingDef><defName>WallC</defName><label>{label}</label></ThingDef>"),
        );
    }

    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("core.mod")
        .mod_("extra_a.mod")
        .mod_("a.mod")
        .mod_("extra_b.mod")
        .mod_("b.mod")
        .def_override("ThingDef", "WallA", &["core.mod", "extra_a.mod", "a.mod"])
        .def_override("ThingDef", "WallB", &["core.mod", "extra_b.mod", "b.mod"])
        .def_override("ThingDef", "WallC", &["core.mod", "a.mod", "b.mod"])
        .build();

    TwoCleanOverridesFixture {
        report,
        sources,
        reader: InMemoryDefSourceReader::new(elements),
        active_mods: vec!["core.mod", "extra_a.mod", "a.mod", "extra_b.mod", "b.mod"],
    }
}

/// Everything [`two_zero_op_overrides_fixture`] builds.
pub struct TwoZeroOpOverridesFixture {
    /// A [`Report`] naming `core.mod`, `a.mod`, and `b.mod`, with a
    /// `Conflict::DefOverride` for each of `ThingDef/WallA`
    /// (`core.mod`/`a.mod`), `ThingDef/WallB` (`core.mod`/`b.mod`), and
    /// `ThingDef/WallC` (`core.mod`/`a.mod`/`b.mod`, a genuine three-owner
    /// conflict).
    pub report: Report,
    /// A [`SourceIndex`] locating all three overrides.
    pub sources: SourceIndex,
    /// A reader seeded with every element `sources`' locators name.
    pub reader: InMemoryDefSourceReader,
    /// Every mod, in load order.
    pub active_mods: Vec<&'static str>,
}

/// [`two_clean_overrides_fixture`]'s own zero-op sibling, the
/// `Session::findings` paging/idempotency fixture (a non-zero-op
/// `DefOverride` is un-promotable, so that fixture cannot serve): `WallA`
/// (`core.mod`/`a.mod`) and `WallB` (`core.mod`/`b.mod`) are each a plain
/// two-owner, zero-op override — `a.mod`/`b.mod` (the winner in each)
/// simply sets its own `<label>`, so the `OneSided` diff already matches
/// the winner's own raw content (`MergeState::Complete { op_count: 0 }`)
/// — the zero-op clean-merge arm promotes *this* shape for either finding
/// kind, so both independently redecide from `Accept`
/// 60 (`Unknown`, `NeedsInput`) to `Accept` 95 (`Auto`) once lazily
/// evaluated. `WallC` (`core.mod`/`a.mod`/`b.mod`) is the identical
/// genuine three-owner conflict `two_clean_overrides_fixture`'s own
/// `WallC` uses (never resolves) — the paging test's own required
/// "control that never promotes" (see that fixture's own doc comment for
/// why one is needed at all).
#[must_use]
pub fn two_zero_op_overrides_fixture() -> TwoZeroOpOverridesFixture {
    let core = ModId::new("core.mod");
    let mod_a = ModId::new("a.mod");
    let mod_b = ModId::new("b.mod");

    let mut sources = SourceIndex::default();
    let mut elements = BTreeMap::new();
    for (def_name, winner, label) in [("WallA", "a.mod", "a label"), ("WallB", "b.mod", "b label")]
    {
        let winner_id = ModId::new(winner);
        let core_locator = locator(&format!("core_{def_name}_zero_op.xml"), 0);
        let winner_locator = locator(&format!("{winner}_{def_name}_zero_op.xml"), 0);
        for (owner, entry_locator) in [(&core, &core_locator), (&winner_id, &winner_locator)] {
            sources.defs.insert(
                (
                    owner.clone(),
                    ("ThingDef".to_string(), def_name.to_string()),
                ),
                vec![DefEntry {
                    def_type: "ThingDef".to_string(),
                    def_name: def_name.to_string(),
                    may_require: Vec::new(),
                    may_require_any_of: Vec::new(),
                    parent_name: None,
                    locator: entry_locator.clone(),
                }],
            );
        }
        elements.insert(
            core_locator,
            format!("<ThingDef><defName>{def_name}</defName></ThingDef>"),
        );
        elements.insert(
            winner_locator,
            format!("<ThingDef><defName>{def_name}</defName><label>{label}</label></ThingDef>"),
        );
    }

    // `WallC`: the same genuine three-owner conflict
    // `two_clean_overrides_fixture`'s own `WallC` uses — see that
    // fixture's own doc comment for why a control that never promotes is
    // needed at all.
    for (owner, label) in [
        (&core, "wall c"),
        (&mod_a, "stone wall c"),
        (&mod_b, "brick wall c"),
    ] {
        let entry_locator = locator(&format!("{owner}_WallC_zero_op.xml"), 0);
        sources.defs.insert(
            (owner.clone(), ("ThingDef".to_string(), "WallC".to_string())),
            vec![DefEntry {
                def_type: "ThingDef".to_string(),
                def_name: "WallC".to_string(),
                may_require: Vec::new(),
                may_require_any_of: Vec::new(),
                parent_name: None,
                locator: entry_locator.clone(),
            }],
        );
        elements.insert(
            entry_locator,
            format!("<ThingDef><defName>WallC</defName><label>{label}</label></ThingDef>"),
        );
    }

    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("core.mod")
        .mod_("a.mod")
        .mod_("b.mod")
        .def_override("ThingDef", "WallA", &["core.mod", "a.mod"])
        .def_override("ThingDef", "WallB", &["core.mod", "b.mod"])
        .def_override("ThingDef", "WallC", &["core.mod", "a.mod", "b.mod"])
        .build();

    TwoZeroOpOverridesFixture {
        report,
        sources,
        reader: InMemoryDefSourceReader::new(elements),
        active_mods: vec!["core.mod", "a.mod", "b.mod"],
    }
}

/// Everything [`five_owner_scope_fixture`] builds.
pub struct FiveOwnerScopeFixture {
    /// A [`Report`] naming `ludeon.rimworld`, `a.mod`, `b.mod`, `c.mod`,
    /// and `d.mod` (in that load order), with a `Conflict::DefOverride` for
    /// `ThingDef/Wall` across all five.
    pub report: Report,
    /// A [`SourceIndex`] locating every owner's own `ThingDef/Wall` (no
    /// `ParentName` chain — this fixture only needs leaf-field diffs).
    pub sources: SourceIndex,
    /// A reader seeded with every element `sources`' locators name.
    pub reader: InMemoryDefSourceReader,
}

/// A five-owner worked example:
/// `ThingDef/Wall` owned by `ludeon.rimworld < a.mod < b.mod < c.mod
/// < d.mod` (load order). `b.mod` changes `statBases/MaxHitPoints` (200 ->
/// 250), `d.mod` changes `costList/Plasteel` (10 -> 20), and `c.mod`
/// changes `fillPercent` (1.0 -> 0.8) — a field no scope member ever
/// touches, included purely to prove an out-of-scope contributor's change
/// never reaches the diff at all once a scope of `{b.mod, d.mod}`
/// restricts participants to `ludeon.rimworld` (Core, always implicit),
/// `b.mod`, and `d.mod`. `a.mod` changes nothing (a
/// plain, uninteresting owner) and, with `c.mod`, is the finding's
/// out-of-scope pair the scoped preview must report as
/// `Caveat::OutOfScopeOwners`.
#[must_use]
pub fn five_owner_scope_fixture() -> FiveOwnerScopeFixture {
    let ids = ["ludeon.rimworld", "a.mod", "b.mod", "c.mod", "d.mod"];
    // (owner, MaxHitPoints, Plasteel, fillPercent) — only one field ever
    // differs from Core's baseline per non-Core owner, so the diff's
    // `OneSided { by }` attribution is unambiguous.
    let variants: [(&str, u32, u32, &str); 5] = [
        ("ludeon.rimworld", 200, 10, "1.0"),
        ("a.mod", 200, 10, "1.0"),
        ("b.mod", 250, 10, "1.0"),
        ("c.mod", 200, 10, "0.8"),
        ("d.mod", 200, 20, "1.0"),
    ];

    let mut sources = SourceIndex::default();
    let mut elements = BTreeMap::new();
    for (owner_str, max_hit_points, plasteel, fill_percent) in variants {
        let owner = ModId::new(owner_str);
        let entry_locator = locator(&format!("{owner_str}_wall.xml"), 0);
        sources.defs.insert(
            (owner, ("ThingDef".to_string(), "Wall".to_string())),
            vec![DefEntry {
                def_type: "ThingDef".to_string(),
                def_name: "Wall".to_string(),
                may_require: Vec::new(),
                may_require_any_of: Vec::new(),
                parent_name: None,
                locator: entry_locator.clone(),
            }],
        );
        elements.insert(entry_locator,
            format!("<ThingDef><defName>Wall</defName><statBases><MaxHitPoints>{max_hit_points}</MaxHitPoints></statBases><costList><Plasteel>{plasteel}</Plasteel></costList><fillPercent>{fill_percent}</fillPercent></ThingDef>"
            ));
    }

    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("ludeon.rimworld")
        .mod_("a.mod")
        .mod_("b.mod")
        .mod_("c.mod")
        .mod_("d.mod")
        .def_override("ThingDef", "Wall", &ids)
        .build();

    FiveOwnerScopeFixture {
        report,
        sources,
        reader: InMemoryDefSourceReader::new(elements),
    }
}

// -- `def_conflict_view` fixtures --

/// Everything [`head_normal_fixture`] builds.
pub struct HeadNormalFixture {
    /// A [`Report`] naming `core.mod`, `a.mod`, and `b.mod` (in that load
    /// order).
    pub report: Report,
    /// A [`SourceIndex`] locating each owner's own `HeadTypeDef/HeadNormal`
    /// (no `ParentName` chain).
    pub sources: SourceIndex,
    /// A reader seeded with every element `sources`' locators name.
    pub reader: InMemoryDefSourceReader,
}

/// Three owners of `HeadTypeDef/HeadNormal`, disagreeing on *two* fields: two
/// genuine `DiffClass::Conflict` rows (a real conflict needs three
/// owners, two differing from the base to *different* values — the same
/// shape [`conflicting_wall_fixture`] uses for one field, doubled here).
#[must_use]
pub fn head_normal_fixture() -> HeadNormalFixture {
    let core = ModId::new("core.mod");
    let mod_a = ModId::new("a.mod");
    let mod_b = ModId::new("b.mod");

    let core_locator = locator("core_head_normal.xml", 0);
    let a_locator = locator("a_head_normal.xml", 0);
    let b_locator = locator("b_head_normal.xml", 0);

    let mut sources = SourceIndex::default();
    for (owner, entry_locator) in [
        (&core, &core_locator),
        (&mod_a, &a_locator),
        (&mod_b, &b_locator),
    ] {
        sources.defs.insert(
            (
                owner.clone(),
                ("HeadTypeDef".to_string(), "HeadNormal".to_string()),
            ),
            vec![DefEntry {
                def_type: "HeadTypeDef".to_string(),
                def_name: "HeadNormal".to_string(),
                may_require: Vec::new(),
                may_require_any_of: Vec::new(),
                parent_name: None,
                locator: entry_locator.clone(),
            }],
        );
    }
    sources.owners_by_def.insert(
        ("HeadTypeDef".to_string(), "HeadNormal".to_string()),
        vec![core.clone(), mod_a.clone(), mod_b.clone()],
    );

    let mut elements = BTreeMap::new();
    elements.insert(
        core_locator,
        "<HeadTypeDef><defName>HeadNormal</defName><narrowCrownType>Average</narrowCrownType>\
         <beardOffset>0</beardOffset></HeadTypeDef>"
            .to_string(),
    );
    elements.insert(
        a_locator,
        "<HeadTypeDef><defName>HeadNormal</defName><narrowCrownType>Wide</narrowCrownType>\
         <beardOffset>1</beardOffset></HeadTypeDef>"
            .to_string(),
    );
    elements.insert(
        b_locator,
        "<HeadTypeDef><defName>HeadNormal</defName><narrowCrownType>Narrow</narrowCrownType>\
         <beardOffset>2</beardOffset></HeadTypeDef>"
            .to_string(),
    );

    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("core.mod")
        .mod_("a.mod")
        .mod_("b.mod")
        .def_override("HeadTypeDef", "HeadNormal", &["core.mod", "a.mod", "b.mod"])
        .build();

    HeadNormalFixture {
        report,
        sources,
        reader: InMemoryDefSourceReader::new(elements),
    }
}

/// Everything [`plant_density_fixture`] builds.
pub struct PlantDensityFixture {
    /// A [`Report`] naming `core.mod`, `x.mod`, and `y.mod`.
    pub report: Report,
    /// A [`SourceIndex`] locating `core.mod`'s own
    /// `BiomeDef/TemperateForest` and both patchers' top-level operations.
    pub sources: SourceIndex,
    /// A reader seeded with every element `sources`' locators name.
    pub reader: InMemoryDefSourceReader,
}

/// The `BiomeDef/TemperateForest` worked example, but with a genuine
/// `plantDensity` *conflict* rather than an `Agreeing` collision: `x.mod`
/// and `y.mod` patch it to different values, and `x.mod` additionally
/// patches an unrelated field (`wildPlantRegrowDays`) — one Conflict row
/// plus the other fields the ops touch.
#[must_use]
pub fn plant_density_fixture() -> PlantDensityFixture {
    let core = ModId::new("core.mod");
    let mod_x = ModId::new("x.mod");
    let mod_y = ModId::new("y.mod");

    let core_locator = locator("core_temperate_forest.xml", 0);
    let x_density_locator = locator("x_patch.xml", 0);
    let x_regrow_locator = locator("x_patch.xml", 1);
    let y_density_locator = locator("y_patch.xml", 0);

    let mut sources = SourceIndex::default();
    sources.defs.insert(
        (
            core.clone(),
            ("BiomeDef".to_string(), "TemperateForest".to_string()),
        ),
        vec![DefEntry {
            def_type: "BiomeDef".to_string(),
            def_name: "TemperateForest".to_string(),
            may_require: Vec::new(),
            may_require_any_of: Vec::new(),
            parent_name: None,
            locator: core_locator.clone(),
        }],
    );
    sources.owners_by_def.insert(
        ("BiomeDef".to_string(), "TemperateForest".to_string()),
        vec![core.clone()],
    );

    let make_replace_op = |mod_id: &ModId, xpath: &str, op_locator: XmlLocator| IndexedPatchOp {
        mod_id: mod_id.clone(),
        op: PatchOp {
            injected_template_names: std::collections::BTreeSet::new(),
            class: "PatchOperationReplace".to_string(),
            xpath: Some(xpath.to_string()),
            target: None,
            find_mod_context: Vec::new(),
            find_mod_names: Vec::new(),
            may_require: Vec::new(),
            may_require_any_of: Vec::new(),
            is_mutating: true,
            injected_types: BTreeSet::new(),
            injected_paths: BTreeSet::new(),
            conditional_xpath: None,
            is_list_item: false,
            load_folder_gate: Vec::new(),
            sequence_tail: true,
            conditional_branch: None,
            conditional_nomatch_creates: false,
            names_single_def: true,
            value_child_names: std::collections::BTreeSet::new(),
            toggle_active: true,
            value_root_names: Vec::new(),
            value_digest: None,
            locator: op_locator,
        },
    };
    sources.patch_ops_by_def.insert(
        (
            "BiomeDef".to_string(),
            "TemperateForest".to_string(),
            Selector::DefName,
        ),
        vec![
            make_replace_op(
                &mod_x,
                r#"Defs/BiomeDef[defName="TemperateForest"]/plantDensity"#,
                x_density_locator.clone(),
            ),
            make_replace_op(
                &mod_x,
                r#"Defs/BiomeDef[defName="TemperateForest"]/wildPlantRegrowDays"#,
                x_regrow_locator.clone(),
            ),
            make_replace_op(
                &mod_y,
                r#"Defs/BiomeDef[defName="TemperateForest"]/plantDensity"#,
                y_density_locator.clone(),
            ),
        ],
    );

    let mut elements = BTreeMap::new();
    elements.insert(
        core_locator,
        "<BiomeDef><defName>TemperateForest</defName><plantDensity>0.65</plantDensity>\
         <wildPlantRegrowDays>10</wildPlantRegrowDays></BiomeDef>"
            .to_string(),
    );
    elements.insert(
        x_density_locator,
        r#"<Operation Class="PatchOperationReplace">
             <xpath>Defs/BiomeDef[defName="TemperateForest"]/plantDensity</xpath>
             <value><plantDensity>1.2</plantDensity></value>
           </Operation>"#
            .to_string(),
    );
    elements.insert(
        x_regrow_locator,
        r#"<Operation Class="PatchOperationReplace">
             <xpath>Defs/BiomeDef[defName="TemperateForest"]/wildPlantRegrowDays</xpath>
             <value><wildPlantRegrowDays>5</wildPlantRegrowDays></value>
           </Operation>"#
            .to_string(),
    );
    elements.insert(
        y_density_locator,
        r#"<Operation Class="PatchOperationReplace">
             <xpath>Defs/BiomeDef[defName="TemperateForest"]/plantDensity</xpath>
             <value><plantDensity>0.3</plantDensity></value>
           </Operation>"#
            .to_string(),
    );

    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("core.mod")
        .mod_("x.mod")
        .mod_("y.mod")
        .patch_collision("BiomeDef", "TemperateForest", &["x.mod", "y.mod"])
        .build();

    PlantDensityFixture {
        report,
        sources,
        reader: InMemoryDefSourceReader::new(elements),
    }
}

/// [`plant_density_fixture`], but with `x.mod` flagged as a
/// Rimmerge-generated mod — for a test asserting a generated patcher's
/// own toucher row is flagged, never hidden.
#[must_use]
pub fn plant_density_fixture_with_generated_patcher() -> PlantDensityFixture {
    let mut fixture = plant_density_fixture();
    fixture.report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("core.mod")
        .mod_with("x.mod", |m| {
            m.generated = Some(GeneratedMarker {
                kind: rim_analyzer::domain::GeneratedKind::Merge,
                patch_id: None,
                scope: None,
            });
        })
        .mod_("y.mod")
        .patch_collision("BiomeDef", "TemperateForest", &["x.mod", "y.mod"])
        .build();
    fixture
}
