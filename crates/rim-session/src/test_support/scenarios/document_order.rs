//! Document-order scenarios: interleaved defs whose order in the file matters.

use std::collections::BTreeMap;

use rim_analyzer::analysis::SourceIndex;
use rim_analyzer::domain::{DefEntry, ModId, Report};

use crate::test_support::InMemoryDefSourceReader;
use crate::test_support::locator;

/// Everything [`document_order_interleaved_fixture`] builds.
pub struct DocumentOrderFixture {
    /// A [`Report`] naming `core.mod` and `bionics.mod`, with a
    /// `Conflict::DefOverride` for `Sprocket2`.
    pub report: Report,
    /// A [`SourceIndex`] with both owners' own `ThingDef/Sprocket2`, the
    /// winner's own raw XML declaring a scalar, then a list, then another
    /// scalar, in that literal document order.
    pub sources: SourceIndex,
    /// A reader seeded with every element `sources`' locators name.
    pub reader: InMemoryDefSourceReader,
}

/// `bionics.mod` (the winner) declares `fieldA`, then `comps` (one `li`),
/// then `fieldZ`, in that literal order; `core.mod` (the base) differs on
/// all three (no `comps` at all). The list entry must sort *between* the
/// two scalar
/// rows in the final field list, matching `effective.resolved`'s own
/// document order — not after both of them, wherever
/// `FieldRowKind::ListEntry` happened to rank.
#[must_use]
pub fn document_order_interleaved_fixture() -> DocumentOrderFixture {
    let core = ModId::new("core.mod");
    let bionics = ModId::new("bionics.mod");

    let core_locator = locator("core_sprocket2.xml", 0);
    let bionics_locator = locator("bionics_sprocket2.xml", 0);

    let mut sources = SourceIndex::default();
    sources.defs.insert(
        (
            core.clone(),
            ("ThingDef".to_string(), "Sprocket2".to_string()),
        ),
        vec![DefEntry {
            def_type: "ThingDef".to_string(),
            def_name: "Sprocket2".to_string(),
            may_require: Vec::new(),
            may_require_any_of: Vec::new(),
            parent_name: None,
            locator: core_locator.clone(),
        }],
    );
    sources.defs.insert(
        (
            bionics.clone(),
            ("ThingDef".to_string(), "Sprocket2".to_string()),
        ),
        vec![DefEntry {
            def_type: "ThingDef".to_string(),
            def_name: "Sprocket2".to_string(),
            may_require: Vec::new(),
            may_require_any_of: Vec::new(),
            parent_name: None,
            locator: bionics_locator.clone(),
        }],
    );
    sources.owners_by_def.insert(
        ("ThingDef".to_string(), "Sprocket2".to_string()),
        vec![core.clone(), bionics.clone()],
    );

    let mut elements = BTreeMap::new();
    elements.insert(
        core_locator,
        "<ThingDef><defName>Sprocket2</defName><fieldA>core-a</fieldA>\
         <fieldZ>core-z</fieldZ></ThingDef>"
            .to_string(),
    );
    elements.insert(
        bionics_locator,
        r#"<ThingDef><defName>Sprocket2</defName><fieldA>bionics-a</fieldA>
           <comps><li Class="CompProperties_Foo"/></comps>
           <fieldZ>bionics-z</fieldZ></ThingDef>"#
            .to_string(),
    );

    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("core.mod")
        .mod_("bionics.mod")
        .def_override("ThingDef", "Sprocket2", &["core.mod", "bionics.mod"])
        .build();

    DocumentOrderFixture {
        report,
        sources,
        reader: InMemoryDefSourceReader::new(elements),
    }
}
