//! The typed records of a parse, gathered from classified entries.
//!
//! The entry pipeline decides what an entry is; this module turns the
//! entries of the classes the port has a record for into that record: a
//! `PatchFailure` entry (its head and its `file:` line) into a terse
//! failure, a `PatchStackTrace` entry (its body and its trailer
//! trailer) into a stack-trace block, a `LoadEvent` entry (its ` - <id>`
//! items) into a load event, and the single-line classes (cross-reference,
//! texture fallback, mod metadata warning) from their head alone. Timers
//! and def-cache lines are not here: they are measurements taken on every
//! line, because a timer can be an indented continuation of another
//! message.
//!
//! **Startup passes.** Every record remembers the pass it was logged in
//! (the count of `RimWorld <version>` banners before it). Failures and
//! blocks pair within a pass ([`super::pairing`]), and a dependency warning
//! is kept from the last pass that logged it, because the game logs the same
//! warnings again in each pass (see [`last_logged_warnings`]).

use std::collections::BTreeMap;

use rim_session::ports::{
    AttributionInput, EntryClass, LogReadStats, RawCrossReference, RawDdsFailure,
    RawDependencyWarning, RawLoadEvent, RawPatchFailure, StackTraceBlock,
};

use super::extract::{
    OpenBlock, TerseFailure, parse_cross_reference, parse_dds_failure, parse_dependency_warning,
    parse_file_line, parse_load_header, parse_load_item, parse_terse_failure,
};
use super::pairing::{PassTagged, pair_failures};
use super::segment::Rule;
use super::shapes::CompiledShapes;

/// The record the open entry is still building.
enum OpenRecord {
    /// The open entry yields no multi-line record.
    None,
    /// A terse failure awaiting its `file:` line.
    Failure { pass: u32, failure: TerseFailure },
    /// A stack-trace block reading its body, then its trailer line.
    Block {
        pass: u32,
        block: OpenBlock,
        source_file: Option<String>,
    },
    /// A mod-list block collecting its items.
    Load(RawLoadEvent),
}

/// What the collector hands back when the input ends.
pub(super) struct ExtractedRecords {
    pub(super) patch_failures: Vec<RawPatchFailure>,
    pub(super) extra_stack_traces: Vec<StackTraceBlock>,
    pub(super) cross_references: Vec<RawCrossReference>,
    pub(super) dds_failures: Vec<RawDdsFailure>,
    pub(super) dependency_warnings: Vec<RawDependencyWarning>,
    pub(super) load_events: Vec<RawLoadEvent>,
}

/// Builds the typed records entry by entry.
pub(super) struct RecordCollector<'shapes> {
    shapes: &'shapes CompiledShapes,
    open: OpenRecord,
    failures: Vec<PassTagged<TerseFailure>>,
    blocks: Vec<PassTagged<StackTraceBlock>>,
    load_events: Vec<RawLoadEvent>,
    cross_references: Vec<RawCrossReference>,
    dds_failures: Vec<RawDdsFailure>,
    dependency_warnings: Vec<PassTagged<RawDependencyWarning>>,
}

impl<'shapes> RecordCollector<'shapes> {
    pub(super) fn new(shapes: &'shapes CompiledShapes) -> Self {
        Self {
            shapes,
            open: OpenRecord::None,
            failures: Vec::new(),
            blocks: Vec::new(),
            load_events: Vec::new(),
            cross_references: Vec::new(),
            dds_failures: Vec::new(),
            dependency_warnings: Vec::new(),
        }
    }

    /// A new entry of `class` opens, headed by `trimmed` at 1-based
    /// `head_line` in startup `pass`. The previous entry is already closed.
    pub(super) fn begin(&mut self, class: EntryClass, head_line: u64, trimmed: &str, pass: u32) {
        match class {
            EntryClass::PatchFailure => self.begin_failure(trimmed, pass),
            EntryClass::PatchStackTrace => self.begin_block(trimmed, pass),
            EntryClass::LoadEvent => self.begin_load_event(trimmed, head_line),
            EntryClass::CrossReference => {
                self.cross_references.extend(parse_cross_reference(trimmed))
            }
            EntryClass::TextureFallback => self
                .dds_failures
                .extend(parse_dds_failure(trimmed, self.shapes)),
            EntryClass::ModMetadataWarning => self.note_dependency_warning(trimmed, pass),
            _ => {}
        }
    }

    fn begin_failure(&mut self, trimmed: &str, pass: u32) {
        if let Some(failure) = parse_terse_failure(trimmed) {
            self.open = OpenRecord::Failure { pass, failure };
        }
    }

    fn begin_block(&mut self, trimmed: &str, pass: u32) {
        if let Some(mod_tag) = self.shapes.stack_start_mod(trimmed) {
            self.open = OpenRecord::Block {
                pass,
                block: OpenBlock::new(mod_tag),
                source_file: None,
            };
        }
    }

    fn begin_load_event(&mut self, trimmed: &str, head_line: u64) {
        if let Some(kind) = parse_load_header(trimmed) {
            self.open = OpenRecord::Load(RawLoadEvent {
                line: usize::try_from(head_line).unwrap_or(usize::MAX),
                kind,
                mods: Vec::new(),
            });
        }
    }

    fn note_dependency_warning(&mut self, trimmed: &str, pass: u32) {
        if let Some(value) = parse_dependency_warning(trimmed) {
            self.dependency_warnings.push(PassTagged { pass, value });
        }
    }

    /// The open entry takes a continuation line of kind `rule`.
    pub(super) fn absorb(&mut self, rule: Rule, trimmed: &str, stats: &mut LogReadStats) {
        match (&mut self.open, rule) {
            (OpenRecord::Failure { failure, .. }, Rule::PatchFailureFileLine) => {
                failure.source_file = parse_file_line(trimmed);
            }
            (OpenRecord::Block { block, .. }, Rule::StackBlockBody) => {
                // The end marker is the last body line and is not an
                // operation.
                if !self.shapes.is_stack_end(trimmed) {
                    block.absorb(trimmed, self.shapes, stats);
                }
            }
            (OpenRecord::Block { source_file, .. }, Rule::StackBlockTrailer) => {
                *source_file = self.shapes.stack_trailer_path(trimmed);
            }
            (OpenRecord::Load(event), Rule::LoadBlockItem) => {
                event.mods.extend(parse_load_item(trimmed));
            }
            _ => {}
        }
    }

    /// What the open entry says about its mod: the tag and, once its `file:`
    /// line or trailer has arrived, the file. `None` when the open entry
    /// builds no failure or block record.
    pub(super) fn open_attribution_input(&self) -> Option<AttributionInput> {
        let (name, path) = match &self.open {
            OpenRecord::Failure { failure, .. } => {
                (failure.mod_tag.as_str(), failure.source_file.as_deref())
            }
            OpenRecord::Block {
                block, source_file, ..
            } => (block.mod_tag(), source_file.as_deref()),
            OpenRecord::None | OpenRecord::Load(_) => return None,
        };
        Some(match path {
            Some(path) => AttributionInput::display_name_and_path(name, path),
            None => AttributionInput::display_name(name),
        })
    }

    /// The open entry ends: its record is complete.
    pub(super) fn close(&mut self) {
        match std::mem::replace(&mut self.open, OpenRecord::None) {
            OpenRecord::None => {}
            OpenRecord::Failure { pass, failure } => {
                self.failures.push(PassTagged {
                    pass,
                    value: failure,
                });
            }
            OpenRecord::Block {
                pass,
                block,
                source_file,
            } => self.blocks.push(PassTagged {
                pass,
                value: block.into_block(source_file),
            }),
            OpenRecord::Load(event) => self.load_events.push(event),
        }
    }

    /// Closes the last entry, pairs failures with blocks, and folds the
    /// dependency warnings of the passes into one set.
    pub(super) fn finish(mut self) -> ExtractedRecords {
        self.close();
        let (patch_failures, extra_stack_traces) = pair_failures(self.failures, self.blocks);
        ExtractedRecords {
            patch_failures,
            extra_stack_traces,
            cross_references: self.cross_references,
            dds_failures: self.dds_failures,
            dependency_warnings: last_logged_warnings(self.dependency_warnings),
            load_events: self.load_events,
        }
    }
}

/// Each warning as its last logging pass has it.
///
/// The game logs the same startup warnings again in every pass, so summing
/// the passes would double them, yet reading only the last pass would drop
/// what a cut-off last pass never reached. The rule: a warning's identity is
/// its (mod name, dependency id); for each identity only the occurrences of
/// the highest pass that logged it are kept (a repeat inside that pass stays,
/// as the game logged it), in file order, which is pass order and then
/// position. When every pass logs the same set, that is exactly the last
/// pass's list.
fn last_logged_warnings(
    records: Vec<PassTagged<RawDependencyWarning>>,
) -> Vec<RawDependencyWarning> {
    let mut last_pass_of: BTreeMap<(&str, &str), u32> = BTreeMap::new();
    for tagged in &records {
        let last = last_pass_of
            .entry((&tagged.value.mod_name, &tagged.value.dependency_id))
            .or_insert(tagged.pass);
        *last = (*last).max(tagged.pass);
    }
    let is_kept: Vec<bool> = records
        .iter()
        .map(|tagged| {
            last_pass_of.get(&(
                tagged.value.mod_name.as_str(),
                tagged.value.dependency_id.as_str(),
            )) == Some(&tagged.pass)
        })
        .collect();
    records
        .into_iter()
        .zip(is_kept)
        .filter_map(|(tagged, is_kept)| is_kept.then_some(tagged.value))
        .collect()
}
