//! [`PreviewOrderImport`]: parses a shared list (a file or pasted text)
//! and diffs it against this install. Read-only: nothing is activated,
//! written or scanned.

use std::path::Path;

use rim_analyzer::domain::ModId;

use crate::Session;
use crate::mod_inventory::ModInventory;
use crate::mod_list::{
    ImportContext, ImportPlan, ParsedModList, Rejection, SkippedEntry, parse_text, plan_import,
};
use crate::ports::{ModListFileError, ModListFileStore, ModListRead};

/// What an import is diffed against: the receiver's inventory, the
/// `ModsConfig.xml` order as last known, and the context facts.
#[derive(Debug, Clone)]
pub struct ImportTarget {
    /// Every mod this install knows, by exact id.
    pub inventory: ModInventory,
    /// `ModsConfig.xml`'s active list, by exact ids, in file order.
    pub file_ids: Vec<ModId>,
    /// The receiver's own merge mod, game version and pending edits.
    pub context: ImportContext,
}

impl ImportTarget {
    /// The target for a loaded session. `has_pending_changes` is the
    /// session's unrescanned activation edits, which an import replaces.
    #[must_use]
    pub fn from_session(session: &Session) -> Self {
        Self {
            inventory: ModInventory::from_report(session.report()),
            file_ids: session.file_active_mods().to_vec(),
            context: ImportContext {
                own_merge_mod: session.own_merge_mod_id(),
                game_version: session.game_version(),
                has_pending_changes: session.is_stale(),
            },
        }
    }
}

/// A list that parsed, and what importing it would do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadyImport {
    /// The diff against this install.
    pub plan: ImportPlan,
    /// What the parser left out of the list, bounded.
    pub skipped: Vec<SkippedEntry>,
    /// How many further skipped parts were not recorded in `skipped`.
    pub omitted_skipped: usize,
}

/// The outcome of previewing an import. A document that cannot be read as
/// a list is an outcome the caller shows, not an error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportPreview {
    /// The input is not an importable list.
    Rejected(Rejection),
    /// The input parsed; here is the diff.
    Ready(ReadyImport),
}

/// The input could not be read at all.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PreviewImportError {
    /// The mod-list file could not be opened or read.
    #[error("cannot read the mod list: {0}")]
    Unreadable(#[from] ModListFileError),
}

/// Previews importing a shared list.
pub struct PreviewOrderImport<Store> {
    store: Store,
}

impl<Store: ModListFileStore> PreviewOrderImport<Store> {
    /// Wires the mod-list file store [`Self::from_file`] reads through.
    #[must_use]
    pub fn new(store: Store) -> Self {
        Self { store }
    }

    /// Previews the list in the file at `path`.
    ///
    /// # Errors
    ///
    /// [`PreviewImportError::Unreadable`] when the file cannot be read; a
    /// file that is not a list is [`ImportPreview::Rejected`] instead.
    pub fn from_file(
        &self,
        path: &Path,
        target: &ImportTarget,
    ) -> Result<ImportPreview, PreviewImportError> {
        Ok(ImportPreview::from_read(self.store.read(path)?, target))
    }
}

impl ImportPreview {
    /// Previews the outcome of reading a list from any source (a file, or
    /// bytes an interface read itself).
    #[must_use]
    pub fn from_read(read: ModListRead, target: &ImportTarget) -> Self {
        match read {
            ModListRead::Parsed(parsed) => ready(parsed, target),
            ModListRead::Rejected(rejection) => Self::Rejected(rejection),
        }
    }

    /// Previews pasted text (the "Copy as text" format, or one package id
    /// per line). Needs no store, so it cannot fail to read.
    #[must_use]
    pub fn from_text(text: &str, target: &ImportTarget) -> Self {
        match parse_text(text) {
            Ok(parsed) => ready(parsed, target),
            Err(rejection) => Self::Rejected(rejection),
        }
    }
}

fn ready(parsed: ParsedModList, target: &ImportTarget) -> ImportPreview {
    let plan = plan_import(
        &parsed.list,
        &target.inventory,
        &target.file_ids,
        target.context.clone(),
    );
    ImportPreview::Ready(ReadyImport {
        plan,
        skipped: parsed.skipped,
        omitted_skipped: parsed.omitted_skipped,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mod_list::{ImportedEntry, ModListLimits, SharedModList};
    use crate::test_support::{InMemoryModListFileStore, session_fixture};
    use rim_resolve::test_support::ReportBuilder;

    fn target(active: &[&str], inactive: &[&str]) -> ImportTarget {
        let mut builder = ReportBuilder::new().core("ludeon.rimworld");
        for id in active {
            builder = builder.mod_(id);
        }
        for id in inactive {
            builder = builder.inactive(id);
        }
        let file_ids = std::iter::once("ludeon.rimworld")
            .chain(active.iter().copied())
            .map(ModId::new)
            .collect();
        ImportTarget {
            inventory: ModInventory::from_report(&builder.build()),
            file_ids,
            context: ImportContext {
                own_merge_mod: ModId::new("rimmerge.merge.3f9a1c2b7d5e"),
                game_version: None,
                has_pending_changes: false,
            },
        }
    }

    fn parsed(text: &str) -> ParsedModList {
        parse_text(text).expect("fixture text parses")
    }

    fn preview_of(store: InMemoryModListFileStore) -> PreviewOrderImport<InMemoryModListFileStore> {
        PreviewOrderImport::new(store)
    }

    fn ready_plan(preview: ImportPreview) -> ReadyImport {
        match preview {
            ImportPreview::Ready(ready) => ready,
            ImportPreview::Rejected(rejection) => panic!("rejected: {rejection:?}"),
        }
    }

    #[test]
    fn from_file_diffs_a_parsed_list_against_the_install() {
        let store = InMemoryModListFileStore::new().with_file(
            "shared.rml",
            ModListRead::Parsed(parsed("ludeon.rimworld\nexample.extra\nghost.mod\n")),
        );

        let ready = ready_plan(
            preview_of(store)
                .from_file(
                    Path::new("shared.rml"),
                    &target(&["example.base"], &["example.extra"]),
                )
                .expect("reads"),
        );

        assert_eq!(
            ready.plan.order,
            [ModId::new("ludeon.rimworld"), ModId::new("example.extra")]
        );
        assert_eq!(ready.plan.deactivated, [ModId::new("example.base")]);
        assert!(matches!(
            ready.plan.entries[2],
            ImportedEntry::NotInstalled { .. }
        ));
    }

    #[test]
    fn from_file_turns_a_store_rejection_into_an_outcome() {
        let store = InMemoryModListFileStore::new()
            .with_file("bad.xml", ModListRead::Rejected(Rejection::DtdNotAllowed));

        let preview = preview_of(store)
            .from_file(Path::new("bad.xml"), &target(&[], &[]))
            .expect("a rejection is not an error");

        assert_eq!(preview, ImportPreview::Rejected(Rejection::DtdNotAllowed));
    }

    #[test]
    fn from_file_reports_an_unreadable_file_as_an_error() {
        let error = preview_of(InMemoryModListFileStore::new())
            .from_file(Path::new("absent.rml"), &target(&[], &[]))
            .expect_err("nothing seeded at that path");

        assert!(
            matches!(error, PreviewImportError::Unreadable(_)),
            "{error:?}"
        );
    }

    #[test]
    fn from_file_carries_the_parsers_skipped_report() {
        let mut with_skips = parsed("ludeon.rimworld\n");
        with_skips.skipped = vec![SkippedEntry::NotAnEntry { line: 3 }];
        with_skips.omitted_skipped = 7;
        let store = InMemoryModListFileStore::new()
            .with_file("shared.txt", ModListRead::Parsed(with_skips));

        let ready = ready_plan(
            preview_of(store)
                .from_file(Path::new("shared.txt"), &target(&[], &[]))
                .expect("reads"),
        );

        assert_eq!(ready.skipped, [SkippedEntry::NotAnEntry { line: 3 }]);
        assert_eq!(ready.omitted_skipped, 7);
    }

    #[test]
    fn from_text_diffs_pasted_text() {
        let ready = ready_plan(ImportPreview::from_text(
            "example.extra\nludeon.rimworld\n",
            &target(&[], &["example.extra"]),
        ));

        assert_eq!(
            ready.plan.order,
            [ModId::new("example.extra"), ModId::new("ludeon.rimworld")]
        );
    }

    #[test]
    fn from_text_rejects_empty_input() {
        let preview = ImportPreview::from_text("  \n# only a comment\n", &target(&[], &[]));

        assert_eq!(preview, ImportPreview::Rejected(Rejection::NoEntries));
    }

    #[test]
    fn from_text_rejects_input_over_the_size_bound() {
        let oversized = "x".repeat(ModListLimits::MAX_INPUT_BYTES + 1);

        let preview = ImportPreview::from_text(&oversized, &target(&[], &[]));

        assert_eq!(
            preview,
            ImportPreview::Rejected(Rejection::TooLarge {
                limit_bytes: ModListLimits::MAX_INPUT_BYTES
            })
        );
    }

    #[test]
    fn from_session_reads_the_files_list_and_flags_pending_edits() {
        let mut session = session_fixture(&["ludeon.rimworld", "a"]);

        let clean = ImportTarget::from_session(&session);
        assert_eq!(
            clean.file_ids,
            [ModId::new("ludeon.rimworld"), ModId::new("a")]
        );
        assert!(!clean.context.has_pending_changes);
        assert_eq!(clean.context.own_merge_mod, session.own_merge_mod_id());
        assert_eq!(
            clean.context.own_merge_mod,
            rim_resolve::domain::GeneratedModIdentity::for_profile(session.paths().profile_hash())
                .package_id
        );
        assert_eq!(clean.context.game_version.as_deref(), Some("1.6"));

        let plan = crate::use_cases::DeactivateMods::plan(&session, &[ModId::new("a")]);
        crate::use_cases::DeactivateMods::execute(&mut session, &plan);
        let edited = ImportTarget::from_session(&session);
        assert!(
            edited.context.has_pending_changes,
            "an unrescanned deactivation is replaced by an import"
        );
        assert_eq!(
            edited.file_ids, clean.file_ids,
            "the file's list does not change with working edits"
        );
    }

    #[test]
    fn from_session_has_no_game_version_when_the_report_has_none() {
        let mut report = rim_resolve::test_support::ReportBuilder::new()
            .core("ludeon.rimworld")
            .build();
        report.metadata.game_version = String::new();
        let session = crate::test_support::session_with_sources_and_mods(
            rim_analyzer::analysis::SourceIndex::default(),
            report,
            &["ludeon.rimworld"],
        );

        assert_eq!(
            ImportTarget::from_session(&session).context.game_version,
            None
        );
    }

    #[test]
    fn a_text_round_trip_previews_as_an_empty_diff() {
        let list: SharedModList = parsed("ludeon.rimworld\nexample.base\n").list;
        let text = crate::mod_list::render_text(&list);

        let ready = ready_plan(ImportPreview::from_text(
            &text,
            &target(&["example.base"], &[]),
        ));

        assert!(ready.plan.deactivated.is_empty());
        assert_eq!(ready.plan.moved, 0);
    }
}
