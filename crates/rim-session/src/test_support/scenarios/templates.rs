//! Template (`Name=`/`ParentName=`) scenarios: duplicate template names and overrides whose
//! template is missing.

use std::collections::BTreeMap;

use rim_analyzer::analysis::SourceIndex;
use rim_analyzer::domain::{DefEntry, ModId, Report, TemplateEntry};

use crate::test_support::InMemoryDefSourceReader;
use crate::test_support::locator;

/// Everything [`duplicate_template_name_fixture`] builds.
pub struct DuplicateTemplateNameFixture {
    /// A [`Report`] naming `a.mod` and `b.mod`, with a
    /// `Conflict::DuplicateTemplateName` for `Base`.
    pub report: Report,
    /// A [`SourceIndex`] with both mods registering their own
    /// `ThingDef Name="Base"` template.
    pub sources: SourceIndex,
    /// A reader seeded with every element `sources`' locators name.
    pub reader: InMemoryDefSourceReader,
}

/// Two mods each register their own `ThingDef Name="Base"` abstract
/// template — `FindingKey::DuplicateTemplateName`'s own scenario. It
/// exercises the one finding kind `def_conflict_view` builds with no
/// fields (`PlanMerge` has no support for it, so
/// `DefConflictView::fields` is always empty).
#[must_use]
pub fn duplicate_template_name_fixture() -> DuplicateTemplateNameFixture {
    let mod_a = ModId::new("a.mod");
    let mod_b = ModId::new("b.mod");
    let a_locator = locator("a_base_template.xml", 0);
    let b_locator = locator("b_base_template.xml", 0);

    let mut sources = SourceIndex::default();
    sources.templates.insert(
        ("ThingDef".to_string(), "Base".to_string()),
        vec![
            (
                mod_a.clone(),
                TemplateEntry {
                    graphic_class: None,
                    may_require: Vec::new(),
                    def_type: "ThingDef".to_string(),
                    name: "Base".to_string(),
                    parent_name: None,
                    is_abstract: true,
                    locator: a_locator.clone(),
                },
            ),
            (
                mod_b.clone(),
                TemplateEntry {
                    graphic_class: None,
                    may_require: Vec::new(),
                    def_type: "ThingDef".to_string(),
                    name: "Base".to_string(),
                    parent_name: None,
                    is_abstract: true,
                    locator: b_locator.clone(),
                },
            ),
        ],
    );

    let mut elements = BTreeMap::new();
    elements.insert(a_locator,
        "<ThingDef Name=\"Base\"><statBases><MaxHitPoints>100</MaxHitPoints></statBases></ThingDef>"
            .to_string());
    elements.insert(b_locator,
        "<ThingDef Name=\"Base\"><statBases><MaxHitPoints>150</MaxHitPoints></statBases></ThingDef>"
            .to_string());

    let mut report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("a.mod")
        .mod_("b.mod")
        .build();
    report
        .conflicts
        .push(rim_analyzer::domain::Conflict::DuplicateTemplateName(
            rim_analyzer::domain::DuplicateTemplateName {
                name: "Base".to_string(),
                owners: vec![mod_a.clone(), mod_b.clone()],
            },
        ));

    DuplicateTemplateNameFixture {
        report,
        sources,
        reader: InMemoryDefSourceReader::new(elements),
    }
}

/// Everything [`duplicate_template_name_with_child_fixture`] builds.
pub struct DuplicateTemplateNameWithChildFixture {
    /// A [`Report`] naming `core.mod`, `zzz_other.mod`, `mmm_winner.mod`,
    /// and `aaa_child.mod`, with a `Conflict::DuplicateTemplateName` for
    /// `Base` across `zzz_other.mod`/`mmm_winner.mod`. The three mod names
    /// are deliberately in the *reverse* of the order the decision must
    /// produce (`zzz_other` sorts last alphabetically, `aaa_child` first)
    /// — see [`duplicate_template_name_with_child_fixture`]'s own doc
    /// comment for why that matters.
    pub report: Report,
    /// A [`SourceIndex`] with both mods registering `ThingDef Name="Base"`,
    /// plus `aaa_child.mod` owning a `ThingDef ParentName="Base"` child
    /// (`Wall`) — the `duplicate_template_children` scenario.
    pub sources: SourceIndex,
}

/// [`duplicate_template_name_fixture`]'s sibling, extended with a real
/// *child*: `aaa_child.mod` owns
/// `ThingDef/Wall`, whose own `ParentName="Base"` makes it a child of the
/// duplicated template — exactly the shape `Session::decide`'s own
/// `duplicate_template_children` must find via
/// `SourceIndex::children_by_template` to make a `PreferWinner` decision
/// on this finding do anything at all. No `DefSourceReader`/raw XML is
/// needed: the sorter-side expansion never reads a child's content, only
/// its owner.
///
/// Mod names are chosen so `TieBreak::Rebuild`'s own natural (alphabetical)
/// order is the *exact reverse* of what a `PreferWinner` decision for
/// `mmm_winner.mod` must produce (`zzz_other.mod` before `mmm_winner.mod`
/// before `aaa_child.mod`) — natural order alone would put
/// `aaa_child.mod` first and `zzz_other.mod` last, so a caller's own test
/// can only pass by the Reorder pairs actually taking effect, never by
/// coincidence of name order.
#[must_use]
pub fn duplicate_template_name_with_child_fixture() -> DuplicateTemplateNameWithChildFixture {
    let mod_other = ModId::new("zzz_other.mod");
    let mod_winner = ModId::new("mmm_winner.mod");
    let mod_child = ModId::new("aaa_child.mod");
    let other_locator = locator("other_base_template.xml", 0);
    let winner_locator = locator("winner_base_template.xml", 0);
    let child_locator = locator("child_wall.xml", 0);

    let mut sources = SourceIndex::default();
    sources.templates.insert(
        ("ThingDef".to_string(), "Base".to_string()),
        vec![
            (
                mod_other.clone(),
                TemplateEntry {
                    graphic_class: None,
                    may_require: Vec::new(),
                    def_type: "ThingDef".to_string(),
                    name: "Base".to_string(),
                    parent_name: None,
                    is_abstract: true,
                    locator: other_locator,
                },
            ),
            (
                mod_winner.clone(),
                TemplateEntry {
                    graphic_class: None,
                    may_require: Vec::new(),
                    def_type: "ThingDef".to_string(),
                    name: "Base".to_string(),
                    parent_name: None,
                    is_abstract: true,
                    locator: winner_locator,
                },
            ),
        ],
    );
    sources.children_by_template.insert(
        ("ThingDef".to_string(), "Base".to_string()),
        vec![(
            mod_child.clone(),
            ("ThingDef".to_string(), "Wall".to_string()),
        )],
    );
    sources.defs.insert(
        (
            mod_child.clone(),
            ("ThingDef".to_string(), "Wall".to_string()),
        ),
        vec![DefEntry {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
            may_require: Vec::new(),
            may_require_any_of: Vec::new(),
            parent_name: Some("Base".to_string()),
            locator: child_locator,
        }],
    );

    let mut report = rim_resolve::test_support::ReportBuilder::new()
        .core("core.mod")
        .mod_("zzz_other.mod")
        .mod_("mmm_winner.mod")
        .mod_("aaa_child.mod")
        .build();
    report
        .conflicts
        .push(rim_analyzer::domain::Conflict::DuplicateTemplateName(
            rim_analyzer::domain::DuplicateTemplateName {
                name: "Base".to_string(),
                owners: vec![mod_other, mod_winner],
            },
        ));

    DuplicateTemplateNameWithChildFixture { report, sources }
}

/// Everything [`def_override_missing_template_fixture`] builds.
pub struct DefOverrideMissingTemplateFixture {
    /// A [`Report`] naming `core.mod` and `broken.mod`, with a
    /// `Conflict::DefOverride` for `Gizmo`.
    pub report: Report,
    /// A [`SourceIndex`] where `broken.mod`'s own `Gizmo` declares a
    /// `ParentName` no active mod registers a template for.
    pub sources: SourceIndex,
    /// A reader seeded with every element `sources`' locators name.
    pub reader: InMemoryDefSourceReader,
}

/// `core.mod` owns a plain `ThingDef/Gizmo`; `broken.mod` (the winner,
/// loading after) redeclares it with `ParentName="ReallyMissing"` — a
/// name no active mod registers anywhere. A regression fixture:
/// `PlanMerge::execute`'s own `plan_def_override`
/// hard-errors on this (`def_sources::template_set` returns
/// `PlanMergeError::MissingSource`, per `crates/rim-session/CLAUDE.md`'s
/// own note), caching no preview at all — while `InspectDef::execute`'s
/// own, more lenient `template_chain` stops silently and still builds a
/// usable inspection (`effective.completeness` reports the identical gap
/// as `Stopper::Inherit(InheritError::MissingParent)`).
/// `Session::def_conflict_view` must still render a full panel from
/// `EffectiveDef::provenance` alone, never `NotPlanned`.
#[must_use]
pub fn def_override_missing_template_fixture() -> DefOverrideMissingTemplateFixture {
    let core = ModId::new("core.mod");
    let broken = ModId::new("broken.mod");

    let core_locator = locator("core_gizmo.xml", 0);
    let broken_locator = locator("broken_gizmo.xml", 0);

    let mut sources = SourceIndex::default();
    sources.defs.insert(
        (core.clone(), ("ThingDef".to_string(), "Gizmo".to_string())),
        vec![DefEntry {
            def_type: "ThingDef".to_string(),
            def_name: "Gizmo".to_string(),
            may_require: Vec::new(),
            may_require_any_of: Vec::new(),
            parent_name: None,
            locator: core_locator.clone(),
        }],
    );
    sources.defs.insert(
        (
            broken.clone(),
            ("ThingDef".to_string(), "Gizmo".to_string()),
        ),
        vec![DefEntry {
            def_type: "ThingDef".to_string(),
            def_name: "Gizmo".to_string(),
            may_require: Vec::new(),
            may_require_any_of: Vec::new(),
            parent_name: Some("ReallyMissing".to_string()),
            locator: broken_locator.clone(),
        }],
    );
    sources.owners_by_def.insert(
        ("ThingDef".to_string(), "Gizmo".to_string()),
        vec![core.clone(), broken.clone()],
    );

    let mut elements = BTreeMap::new();
    elements.insert(
        core_locator,
        "<ThingDef><defName>Gizmo</defName><label>core label</label></ThingDef>".to_string(),
    );
    elements.insert(
        broken_locator,
        r#"<ThingDef ParentName="ReallyMissing"><defName>Gizmo</defName>
           <label>broken label</label><description>broken desc</description></ThingDef>"#
            .to_string(),
    );

    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("core.mod")
        .mod_("broken.mod")
        .def_override("ThingDef", "Gizmo", &["core.mod", "broken.mod"])
        .build();

    DefOverrideMissingTemplateFixture {
        report,
        sources,
        reader: InMemoryDefSourceReader::new(elements),
    }
}

/// [`def_override_missing_template_fixture`]'s own mirror image, with the
/// broken owner and the winner swapped: `broken.mod` loads *first* here
/// (never the
/// winner) with the identical `ParentName="ReallyMissing"` gap, and
/// `clean.mod` loads after it with a plain, resolvable `Gizmo` — so
/// `clean.mod` wins the override cleanly. `InspectDef::execute`'s own
/// `effective.completeness` is derived only from the *winning* owner's
/// own template chain (`clean.mod`'s, which never touches `broken.mod`'s
/// gap at all), so it reports `Complete` even though `PlanMerge::execute`
/// still hard-errors the same way — every participant's chain, winner or
/// not, has to resolve for the full multi-owner diff. Pins that a
/// losing owner's own broken chain must still surface as a `Problem`
/// somewhere in the view, never silently disappear behind a `Complete`
/// pill just because the winner itself is clean.
#[must_use]
pub fn def_override_missing_template_on_a_losing_owner_fixture() -> DefOverrideMissingTemplateFixture
{
    let broken = ModId::new("broken.mod");
    let clean = ModId::new("clean.mod");

    let broken_locator = locator("broken_gizmo.xml", 0);
    let clean_locator = locator("clean_gizmo.xml", 0);

    let mut sources = SourceIndex::default();
    sources.defs.insert(
        (
            broken.clone(),
            ("ThingDef".to_string(), "Gizmo".to_string()),
        ),
        vec![DefEntry {
            def_type: "ThingDef".to_string(),
            def_name: "Gizmo".to_string(),
            may_require: Vec::new(),
            may_require_any_of: Vec::new(),
            parent_name: Some("ReallyMissing".to_string()),
            locator: broken_locator.clone(),
        }],
    );
    sources.defs.insert(
        (clean.clone(), ("ThingDef".to_string(), "Gizmo".to_string())),
        vec![DefEntry {
            def_type: "ThingDef".to_string(),
            def_name: "Gizmo".to_string(),
            may_require: Vec::new(),
            may_require_any_of: Vec::new(),
            parent_name: None,
            locator: clean_locator.clone(),
        }],
    );
    sources.owners_by_def.insert(
        ("ThingDef".to_string(), "Gizmo".to_string()),
        vec![broken.clone(), clean.clone()],
    );

    let mut elements = BTreeMap::new();
    elements.insert(
        broken_locator,
        r#"<ThingDef ParentName="ReallyMissing"><defName>Gizmo</defName>
           <label>broken label</label><description>broken desc</description></ThingDef>"#
            .to_string(),
    );
    elements.insert(
        clean_locator,
        "<ThingDef><defName>Gizmo</defName><label>clean label</label></ThingDef>".to_string(),
    );

    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("broken.mod")
        .mod_("clean.mod")
        .def_override("ThingDef", "Gizmo", &["broken.mod", "clean.mod"])
        .build();

    DefOverrideMissingTemplateFixture {
        report,
        sources,
        reader: InMemoryDefSourceReader::new(elements),
    }
}
