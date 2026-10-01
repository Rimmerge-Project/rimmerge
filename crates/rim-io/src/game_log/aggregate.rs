//! Aggregation: closed entries become families (a count, a line span, the
//! passes seen, one retained sample) inside their class. Every bound
//! truncates **and counts**: an entry whose family does not fit lands in the
//! class's overflow tally, never nowhere.

use std::collections::BTreeMap;

use rim_session::ports::{
    AttributionInput, ClassTally, EntryClass, EntrySample, Family, FamilyKey, LogReadStats,
    Severity,
};

/// The most distinct families one class keeps.
pub(super) const MAX_FAMILIES_PER_CLASS: usize = 5_000;
/// The most distinct families all classes keep together.
pub(super) const MAX_FAMILIES_TOTAL: usize = 50_000;
/// The most bytes of samples retained across all families.
pub(super) const MAX_SAMPLE_BUDGET_BYTES: usize = 16 * 1024 * 1024;
/// The last startup pass a family tracks separately; later passes fold into
/// it.
pub(super) const MAX_TRACKED_PASSES: u32 = 16;

/// One closed entry, ready to be recorded.
pub(super) struct ClosedEntry<'a> {
    pub(super) class: EntryClass,
    pub(super) key: &'a FamilyKey,
    pub(super) severity: Option<Severity>,
    pub(super) head_line: u64,
    pub(super) lines: u64,
    pub(super) pass: u32,
    pub(super) sample: &'a str,
    pub(super) is_sample_truncated: bool,
    /// The entry's attribution evidence, read only when the entry opens a
    /// new family: attribution is per family, so a storm of one message
    /// builds it once.
    pub(super) attribution_input: &'a dyn Fn() -> Option<AttributionInput>,
}

/// The class tallies of one parse, under the bounds.
#[derive(Default)]
pub(super) struct Aggregator {
    classes: BTreeMap<EntryClass, ClassTally>,
    family_count: usize,
    sample_bytes: usize,
}

impl Aggregator {
    /// Records one closed entry; `stats` receives the bounds it hit.
    pub(super) fn add(&mut self, entry: &ClosedEntry<'_>, stats: &mut LogReadStats) {
        let tracked_pass = entry.pass.min(MAX_TRACKED_PASSES);
        stats.passes_folded += u64::from(entry.pass > MAX_TRACKED_PASSES);
        let tally = self.classes.entry(entry.class).or_default();
        if let Some(family) = tally.families.get_mut(entry.key) {
            family.count += 1;
            family.lines += entry.lines;
            family.last_line = entry.head_line;
            *family.count_by_pass.entry(tracked_pass).or_default() += 1;
            return;
        }
        if tally.families.len() >= MAX_FAMILIES_PER_CLASS || self.family_count >= MAX_FAMILIES_TOTAL
        {
            tally.overflow.entries += 1;
            tally.overflow.lines += entry.lines;
            return;
        }
        let sample = retain_sample(&mut self.sample_bytes, entry);
        tally.families.insert(
            entry.key.clone(),
            Family {
                count: 1,
                lines: entry.lines,
                first_line: entry.head_line,
                last_line: entry.head_line,
                count_by_pass: BTreeMap::from([(tracked_pass, 1)]),
                severity: entry.severity,
                sample,
                attribution_input: (entry.attribution_input)(),
            },
        );
        self.family_count += 1;
    }

    /// The finished class tallies.
    pub(super) fn into_classes(self) -> BTreeMap<EntryClass, ClassTally> {
        self.classes
    }
}

/// The sample of a new family, unless the budget is spent.
fn retain_sample(sample_bytes: &mut usize, entry: &ClosedEntry<'_>) -> Option<EntrySample> {
    if *sample_bytes + entry.sample.len() > MAX_SAMPLE_BUDGET_BYTES {
        return None;
    }
    *sample_bytes += entry.sample.len();
    Some(EntrySample {
        text: entry.sample.to_string(),
        is_truncated: entry.is_sample_truncated,
    })
}
