//! The entry pipeline: segmenter -> open entry -> classifier -> aggregator,
//! plus the back-reference index and the sentinel check. Fed one line at a
//! time by the parser; holds one entry's worth of text, whatever the file
//! size.

use std::collections::BTreeMap;

use rim_session::ports::{
    ClassTally, DefCacheCarrier, EngineInfoKind, EntryClass, LogCoverage, LogReadStats, LoggingGap,
    SentinelReport,
};

use rim_session::ports::FamilyKey;

use super::aggregate::{Aggregator, ClosedEntry};
use super::attribution::{EvidenceSource, attribution_input};

use super::classify::{Classification, ClassifierSet, family_key};
use super::coverage::CoverageCollector;
use super::entry::OpenEntry;
use super::records::{ExtractedRecords, RecordCollector};
use super::segment::{Framing, LineView, Segment, Segmenter, context_for_head};
use super::sentinels::{PlacedLine, SentinelChecker, SentinelSet};
use super::shapes::CompiledShapes;

/// The most back-reference originals the index holds.
const MAX_INDEXED_REFS: usize = 10_000;

/// Back-reference id -> the frame type its original shows.
///
/// **Memory bound.** At most [`MAX_INDEXED_REFS`] entries, each an id of at
/// most 16 bytes (the id patterns admit no more) and a frame type of at most
/// [`FamilyKey::MAX_BYTES`] bytes: 10,000 x 272 bytes of text, about 2.7 MB
/// plus map overhead, whatever the input. One entry's own bookkeeping adds
/// at most 64 originals and 8 stub ids of the same sizes.
#[derive(Default)]
struct BackRefIndex {
    frames: BTreeMap<String, String>,
}

impl BackRefIndex {
    fn frame_of(&self, id: &str) -> Option<&str> {
        self.frames.get(id).map(String::as_str)
    }

    /// Registers an original; the first registration of an id wins. A full
    /// index counts the drop instead.
    fn register(&mut self, id: String, frame: String, stats: &mut LogReadStats) {
        if self.frames.contains_key(&id) {
            return;
        }
        if self.frames.len() >= MAX_INDEXED_REFS {
            stats.stack_refs_dropped += 1;
            return;
        }
        self.frames.insert(id, frame);
    }
}

/// The last family key computed, and what it was computed from. Storms
/// repeat one message thousands of times in a row, so the next entry usually
/// needs the same key.
#[derive(Default)]
struct KeyMemo {
    class: Option<EntryClass>,
    head: String,
    frame: Option<String>,
    key: FamilyKey,
}

impl KeyMemo {
    fn key_for(&mut self, class: EntryClass, head: &str, frame: Option<&str>) -> &FamilyKey {
        let is_same =
            self.class == Some(class) && self.head == head && self.frame.as_deref() == frame;
        if !is_same {
            self.key = family_key(class, head, frame);
            self.class = Some(class);
            self.head.clear();
            self.head.push_str(head);
            self.frame = frame.map(str::to_string);
        }
        &self.key
    }
}

/// What the pipeline hands back when the input ends.
pub(super) struct PipelineOutput {
    pub(super) classes: BTreeMap<EntryClass, ClassTally>,
    pub(super) blank_separator_lines: u64,
    pub(super) sentinels: SentinelReport,
    pub(super) records: ExtractedRecords,
    pub(super) coverage: LogCoverage,
    pub(super) logging_gaps: Vec<LoggingGap>,
}

pub(super) struct EntryPipeline<'formats> {
    carriers: &'formats [DefCacheCarrier],
    shapes: &'formats CompiledShapes,
    classifier: &'static ClassifierSet,
    segmenter: Segmenter<'formats>,
    entry: OpenEntry,
    is_entry_open: bool,
    pass: u32,
    refs: BackRefIndex,
    key_memo: KeyMemo,
    aggregator: Aggregator,
    records: RecordCollector<'formats>,
    sentinels: SentinelChecker,
    blank_separator_lines: u64,
    line_number: u64,
    framing: Framing,
    coverage: CoverageCollector,
}

impl<'formats> EntryPipeline<'formats> {
    pub(super) fn new(
        carriers: &'formats [DefCacheCarrier],
        shapes: &'formats CompiledShapes,
        framing: Framing,
    ) -> Self {
        Self {
            carriers,
            shapes,
            classifier: ClassifierSet::builtin(),
            segmenter: Segmenter::new(framing, shapes),
            entry: OpenEntry::new(),
            is_entry_open: false,
            pass: 0,
            refs: BackRefIndex::default(),
            key_memo: KeyMemo::default(),
            aggregator: Aggregator::default(),
            records: RecordCollector::new(shapes),
            sentinels: SentinelChecker::new(SentinelSet::builtin(), shapes.sentinels()),
            blank_separator_lines: 0,
            line_number: 0,
            framing,
            coverage: CoverageCollector::new(),
        }
    }

    /// The startup pass the most recently fed line belongs to: the number
    /// of `RimWorld <version>` banners fed so far (`0` before the first).
    pub(super) fn pass(&self) -> u32 {
        self.pass
    }

    /// Feeds one line (without its line ending).
    pub(super) fn feed(&mut self, text: &str, stats: &mut LogReadStats) {
        self.line_number += 1;
        let line = LineView::new(text);
        let rule = match self.segmenter.push(&line) {
            Segment::Separator => {
                self.close_entry(stats);
                self.blank_separator_lines += 1;
                return;
            }
            Segment::Continuation(rule) => {
                self.entry.absorb(&line, rule);
                self.records.absorb(rule, line.trimmed, stats);
                self.coverage.absorb(line.trimmed);
                if rule.is_stack_frame() {
                    return;
                }
                Some(rule)
            }
            Segment::Head => {
                self.close_entry(stats);
                self.open_entry(&line);
                None
            }
        };
        let class = self.entry.class();
        let (classifier, carriers, shapes) = (self.classifier, self.carriers, self.shapes);
        let placed = PlacedLine {
            number: self.line_number,
            text,
            found: class,
            rule,
        };
        self.sentinels.check(&placed, || {
            classifier.classify(text, carriers, shapes).class == class
        });
    }

    fn open_entry(&mut self, line: &LineView<'_>) {
        // The previous entry's head is still in `entry` until `begin` below.
        let classification = if self.entry.head() == line.text {
            Classification {
                class: self.entry.class(),
                severity: self.entry.severity(),
            }
        } else {
            self.classifier
                .classify(line.text, self.carriers, self.shapes)
        };
        if classification.class == EntryClass::EngineInfo(EngineInfoKind::Banner) {
            self.pass = self.pass.saturating_add(1);
        }
        self.entry
            .begin(self.line_number, line.text, classification, self.pass);
        self.records.begin(
            classification.class,
            self.line_number,
            line.trimmed,
            self.pass,
        );
        self.coverage
            .begin(classification.class, self.line_number, line.trimmed);
        self.segmenter
            .enter_context(context_for_head(classification.class, line.trimmed));
        self.is_entry_open = true;
    }

    fn close_entry(&mut self, stats: &mut LogReadStats) {
        if !self.is_entry_open {
            return;
        }
        self.is_entry_open = false;
        stats.stack_refs_dropped += self.entry.originals_dropped();
        for (id, frame) in self.entry.take_resolved_originals() {
            self.refs.register(id, frame, stats);
        }
        let frame = key_frame(&self.entry, &self.refs);
        let key = self
            .key_memo
            .key_for(self.entry.class(), self.entry.head(), frame);
        // Read while the entry's failure/block record is still open.
        let (entry, records, shapes) = (&self.entry, &self.records, self.shapes);
        let first_evidence = || {
            attribution_input(&EvidenceSource {
                class: entry.class(),
                head: entry.head(),
                records,
                shapes,
                key_frame: frame,
            })
        };
        self.aggregator.add(
            &ClosedEntry {
                class: entry.class(),
                key,
                severity: entry.severity(),
                head_line: entry.head_line(),
                lines: entry.lines(),
                pass: entry.pass(),
                sample: entry.sample(),
                is_sample_truncated: entry.is_sample_truncated(),
                attribution_input: &first_evidence,
            },
            stats,
        );
        self.records.close();
    }

    /// Closes the last entry and returns what the parse found.
    pub(super) fn finish(mut self, stats: &mut LogReadStats) -> PipelineOutput {
        self.close_entry(stats);
        let classes = self.aggregator.into_classes();
        let collected = self.coverage.finish(self.framing, &classes);
        stats.logging_gaps_dropped += collected.logging_gaps_dropped;
        stats.crash_report_paths_truncated += collected.crash_report_paths_truncated;
        PipelineOutput {
            classes,
            blank_separator_lines: self.blank_separator_lines,
            sentinels: self.sentinels.finish(),
            records: self.records.finish(),
            coverage: collected.coverage,
            logging_gaps: collected.logging_gaps,
        }
    }
}

/// The first non-engine frame of an entry's stack: the first back-reference
/// stub whose original is known, else the first frame printed directly.
fn key_frame<'a>(entry: &'a OpenEntry, refs: &'a BackRefIndex) -> Option<&'a str> {
    entry
        .pending_stub_ids()
        .iter()
        .find_map(|id| refs.frame_of(id))
        .or_else(|| entry.direct_frame())
}
