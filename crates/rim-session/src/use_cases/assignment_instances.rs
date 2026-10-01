//! [`AssignmentInstances`]: every active instance of one assignment def
//! type, flattened and cached —
//! the expensive read `CreateAssignment`/`UpdateAssignment` share.

use std::collections::BTreeSet;
use std::sync::Arc;

use rim_analyzer::domain::{ModId, Selector};
use rim_resolve::domain::InstanceValues;

use super::def_sources::{self, DefSourceLookupError};
use crate::Session;
use crate::ports::{DefSourceError, DefSourceReader};

/// Everything that can go wrong reading every active instance of one
/// assignment def type.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AssignmentInstancesError {
    /// An instance's XML couldn't be read back — the scan is stale
    /// relative to what's on disk now, or the file itself couldn't be
    /// read.
    #[error(transparent)]
    Source(#[from] DefSourceError),
    /// The read-back XML text failed to parse.
    #[error("parsing merge XML: {0}")]
    Xml(String),
    /// The source index has no record of an owner the scan itself just
    /// named — a stale or inconsistent scan. Unreachable in ordinary use
    /// (every `def_name` this reads comes straight from
    /// [`rim_analyzer::analysis::SourceIndex::owners_by_def`]'s own keys),
    /// kept as an honest answer rather than a panic if it somehow still
    /// fires (see `super::inspect_def`'s `map_lookup_error` for the same
    /// reasoning).
    #[error("{0}")]
    MissingSource(String),
}

impl From<DefSourceLookupError> for AssignmentInstancesError {
    fn from(error: DefSourceLookupError) -> Self {
        match error {
            DefSourceLookupError::Source(source) => Self::Source(source),
            DefSourceLookupError::Xml(message) => Self::Xml(message),
            DefSourceLookupError::MissingSource(message) => Self::MissingSource(message),
        }
    }
}

/// Every active `def_name` of `def_type`, sorted — the one ordering
/// [`AssignmentInstances::execute`]'s own returned `Vec` is built in, and
/// the ordering [`AssignmentInstances::find_by_name`] indexes into it
/// with. Factored out so the two never drift apart (see `find_by_name`'s
/// own doc comment for why a caller must not recompute this same
/// filter/sort independently).
fn def_names(session: &Session, def_type: &str) -> BTreeSet<String> {
    session
        .sources()
        .owners_by_def
        .keys()
        .filter(|(candidate_type, _)| candidate_type == def_type)
        .map(|(_, def_name)| def_name.clone())
        .collect()
}

/// Reads (and caches) every active instance of one assignment def type,
/// flattened via [`rim_merge::assign::read_instance`] — "read once per
/// session and reused by inference and coverage".
pub struct AssignmentInstances<Reader> {
    reader: Reader,
}

impl<Reader: DefSourceReader> AssignmentInstances<Reader> {
    /// Builds the use case from its port.
    #[must_use]
    pub fn new(reader: Reader) -> Self {
        Self { reader }
    }

    /// Every active instance of `def_type`, under [`Session::selected`]'s
    /// order, as `(owner, flattened fields)` pairs — reused from
    /// [`Session`]'s own cache on a hit.
    ///
    /// # Errors
    ///
    /// Returns [`AssignmentInstancesError`] the moment any one instance
    /// fails to read or parse — never a partial result silently missing
    /// that instance's own evidence (a stale file surfaces as
    /// `DefSourceError::Stale`, never a partial schema). A failed read is never cached, so a transient
    /// failure (a file that changes back, or a later rescan) doesn't wedge
    /// this call permanently.
    pub fn execute(
        &self,
        session: &mut Session,
        def_type: &str,
    ) -> Result<Arc<Vec<(ModId, InstanceValues)>>, AssignmentInstancesError> {
        let source = session.selected();
        if let Some(cached) = session.cached_assignment_instances(source, def_type) {
            return Ok(Arc::clone(cached));
        }

        let order = session.orders().get(source).clone();
        let names = def_names(session, def_type);

        let mut instances = Vec::with_capacity(names.len());
        for def_name in &names {
            let (owner, tree) = def_sources::def_owner_and_raw(
                &self.reader,
                session,
                &order,
                def_type,
                def_name,
                Selector::DefName,
            )?;
            instances.push((owner, rim_merge::assign::read_instance(&tree)));
        }

        let instances = Arc::new(instances);
        session.cache_assignment_instances(source, def_type.to_string(), Arc::clone(&instances));
        Ok(instances)
    }

    /// Locates one active instance of `def_type` by its own `defName`,
    /// e.g. for a "copy from" picker that names one specific existing
    /// instance. [`Self::execute`]'s own returned `Vec` never carries an
    /// instance's own `defName` (`rim_merge::assign::read_instance`
    /// deliberately drops it — an instance's identity would otherwise
    /// pollute inference), so a caller that already knows which `defName`
    /// it wants has no way to pick it out of that `Vec` directly. This
    /// method is the one place that gap is closed: it indexes into
    /// `execute`'s own (cached) result at `def_name`'s position in the
    /// same sorted [`def_names`] ordering `execute` itself builds from —
    /// never a second, independently-recomputed ordering a caller might
    /// let drift out of sync with this one.
    ///
    /// # Errors
    ///
    /// See [`Self::execute`]. Returns `Ok(None)` (not an error) when no
    /// active instance of `def_type` is named `def_name`.
    pub fn find_by_name(
        &self,
        session: &mut Session,
        def_type: &str,
        def_name: &str,
    ) -> Result<Option<(ModId, InstanceValues)>, AssignmentInstancesError> {
        let instances = self.execute(session, def_type)?;
        let names = def_names(session, def_type);
        Ok(names
            .iter()
            .position(|name| name == def_name)
            .and_then(|index| instances.get(index).cloned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::ElementExpectation;
    use crate::test_support::{report_fixture, session_with_sources_and_mods};
    use rim_analyzer::analysis::SourceIndex;
    use rim_analyzer::domain::{DefEntry, XmlLocator};
    use std::collections::BTreeMap;
    use std::path::Path;
    use std::sync::Arc as StdArc;

    /// A [`DefSourceReader`] fake backed by a fixed `(tag, def_name) ->
    /// XML text` map — enough for these tests, which never touch a
    /// template or a patch op.
    struct FakeReader {
        by_locator_ordinal: BTreeMap<u32, String>,
    }

    impl DefSourceReader for FakeReader {
        fn read_element(
            &self,
            locator: &XmlLocator,
            _expected: &ElementExpectation,
        ) -> Result<String, DefSourceError> {
            self.by_locator_ordinal
                .get(&locator.element_path[0])
                .cloned()
                .ok_or_else(|| DefSourceError::Io {
                    file: locator.file.to_path_buf(),
                    message: "not seeded".to_string(),
                })
        }
    }

    fn locator(ordinal: u32) -> XmlLocator {
        XmlLocator::new(StdArc::from(Path::new("Defs/fixture.xml")), vec![ordinal])
    }

    fn source_index_with_two_instances() -> (SourceIndex, FakeReader) {
        let mut index = SourceIndex::default();
        index.defs.insert(
            (
                ModId::new("mod.a"),
                (
                    "example.PartAssignmentDef".to_string(),
                    "Group_Human".to_string(),
                ),
            ),
            vec![DefEntry {
                def_type: "example.PartAssignmentDef".to_string(),
                def_name: "Group_Human".to_string(),
                may_require: Vec::new(),
                may_require_any_of: Vec::new(),
                parent_name: None,
                locator: locator(0),
            }],
        );
        index.owners_by_def.insert(
            (
                "example.PartAssignmentDef".to_string(),
                "Group_Human".to_string(),
            ),
            vec![ModId::new("mod.a")],
        );
        index.defs.insert(
            (
                ModId::new("mod.a"),
                (
                    "example.PartAssignmentDef".to_string(),
                    "Group_Elf".to_string(),
                ),
            ),
            vec![DefEntry {
                def_type: "example.PartAssignmentDef".to_string(),
                def_name: "Group_Elf".to_string(),
                may_require: Vec::new(),
                may_require_any_of: Vec::new(),
                parent_name: None,
                locator: locator(1),
            }],
        );
        index.owners_by_def.insert(
            (
                "example.PartAssignmentDef".to_string(),
                "Group_Elf".to_string(),
            ),
            vec![ModId::new("mod.a")],
        );
        // Never scoped to this def type at all — proves the query never
        // reads an unrelated def's own XML.
        index.defs.insert(
            (
                ModId::new("mod.a"),
                ("ThingDef".to_string(), "Wall".to_string()),
            ),
            vec![DefEntry {
                def_type: "ThingDef".to_string(),
                def_name: "Wall".to_string(),
                may_require: Vec::new(),
                may_require_any_of: Vec::new(),
                parent_name: None,
                locator: locator(2),
            }],
        );
        index.owners_by_def.insert(
            ("ThingDef".to_string(), "Wall".to_string()),
            vec![ModId::new("mod.a")],
        );

        let reader = FakeReader {
            by_locator_ordinal: BTreeMap::from([
                (0,
                    "<example.PartAssignmentDef><defName>Group_Human</defName><speciesNames><li>Human</li></speciesNames></example.PartAssignmentDef>"
                        .to_string()),
                (1,
                    "<example.PartAssignmentDef><defName>Group_Elf</defName><speciesNames><li>Elf</li></speciesNames></example.PartAssignmentDef>"
                        .to_string()),
                (2, "<ThingDef><defName>Wall</defName></ThingDef>".to_string()),
            ]),
        };
        (index, reader)
    }

    #[test]
    fn reads_every_active_instance_of_the_def_type_and_no_others() {
        let (sources, reader) = source_index_with_two_instances();
        let mut session =
            session_with_sources_and_mods(sources, report_fixture(&["mod.a"]), &["mod.a"]);
        let use_case = AssignmentInstances::new(reader);

        let instances = use_case
            .execute(&mut session, "example.PartAssignmentDef")
            .expect("read must succeed");

        assert_eq!(instances.len(), 2);
        let names: BTreeSet<String> = instances
            .iter()
            .flat_map(|(_, values)| values.get(&"speciesNames".parse().unwrap()))
            .flat_map(|occurrence| occurrence.values.clone())
            .collect();
        assert_eq!(
            names,
            BTreeSet::from(["Human".to_string(), "Elf".to_string()])
        );
    }

    #[test]
    fn a_second_call_reuses_the_cache_without_reading_again() {
        let (sources, reader) = source_index_with_two_instances();
        let mut session =
            session_with_sources_and_mods(sources, report_fixture(&["mod.a"]), &["mod.a"]);
        let use_case = AssignmentInstances::new(reader);

        let first = use_case
            .execute(&mut session, "example.PartAssignmentDef")
            .expect("first read must succeed");
        // A reader that always errors: if this returns `Ok`, it must have
        // come from the cache, not a second read.
        let use_case_that_would_fail = AssignmentInstances::new(FakeReader {
            by_locator_ordinal: BTreeMap::new(),
        });
        let second = use_case_that_would_fail
            .execute(&mut session, "example.PartAssignmentDef")
            .expect("a cache hit must succeed even though this reader would fail on a real read");

        assert!(StdArc::ptr_eq(&first, &second));
    }

    #[test]
    fn a_stale_file_fails_the_whole_call_rather_than_a_partial_result() {
        let (sources, _reader) = source_index_with_two_instances();
        // A reader that can serve the first instance but not the second —
        // simulating a scan that's gone stale relative to disk for just
        // one of the two files.
        let reader = FakeReader {
            by_locator_ordinal: BTreeMap::from([(0,
                "<example.PartAssignmentDef><defName>Group_Human</defName></example.PartAssignmentDef>".to_string())]),
        };
        let mut session =
            session_with_sources_and_mods(sources, report_fixture(&["mod.a"]), &["mod.a"]);
        let use_case = AssignmentInstances::new(reader);

        let result = use_case.execute(&mut session, "example.PartAssignmentDef");

        assert!(
            result.is_err(),
            "one instance failing to read must fail the whole call, not silently drop it"
        );
    }

    #[test]
    fn find_by_name_locates_one_active_instance_by_its_own_def_name() {
        let (sources, reader) = source_index_with_two_instances();
        let mut session =
            session_with_sources_and_mods(sources, report_fixture(&["mod.a"]), &["mod.a"]);
        let use_case = AssignmentInstances::new(reader);

        let (owner, values) = use_case
            .find_by_name(&mut session, "example.PartAssignmentDef", "Group_Elf")
            .expect("lookup must succeed")
            .expect("Group_Elf is an active instance of this type");

        assert_eq!(owner, ModId::new("mod.a"));
        let race_names: Vec<String> = values
            .get(&"speciesNames".parse().unwrap())
            .expect("speciesNames must be present")
            .values
            .clone();
        assert_eq!(race_names, vec!["Elf".to_string()]);
    }

    #[test]
    fn find_by_name_returns_none_for_a_name_that_does_not_exist() {
        let (sources, reader) = source_index_with_two_instances();
        let mut session =
            session_with_sources_and_mods(sources, report_fixture(&["mod.a"]), &["mod.a"]);
        let use_case = AssignmentInstances::new(reader);

        let found = use_case
            .find_by_name(&mut session, "example.PartAssignmentDef", "Group_Dwarf")
            .expect("lookup must succeed (an absent name is not an error)");

        assert!(found.is_none());
    }

    #[test]
    fn find_by_name_reuses_the_cache_the_same_way_execute_does() {
        let (sources, reader) = source_index_with_two_instances();
        let mut session =
            session_with_sources_and_mods(sources, report_fixture(&["mod.a"]), &["mod.a"]);
        AssignmentInstances::new(reader)
            .execute(&mut session, "example.PartAssignmentDef")
            .expect("priming read must succeed");

        // A reader that always errors: if this still finds the instance,
        // the lookup came from the cache `execute` already populated.
        let use_case_that_would_fail = AssignmentInstances::new(FakeReader {
            by_locator_ordinal: BTreeMap::new(),
        });
        let found = use_case_that_would_fail
            .find_by_name(&mut session, "example.PartAssignmentDef", "Group_Human")
            .expect("a cache hit must succeed even though this reader would fail on a real read");

        assert!(found.is_some());
    }
}
