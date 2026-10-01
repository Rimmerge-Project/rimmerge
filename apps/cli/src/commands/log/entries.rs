//! The entry view of `log import`: the P1 totals, the per-class families and
//! the P2 sentinel report, as JSON and as text lines.

use std::collections::{BTreeMap, BTreeSet};

use rim_analyzer::domain::{Mod, ModId};
use rim_session::ports::{
    EngineInfoKind, EntryClass, Family, FamilyKey, SaveLoadPhase, SentinelReport, Severity,
};
use rim_session::use_cases::{AttributedClass, FamilyAttribution, GameLogSummary};
use serde::Serialize;

use super::mod_name;
use crate::common::TerminalSafe;

/// The most `Unclassified` families the text output lists.
const TEXT_UNCLASSIFIED_FAMILIES: usize = 10;
/// The most families the text output lists under one attributed class.
const TEXT_TOP_FAMILIES_PER_CLASS: usize = 3;
/// The most candidate mods the text output names for an ambiguous family.
const TEXT_AMBIGUOUS_CANDIDATES: usize = 3;
/// The most characters of a family key the text output prints.
const TEXT_KEY_CHARS: usize = 100;

/// The class's stable output name.
pub(super) fn format_class(class: EntryClass) -> &'static str {
    match class {
        EntryClass::LoadEvent => "load_event",
        EntryClass::EngineInfo(EngineInfoKind::Banner) => "engine_info_banner",
        EntryClass::EngineInfo(EngineInfoKind::LoggingStopped) => "engine_info_logging_stopped",
        EntryClass::EngineInfo(EngineInfoKind::LoggingResumed) => "engine_info_logging_resumed",
        EntryClass::EngineInfo(EngineInfoKind::SessionMarker) => "engine_info_session_marker",
        EntryClass::EngineInfo(EngineInfoKind::UnityRuntime) => "engine_info_unity_runtime",
        EntryClass::PatchFailure => "patch_failure",
        EntryClass::ConfigError => "config_error",
        EntryClass::PatchError => "patch_error",
        EntryClass::PatchStackTrace => "patch_stack_trace",
        EntryClass::CrossReference => "cross_reference",
        EntryClass::MissingParent => "missing_parent",
        EntryClass::XmlError => "xml_error",
        EntryClass::DuplicateDef => "duplicate_def",
        EntryClass::TextureFallback => "texture_fallback",
        EntryClass::TextureLoadFailure => "texture_load_failure",
        EntryClass::TypeLoadError => "type_load_error",
        EntryClass::SaveLoadReference(SaveLoadPhase::Save) => "save_load_reference_save",
        EntryClass::SaveLoadReference(SaveLoadPhase::Load) => "save_load_reference_load",
        EntryClass::ModMetadataWarning => "mod_metadata_warning",
        EntryClass::BaseGenRuleMissing => "base_gen_rule_missing",
        EntryClass::RuntimeException => "runtime_exception",
        EntryClass::UnityRuntimeError => "unity_runtime_error",
        EntryClass::DefCacheLine => "def_cache_line",
        EntryClass::Timer => "timer",
        EntryClass::ModMessage => "mod_message",
        EntryClass::Unclassified => "unclassified",
    }
}

fn format_severity(severity: Severity) -> &'static str {
    match severity {
        Severity::Message => "message",
        Severity::Warning => "warning",
        Severity::Error => "error",
    }
}

#[derive(Debug, Serialize)]
pub(super) struct TotalsJson {
    lines_read: u64,
    entry_lines: u64,
    blank_separator_lines: u64,
    entries: u64,
    /// Startup passes: stated for a `Player.log` only, absent for a console
    /// snapshot (which is never a whole session).
    #[serde(skip_serializing_if = "Option::is_none")]
    passes: Option<u64>,
    conserved: bool,
}

impl From<&GameLogSummary> for TotalsJson {
    fn from(summary: &GameLogSummary) -> Self {
        let totals = summary.totals();
        Self {
            lines_read: totals.lines_read,
            entry_lines: totals.entry_lines,
            blank_separator_lines: totals.blank_separator_lines,
            entries: totals.entries,
            passes: summary.startup_passes(),
            conserved: totals.is_conserved(),
        }
    }
}

/// A mod named in a family's attribution.
#[derive(Debug, Serialize)]
struct ModRefJson {
    mod_id: String,
    mod_name: String,
}

/// A family's attribution, one shape per outcome (no flag plus nullable
/// fields): `kind` is `mod`, `unattributed` or `ambiguous`.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum FamilyAttributionJson {
    Mod {
        mod_id: String,
        mod_name: String,
    },
    Unattributed {
        raw: String,
    },
    Ambiguous {
        raw: String,
        /// Every active mod the log's name fits, by id.
        candidates: Vec<ModRefJson>,
    },
}

fn mod_ref_json(mods_by_id: &BTreeMap<ModId, &Mod>, id: &ModId) -> ModRefJson {
    ModRefJson {
        mod_id: id.to_string(),
        mod_name: mod_name(mods_by_id, id),
    }
}

fn family_attribution_json(
    mods_by_id: &BTreeMap<ModId, &Mod>,
    attribution: &FamilyAttribution,
) -> FamilyAttributionJson {
    match attribution {
        FamilyAttribution::Mod(id) => FamilyAttributionJson::Mod {
            mod_id: id.to_string(),
            mod_name: mod_name(mods_by_id, id),
        },
        FamilyAttribution::Unattributed { raw } => {
            FamilyAttributionJson::Unattributed { raw: raw.clone() }
        }
        FamilyAttribution::Ambiguous { raw, candidates } => FamilyAttributionJson::Ambiguous {
            raw: raw.clone(),
            candidates: candidates
                .iter()
                .map(|id| mod_ref_json(mods_by_id, id))
                .collect(),
        },
    }
}

/// The text form of a family's attribution: the mod's name, `(unattributed:
/// <raw>)`, or `(ambiguous: <raw>; N mods: <first names>)`.
fn format_family_attribution(
    mods_by_id: &BTreeMap<ModId, &Mod>,
    attribution: &FamilyAttribution,
) -> String {
    match attribution {
        FamilyAttribution::Mod(id) => TerminalSafe::line(mod_name(mods_by_id, id)).to_string(),
        FamilyAttribution::Unattributed { raw } => {
            format!("(unattributed: {})", TerminalSafe::line(raw))
        }
        FamilyAttribution::Ambiguous { raw, candidates } => {
            format!(
                "(ambiguous: {}; {} mods: {})",
                TerminalSafe::line(raw),
                candidates.len(),
                candidate_names(mods_by_id, candidates)
            )
        }
    }
}

/// The first [`TEXT_AMBIGUOUS_CANDIDATES`] candidates' names, then how many
/// more there are.
fn candidate_names(mods_by_id: &BTreeMap<ModId, &Mod>, candidates: &BTreeSet<ModId>) -> String {
    let names: Vec<String> = candidates
        .iter()
        .take(TEXT_AMBIGUOUS_CANDIDATES)
        .map(|id| TerminalSafe::line(mod_name(mods_by_id, id)).to_string())
        .collect();
    match candidates.len().checked_sub(TEXT_AMBIGUOUS_CANDIDATES) {
        Some(more) if more > 0 => format!("{}, and {more} more", names.join(", ")),
        _ => names.join(", "),
    }
}

#[derive(Debug, Serialize)]
struct FamilyJson {
    key: String,
    count: u64,
    lines: u64,
    first_line: u64,
    last_line: u64,
    count_by_pass: BTreeMap<u32, u64>,
    severity: Option<&'static str>,
    /// What the family is attributed to; `null` when its first entry carried
    /// no evidence (a cross-reference, an engine message).
    attribution: Option<FamilyAttributionJson>,
    sample: Option<String>,
    sample_truncated: bool,
}

fn family_json(
    key: &FamilyKey,
    family: &Family,
    class: &AttributedClass,
    mods_by_id: &BTreeMap<ModId, &Mod>,
) -> FamilyJson {
    FamilyJson {
        key: key.as_str().to_string(),
        count: family.count,
        lines: family.lines,
        first_line: family.first_line,
        last_line: family.last_line,
        count_by_pass: family.count_by_pass.clone(),
        severity: family.severity.map(format_severity),
        attribution: class
            .attribution
            .get(key)
            .map(|attribution| family_attribution_json(mods_by_id, attribution)),
        sample: family.sample.as_ref().map(|sample| sample.text.clone()),
        sample_truncated: family
            .sample
            .as_ref()
            .is_some_and(|sample| sample.is_truncated),
    }
}

#[derive(Debug, Serialize)]
struct OverflowJson {
    entries: u64,
    lines: u64,
}

#[derive(Debug, Serialize)]
pub(super) struct ClassJson {
    class: &'static str,
    entries: u64,
    lines: u64,
    overflow: Option<OverflowJson>,
    families: Vec<FamilyJson>,
}

/// A class's families, most frequent first, then by key.
fn sorted_families(class: &AttributedClass) -> Vec<(&FamilyKey, &Family)> {
    let mut families: Vec<(&FamilyKey, &Family)> = class.tally.families.iter().collect();
    families.sort_by(|(left_key, left), (right_key, right)| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left_key.cmp(right_key))
    });
    families
}

fn class_json(
    class: EntryClass,
    attributed: &AttributedClass,
    mods_by_id: &BTreeMap<ModId, &Mod>,
) -> ClassJson {
    let tally = &attributed.tally;
    let has_overflow = tally.overflow.entries != 0;
    ClassJson {
        class: format_class(class),
        entries: tally.entries(),
        lines: tally.lines(),
        overflow: has_overflow.then_some(OverflowJson {
            entries: tally.overflow.entries,
            lines: tally.overflow.lines,
        }),
        families: sorted_families(attributed)
            .into_iter()
            .map(|(key, family)| family_json(key, family, attributed, mods_by_id))
            .collect(),
    }
}

pub(super) fn classes_json(
    summary: &GameLogSummary,
    mods_by_id: &BTreeMap<ModId, &Mod>,
) -> Vec<ClassJson> {
    summary
        .classes
        .iter()
        .map(|(class, attributed)| class_json(*class, attributed, mods_by_id))
        .collect()
}

#[derive(Debug, Serialize)]
struct LeakJson {
    sentinel: String,
    expected: Vec<&'static str>,
    found: &'static str,
    line: u64,
    text: String,
}

#[derive(Debug, Serialize)]
pub(super) struct SentinelsJson {
    total: u64,
    leaks: Vec<LeakJson>,
}

impl From<&SentinelReport> for SentinelsJson {
    fn from(report: &SentinelReport) -> Self {
        Self {
            total: report.total,
            leaks: report
                .leaks
                .iter()
                .map(|leak| LeakJson {
                    sentinel: leak.sentinel.clone(),
                    expected: leak.expected.iter().copied().map(format_class).collect(),
                    found: format_class(leak.found),
                    line: leak.line,
                    text: leak.text.clone(),
                })
                .collect(),
        }
    }
}

/// The text lines that lead with a problem: a broken reconciliation or a
/// sentinel leak, each printed only when it happens.
pub(super) fn print_entry_warnings(summary: &GameLogSummary) {
    if !summary.totals().is_conserved() {
        println!("  WARNING: line accounting does not reconcile (a parser bug, please report)");
    }
    if summary.sentinels.total != 0 {
        println!(
            "  WARNING: {} sentinel hit(s): a known class's loose pattern matched a line that \
             sits in another class (a possible classifier leak){}",
            summary.sentinels.total,
            listed_note(&summary.sentinels)
        );
        for leak in &summary.sentinels.leaks {
            println!(
                "    line {}: {} in {}: {}",
                leak.line,
                TerminalSafe::line(&leak.sentinel),
                format_class(leak.found),
                TerminalSafe::line(&leak.text)
            );
        }
    }
}

/// `; only the first N are listed` when the report holds fewer leaks than
/// hits.
fn listed_note(report: &SentinelReport) -> String {
    let listed = report.leaks.len() as u64;
    if listed >= report.total {
        return String::new();
    }
    format!("; only the first {listed} are listed")
}

/// The `totals:` line.
pub(super) fn print_totals(summary: &GameLogSummary) {
    let totals = summary.totals();
    let family_count: usize = summary
        .classes
        .values()
        .map(|class| class.tally.families.len())
        .sum();
    let passes = summary
        .startup_passes()
        .map_or_else(String::new, |passes| format!("; {passes} startup passes"));
    println!(
        "  totals: {} lines = {} entry lines + {} blank separators; {} entries in {} families{passes}",
        totals.lines_read,
        totals.entry_lines,
        totals.blank_separator_lines,
        totals.entries,
        family_count,
    );
}

/// The `classes:` section: one line per class (with its top families and the
/// mods they are attributed to, for a class that has attributions), then the
/// top `Unclassified` families.
pub(super) fn print_classes(summary: &GameLogSummary, mods_by_id: &BTreeMap<ModId, &Mod>) {
    if summary.classes.is_empty() {
        return;
    }
    println!();
    println!("classes:");
    for (class, attributed) in &summary.classes {
        let tally = &attributed.tally;
        let overflow = if tally.overflow.entries == 0 {
            String::new()
        } else {
            format!(
                " (+{} entries past the family bound)",
                tally.overflow.entries
            )
        };
        println!(
            "  {}: {} entries in {} families{overflow}",
            format_class(*class),
            tally.entries(),
            tally.families.len()
        );
        print_top_attributed_families(attributed, mods_by_id);
    }
    print_top_unclassified(summary);
}

/// A key cut to [`TEXT_KEY_CHARS`] characters, marked when it was cut.
fn shortened(key: &str) -> String {
    match key.char_indices().nth(TEXT_KEY_CHARS) {
        Some((cut, _)) => format!("{}...", &key[..cut]),
        None => key.to_string(),
    }
}

/// The most frequent families of a class that has attributions, each with
/// the mod it is attributed to. A class with no attribution prints nothing
/// (a wall of families is the `--json` output's job).
fn print_top_attributed_families(attributed: &AttributedClass, mods_by_id: &BTreeMap<ModId, &Mod>) {
    if attributed.attribution.is_empty() {
        return;
    }
    for (key, family) in sorted_families(attributed)
        .into_iter()
        .take(TEXT_TOP_FAMILIES_PER_CLASS)
    {
        let mod_text = attributed.attribution.get(key).map_or_else(
            || "(no attribution)".to_string(),
            |attribution| format_family_attribution(mods_by_id, attribution),
        );
        println!(
            "    {} x {} -> {mod_text}",
            family.count,
            TerminalSafe::line(shortened(key.as_str()))
        );
    }
}

fn print_top_unclassified(summary: &GameLogSummary) {
    let Some(attributed) = summary.classes.get(&EntryClass::Unclassified) else {
        return;
    };
    println!();
    println!("top unclassified families:");
    for (key, family) in sorted_families(attributed)
        .into_iter()
        .take(TEXT_UNCLASSIFIED_FAMILIES)
    {
        println!(
            "  {} x {} (first at line {})",
            family.count,
            TerminalSafe::line(key.as_str()),
            family.first_line
        );
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use rim_session::ports::{ClassTally, Family, FamilyKey};
    use rim_session::use_cases::AttributedClass;

    use super::{shortened, sorted_families};

    fn family(count: u64) -> Family {
        Family {
            count,
            lines: count,
            first_line: 1,
            last_line: 1,
            count_by_pass: BTreeMap::new(),
            severity: None,
            sample: None,
            attribution_input: None,
        }
    }

    #[test]
    fn families_sort_by_count_descending_then_key_ascending() {
        let tally = ClassTally {
            families: BTreeMap::from([
                (FamilyKey::new("b".to_string()), family(2)),
                (FamilyKey::new("a".to_string()), family(2)),
                (FamilyKey::new("z".to_string()), family(9)),
                (FamilyKey::new("c".to_string()), family(1)),
            ]),
            overflow: rim_session::ports::Tally::default(),
        };
        let class = AttributedClass {
            tally,
            attribution: BTreeMap::new(),
        };

        let keys: Vec<&str> = sorted_families(&class)
            .into_iter()
            .map(|(key, _family)| key.as_str())
            .collect();

        assert_eq!(keys, ["z", "a", "b", "c"]);
    }

    #[test]
    fn a_long_key_is_cut_on_a_character_boundary_and_a_short_one_is_kept() {
        let long = "é".repeat(150);

        assert_eq!(shortened("short"), "short");
        assert_eq!(shortened(&long), format!("{}...", "é".repeat(100)));
    }
}
