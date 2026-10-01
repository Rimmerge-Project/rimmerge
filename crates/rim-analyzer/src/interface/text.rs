//! Renders a [`Report`] as the text summary printed to stdout.

use std::fmt;
use std::io::Write;
use std::time::Duration;

use crate::domain::{
    Conflict, Constraint, ConstraintStatus, DefOverride, Edge, EdgeKind, EdgeStatus, EdgeStrength,
    InactiveMod, Mod, ModsById, PatchCollision, PatchCollisionSeverity, Report,
};

/// Top-N cap for text-summary lists when `--verbose` isn't given.
const SUMMARY_TOP_N: usize = 20;

/// A `Write` wrapper that swallows write errors after the first one
/// (typically a closed/broken stdout pipe, e.g. piping into `head`)
/// instead of panicking — every subsequent `line` call becomes a no-op.
struct Printer<W: Write> {
    out: W,
    broken: bool,
}

impl<W: Write> Printer<W> {
    fn new(out: W) -> Self {
        Self { out, broken: false }
    }

    fn line(&mut self, args: fmt::Arguments) {
        if self.broken {
            return;
        }
        if writeln!(self.out, "{args}").is_err() {
            self.broken = true;
        }
    }
}

/// Prints the full text summary to `out`. Never panics, even if `out` is
/// a closed pipe.
pub fn print_summary(out: impl Write, report: &Report, verbose: bool, elapsed: Duration) {
    let mut printer = Printer::new(out);
    print_counts_table(&mut printer, report, elapsed);
    print_violated_edges(
        &mut printer,
        report,
        "Violated forceLoad edges",
        verbose,
        |e| matches!(e.kind, EdgeKind::ForceLoadAfter | EdgeKind::ForceLoadBefore),
    );
    print_violated_edges(
        &mut printer,
        report,
        "Violated Hard edges (load-time assembly references, patch-injected nodes)",
        verbose,
        is_other_hard_edge,
    );
    print_violated_edges(
        &mut printer,
        report,
        "Violated lazy assembly references",
        verbose,
        |e| e.strength() == EdgeStrength::Soft,
    );
    print_violated_edges(
        &mut printer,
        report,
        "Violated declared edges",
        verbose,
        |e| e.strength() == EdgeStrength::Declared,
    );
    print_violated_edges(
        &mut printer,
        report,
        "Violated inferred edges (heuristic: PatchRemovedNode/RetextureAfterOwner/DefOverrideAfterOrigin/PatchInvalidatesPredicate/PatchRemovedNodeCosmetic)",
        verbose,
        is_inferred_edge,
    );
    print_capped(
        &mut printer,
        "Undeclared hard dependencies (AssemblyRef with no declared counterpart)",
        &report.undeclared_hard_dependencies,
        verbose,
        describe_edge,
    );
    print_constraints(&mut printer, report, verbose);
    print_def_overrides(&mut printer, report, verbose);
    print_patch_collisions(&mut printer, report, verbose);
    print_generic_conflicts(&mut printer, report, verbose);
    print_likely_duplicate_mods(&mut printer, report, verbose);
    print_framework_candidates(&mut printer, report, verbose);
    print_inactive_mods(&mut printer, report, verbose);
    printer.line(format_args!(
        "\nWarnings: {} (see --json output for details)",
        report.warnings.len()
    ));
}

/// Prints `title: <count>` then up to [`SUMMARY_TOP_N`] items (or all, if
/// `verbose`), rendered by `render`, followed by a "... and N more" line
/// when the list was truncated. The one list-printing helper every capped
/// section in the summary shares.
fn print_capped<T, W: Write>(
    printer: &mut Printer<W>,
    title: &str,
    items: &[T],
    verbose: bool,
    render: impl Fn(&T) -> String,
) {
    printer.line(format_args!("\n{title}: {}", items.len()));
    let shown = if verbose {
        items.len()
    } else {
        SUMMARY_TOP_N.min(items.len())
    };
    for item in &items[..shown] {
        printer.line(format_args!("  {}", render(item)));
    }
    if !verbose && items.len() > shown {
        printer.line(format_args!(
            "  ... and {} more (use --verbose or --json to see all)",
            items.len() - shown
        ));
    }
}

fn print_counts_table<W: Write>(printer: &mut Printer<W>, report: &Report, elapsed: Duration) {
    let m = &report.metadata;
    printer.line(format_args!(
        "rim-analyzer — {} ({})",
        m.game_dir.display(),
        m.game_version
    ));
    printer.line(format_args!(
        "  discovered mods:      {}",
        m.discovered_mod_count
    ));
    printer.line(format_args!(
        "  active mods:          {}",
        m.active_mod_count
    ));
    printer.line(format_args!(
        "  scanned mods:         {}",
        m.scanned_mod_count
    ));
    printer.line(format_args!(
        "  inactive mods:        {}",
        report.inactive_mods.len()
    ));
    printer.line(format_args!(
        "  missing mods:         {}",
        report.missing_mods.len()
    ));
    printer.line(format_args!("  mods with Defs:       {}", m.mods_with_defs));
    printer.line(format_args!(
        "  mods with Patches:    {}",
        m.mods_with_patches
    ));
    printer.line(format_args!(
        "  mods with Assemblies: {}",
        m.mods_with_assemblies
    ));
    printer.line(format_args!(
        "  total defs indexed:   {}",
        m.total_defs_indexed
    ));
    printer.line(format_args!(
        "  distinct textures:    {}",
        m.distinct_texture_paths
    ));
    printer.line(format_args!(
        "  edges built:          {}",
        report.edges.len()
    ));
    printer.line(format_args!(
        "  constraints built:    {}",
        report.constraints.len()
    ));
    printer.line(format_args!(
        "  conflicts found:      {}",
        report.conflicts.len()
    ));
    printer.line(format_args!("  elapsed:              {elapsed:.2?}"));
}

/// Every `Hard`-strength edge kind not already given its own dedicated
/// section: `ForceLoadAfter`/`ForceLoadBefore` are excluded here (their own
/// "Violated forceLoad edges" section, above, would otherwise list them
/// twice); `AssemblyRef` only counts when its own `load_time` flag makes it
/// `Hard` (a lazy reference is `Soft`, its own section below).
/// `PatchInjectedNode` is always `Hard` and has no section of its own, so
/// without this a violated one would be invisible in the text summary.
fn is_other_hard_edge(edge: &Edge) -> bool {
    edge.strength() == EdgeStrength::Hard
        && !matches!(
            edge.kind,
            EdgeKind::ForceLoadAfter | EdgeKind::ForceLoadBefore
        )
}

/// A violated `Inferred`-strength edge
/// (`PatchRemovedNode`/`RetextureAfterOwner`/`DefOverrideAfterOrigin`/
/// `PatchInvalidatesPredicate`/`PatchRemovedNodeCosmetic`) gets
/// its own section, the same reason [`is_other_hard_edge`] exists for
/// `PatchInjectedNode` — otherwise a violated one would be invisible in the
/// text summary a `cargo run -p rim-analyzer -- analyze` records real-install
/// counts from.
fn is_inferred_edge(edge: &Edge) -> bool {
    edge.strength() == EdgeStrength::Inferred
}

fn print_violated_edges<W: Write>(
    printer: &mut Printer<W>,
    report: &Report,
    title: &str,
    verbose: bool,
    matches_section: impl Fn(&Edge) -> bool,
) {
    let violated: Vec<&Edge> = report
        .edges
        .iter()
        .filter(|r| r.status == EdgeStatus::Violated && matches_section(&r.edge))
        .map(|r| &r.edge)
        .collect();
    print_capped(printer, title, &violated, verbose, |edge| {
        describe_edge(edge)
    });
}

fn describe_edge(edge: &Edge) -> String {
    format!(
        "{} -> {} [{:?}] {}",
        edge.after, edge.before, edge.kind, edge.detail
    )
}

/// "Any-of" assembly constraints: an ambiguous `AssemblyRef` target satisfied
/// by any one of several candidates, rather than a single mod — lists only
/// the violated ones, with the total count.
fn print_constraints<W: Write>(printer: &mut Printer<W>, report: &Report, verbose: bool) {
    let violated: Vec<&Constraint> = report
        .constraints
        .iter()
        .filter(|c| {
            let Constraint::AnyOf { status, .. } = c;
            *status == ConstraintStatus::Violated
        })
        .collect();
    printer.line(format_args!(
        "\nAny-of assembly constraints violated: {} (of {} built)",
        violated.len(),
        report.constraints.len()
    ));
    let shown = if verbose {
        violated.len()
    } else {
        SUMMARY_TOP_N.min(violated.len())
    };
    for constraint in &violated[..shown] {
        printer.line(format_args!("  {}", describe_constraint(constraint)));
    }
    if !verbose && violated.len() > shown {
        printer.line(format_args!(
            "  ... and {} more (use --verbose or --json to see all)",
            violated.len() - shown
        ));
    }
}

fn describe_constraint(constraint: &Constraint) -> String {
    let Constraint::AnyOf {
        after,
        assembly,
        candidates,
        load_time,
        ..
    } = constraint;
    let candidate_list = candidates
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    let kind = if *load_time { "load-time" } else { "lazy" };
    format!("{after} references '{assembly}' ({kind}) — any of: {candidate_list}")
}

/// Which of the four mutually exclusive def-override categories a
/// [`DefOverride`] falls into, in priority order: a vanilla override (almost
/// always deliberate) beats a declared-intent one, which beats a
/// framework-shadowing one, which beats "unexplained".
enum DefOverrideCategory {
    Vanilla,
    DeclaredIntent,
    FrameworkShadowed,
    Unexplained,
}

/// `c`'s category in the order the scan ran in: its winner is the last
/// owner, `owners` being in that order.
fn categorize_def_override(c: &DefOverride, mods: &ModsById<'_>) -> DefOverrideCategory {
    let Some(winner) = c.owners.last() else {
        return DefOverrideCategory::Unexplained;
    };
    // A framework def being shadowed matters even when the winner declares
    // loadAfter on the framework: the declaration explains the order, not
    // why a copy of the framework's def is shipped at all.
    if c.overrides_vanilla {
        DefOverrideCategory::Vanilla
    } else if c.shadows_framework(winner, mods) {
        DefOverrideCategory::FrameworkShadowed
    } else if c.winner_declares_relation(winner, mods) {
        DefOverrideCategory::DeclaredIntent
    } else {
        DefOverrideCategory::Unexplained
    }
}

fn print_def_overrides<W: Write>(printer: &mut Printer<W>, report: &Report, verbose: bool) {
    let overrides: Vec<&DefOverride> = report
        .conflicts
        .iter()
        .filter_map(|c| match c {
            Conflict::DefOverride(d) => Some(d),
            _ => None,
        })
        .collect();

    let mut vanilla = Vec::new();
    let mut declared_intent = Vec::new();
    let mut framework_shadowed = Vec::new();
    let mut unexplained = Vec::new();
    let mods: ModsById<'_> = report.mods.iter().map(|m| (m.id.clone(), m)).collect();
    for c in overrides {
        match categorize_def_override(c, &mods) {
            DefOverrideCategory::Vanilla => vanilla.push(c),
            DefOverrideCategory::DeclaredIntent => declared_intent.push(c),
            DefOverrideCategory::FrameworkShadowed => framework_shadowed.push(c),
            DefOverrideCategory::Unexplained => unexplained.push(c),
        }
    }

    printer.line(format_args!(
        "\nDefOverride: {} (vanilla {}, declared-intent {}, framework-shadowed {}, unexplained {})",
        vanilla.len() + declared_intent.len() + framework_shadowed.len() + unexplained.len(),
        vanilla.len(),
        declared_intent.len(),
        framework_shadowed.len(),
        unexplained.len()
    ));
    print_def_override_list(
        printer,
        "  framework defs shadowed:",
        &framework_shadowed,
        verbose,
    );
    print_def_override_list(printer, "  unexplained overrides:", &unexplained, verbose);
}

fn print_def_override_list<W: Write>(
    printer: &mut Printer<W>,
    title: &str,
    items: &[&DefOverride],
    verbose: bool,
) {
    if items.is_empty() {
        return;
    }
    printer.line(format_args!("{title}"));
    let shown = if verbose {
        items.len()
    } else {
        SUMMARY_TOP_N.min(items.len())
    };
    for c in &items[..shown] {
        let winner = c
            .owners
            .last()
            .map_or_else(|| "none".to_string(), ToString::to_string);
        printer.line(format_args!(
            "    {}[{}] owned by {} mods, winner {winner}",
            c.def_type,
            c.def_name,
            c.owners.len(),
        ));
    }
    if !verbose && items.len() > shown {
        printer.line(format_args!(
            "    ... and {} more (use --verbose or --json to see all)",
            items.len() - shown
        ));
    }
}

/// Patch collisions: the count line breaks down contested vs. additive, but
/// only contested collisions — the ones where load order between the
/// contributing mods can actually change the outcome — are listed.
fn print_patch_collisions<W: Write>(printer: &mut Printer<W>, report: &Report, verbose: bool) {
    let collisions: Vec<&PatchCollision> = report
        .conflicts
        .iter()
        .filter_map(|c| match c {
            Conflict::PatchCollision(p) => Some(p),
            _ => None,
        })
        .collect();
    let mut contested: Vec<&PatchCollision> = collisions
        .iter()
        .filter(|c| c.severity == PatchCollisionSeverity::Contested)
        .copied()
        .collect();
    let additive_count = collisions.len() - contested.len();

    printer.line(format_args!(
        "\nPatchCollision: {} (contested {}, additive {})",
        collisions.len(),
        contested.len(),
        additive_count
    ));
    contested.sort_by_key(|c| std::cmp::Reverse(c.mods.len()));
    let shown = if verbose {
        contested.len()
    } else {
        SUMMARY_TOP_N.min(contested.len())
    };
    for c in &contested[..shown] {
        printer.line(format_args!(
            "  {}[{}]{} patched by {} mods",
            c.def_type,
            c.def_name,
            c.sub_path.as_deref().unwrap_or(""),
            c.mods.len()
        ));
    }
    if !verbose && contested.len() > shown {
        printer.line(format_args!(
            "  ... and {} more (use --verbose or --json to see all)",
            contested.len() - shown
        ));
    }
}

/// The conflict kinds that still render as a plain top-N-by-owner-count list:
/// `TextureOverride`, `DuplicateAssembly`, `DuplicateTemplateName`,
/// `KeyedTranslationCollision`, `SoundOverride`, `RuntimePatchCollision`,
/// `MissingTexturePath`, `TranspilerCollision`, `UndecodableTexture`,
/// `BrokenInheritance`, `NearMissModReference`, and `DiscardedAddition` —
/// diagnostic-only, but still worth a plain-text tally here rather than being
/// visible only through the ledger's own `Finding` rows. `DefOverride`,
/// `PatchCollision`, and `LikelyDuplicateMod` each get their own section.
fn print_generic_conflicts<W: Write>(printer: &mut Printer<W>, report: &Report, verbose: bool) {
    let mut by_kind: [(&str, Vec<&Conflict>); 13] = [
        ("TextureOverride", Vec::new()),
        ("DuplicateAssembly", Vec::new()),
        ("DuplicateTemplateName", Vec::new()),
        ("KeyedTranslationCollision", Vec::new()),
        ("SoundOverride", Vec::new()),
        ("RuntimePatchCollision", Vec::new()),
        ("MissingTexturePath", Vec::new()),
        ("TranspilerCollision", Vec::new()),
        ("UndecodableTexture", Vec::new()),
        ("BrokenInheritance", Vec::new()),
        ("NearMissModReference", Vec::new()),
        ("DiscardedAddition", Vec::new()),
        ("DanglingDefReference", Vec::new()),
    ];
    for conflict in &report.conflicts {
        let index = match conflict {
            Conflict::TextureOverride(_) => 0,
            Conflict::DuplicateAssembly(_) => 1,
            Conflict::DuplicateTemplateName(_) => 2,
            Conflict::KeyedTranslationCollision(_) => 3,
            Conflict::SoundOverride(_) => 4,
            Conflict::RuntimePatchCollision(_) => 5,
            Conflict::MissingTexturePath(_) => 6,
            Conflict::TranspilerCollision(_) => 7,
            Conflict::UndecodableTexture(_) => 8,
            Conflict::BrokenInheritance(_) => 9,
            Conflict::NearMissModReference(_) => 10,
            Conflict::DiscardedAddition(_) => 11,
            Conflict::DanglingDefReference(_) => 12,
            Conflict::DefOverride(_)
            | Conflict::PatchCollision(_)
            | Conflict::LikelyDuplicateMod(_) => {
                continue;
            }
        };
        by_kind[index].1.push(conflict);
    }

    for (name, items) in &mut by_kind {
        printer.line(format_args!("\n{name}: {}", items.len()));
        items.sort_by_key(|c| std::cmp::Reverse(c.owner_count()));
        let shown = if verbose {
            items.len()
        } else {
            SUMMARY_TOP_N.min(items.len())
        };
        for conflict in &items[..shown] {
            printer.line(format_args!("  {}", describe_conflict(conflict)));
        }
        if !verbose && items.len() > shown {
            printer.line(format_args!(
                "  ... and {} more (use --verbose or --json to see all)",
                items.len() - shown
            ));
        }
    }
}

fn describe_conflict(conflict: &Conflict) -> String {
    match conflict {
        Conflict::TextureOverride(c) => {
            format!("{} shipped by {} mods", c.texture_path, c.owners.len())
        }
        Conflict::DuplicateAssembly(c) => {
            format!("{} shipped by {} mods", c.assembly_name, c.owners.len())
        }
        Conflict::DuplicateTemplateName(c) => {
            format!("{} registered by {} mods", c.name, c.owners.len())
        }
        Conflict::KeyedTranslationCollision(c) => {
            format!("{} defined by {} mods", c.key, c.owners.len())
        }
        Conflict::SoundOverride(c) => {
            format!("{} shipped by {} mods", c.path, c.owners.len())
        }
        Conflict::RuntimePatchCollision(c) => {
            format!(
                "{}.{} patched by {} mods",
                c.target_type,
                c.target_method,
                c.owners.len()
            )
        }
        Conflict::MissingTexturePath(c) => {
            format!(
                "{}/{} field '{}' names '{}', shipped by no active mod",
                c.def_type, c.def_name, c.field, c.path
            )
        }
        Conflict::TranspilerCollision(c) => {
            format!(
                "{}.{} transpiled by {} mods",
                c.target_type,
                c.target_method,
                c.owners.len()
            )
        }
        Conflict::UndecodableTexture(c) => {
            format!(
                "{} in {} won't decode ({}x{}, {})",
                c.path, c.mod_id, c.width, c.height, c.fourcc
            )
        }
        Conflict::BrokenInheritance(c) => {
            let problem = match &c.problem {
                crate::domain::InheritanceProblem::MissingParent => {
                    "no active mod defines it".to_string()
                }
                crate::domain::InheritanceProblem::ParentTypeMismatch { parent_type, .. } => {
                    format!("resolves to a {parent_type}, not {}", c.child.def_type)
                }
            };
            format!(
                "{} in {} inherits from '{}': {problem} ({} affected def(s))",
                c.child.name,
                c.mod_id,
                c.parent_name,
                c.affected.len()
            )
        }
        Conflict::NearMissModReference(c) => {
            format!(
                "{} references '{}', closest active mod is {} ({:?})",
                c.referrer, c.written, c.candidate_name, c.rule
            )
        }
        Conflict::DiscardedAddition(c) => {
            format!(
                "{} replaces '{}', deliberately discarding {}'s own addition at '{}' ({} declares it loads after {})",
                c.replacer, c.path, c.adder, c.adder_path, c.replacer, c.adder
            )
        }
        Conflict::DanglingDefReference(c) => {
            let cause = match &c.cause {
                crate::domain::DanglingCause::RemovedBy { mod_id, .. } => {
                    format!("removed by {mod_id}'s patch")
                }
                crate::domain::DanglingCause::OnlyInUnloadedFolder { mod_id, folder } => {
                    format!("defined only in {mod_id}'s unloaded '{folder}' folder")
                }
                crate::domain::DanglingCause::OnlyInInactiveMod { mod_id } => {
                    format!("defined only in the inactive mod {mod_id}")
                }
                crate::domain::DanglingCause::DefinedNowhere => "defined nowhere".to_string(),
                crate::domain::DanglingCause::Unexplained => "cause not determined".to_string(),
            };
            format!(
                "'{}' referenced by {} site(s), {cause}",
                c.name,
                c.referrers.len()
            )
        }
        Conflict::DefOverride(_)
        | Conflict::PatchCollision(_)
        | Conflict::LikelyDuplicateMod(_) => {
            unreachable!("only rendered by their own dedicated section")
        }
    }
}

/// Mods that look like duplicates or forks of the same content.
fn print_likely_duplicate_mods<W: Write>(printer: &mut Printer<W>, report: &Report, verbose: bool) {
    let duplicates: Vec<_> = report
        .conflicts
        .iter()
        .filter_map(|c| match c {
            Conflict::LikelyDuplicateMod(d) => Some(d),
            _ => None,
        })
        .collect();
    print_capped(
        printer,
        "Likely duplicate or forked mods",
        &duplicates,
        verbose,
        |d| {
            format!(
                "{} <-> {}: {} shared defs ({:.0}% of the smaller mod)",
                d.a,
                d.b,
                d.shared_defs,
                d.share_of_smaller * 100.0
            )
        },
    );
}

fn print_framework_candidates<W: Write>(printer: &mut Printer<W>, report: &Report, verbose: bool) {
    let mut candidates: Vec<&Mod> = report
        .mods
        .iter()
        .filter(|m| m.is_framework_candidate)
        .collect();
    candidates.sort_by_key(|m| std::cmp::Reverse(m.hard_dependents));
    print_capped(
        printer,
        "Framework candidates (by hard dependents)",
        &candidates,
        verbose,
        |m| {
            format!(
                "{} — hard dependents: {}, soft dependents: {}, awareness dependents: {}",
                m.id, m.hard_dependents, m.soft_dependents, m.awareness_dependents
            )
        },
    );
}

/// The discovered-but-inactive mod list, capped like every other list in this
/// summary. Already sorted by id
/// ([`crate::infra::discovery::Discovered::inactive`]'s own contract) — no
/// re-sort here.
fn print_inactive_mods<W: Write>(printer: &mut Printer<W>, report: &Report, verbose: bool) {
    print_capped(
        printer,
        "Inactive mods (discovered but not active)",
        &report.inactive_mods,
        verbose,
        describe_inactive_mod,
    );
}

fn describe_inactive_mod(m: &InactiveMod) -> String {
    format!("{}  {}  {}", m.id, m.name, m.source)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn print_capped_truncates_and_reports_the_remainder() {
        let mut printer = Printer::new(Vec::new());
        let items: Vec<i32> = (0..25).collect();
        print_capped(&mut printer, "Numbers", &items, false, |n| n.to_string());
        let output = String::from_utf8(printer.out).unwrap();
        assert!(output.contains("Numbers: 25"));
        assert!(output.contains("... and 5 more"));
        assert_eq!(output.lines().filter(|l| l.trim() == "19").count(), 1);
        assert!(!output.contains("\n  20\n"));
    }

    #[test]
    fn print_capped_shows_everything_when_verbose() {
        let mut printer = Printer::new(Vec::new());
        let items: Vec<i32> = (0..25).collect();
        print_capped(&mut printer, "Numbers", &items, true, |n| n.to_string());
        let output = String::from_utf8(printer.out).unwrap();
        assert!(!output.contains("more"));
        assert!(output.contains("  24"));
    }

    #[test]
    fn printer_marks_itself_broken_instead_of_panicking_on_write_failure() {
        struct AlwaysFails;
        impl Write for AlwaysFails {
            fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
                Err(std::io::Error::from(std::io::ErrorKind::BrokenPipe))
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut printer = Printer::new(AlwaysFails);
        printer.line(format_args!("this fails"));
        assert!(printer.broken);
        // A second call must not panic or attempt to write again.
        printer.line(format_args!("this is a no-op"));
    }

    fn edge(kind: EdgeKind, load_time: bool) -> Edge {
        Edge {
            after: crate::domain::ModId::new("a"),
            before: crate::domain::ModId::new("b"),
            kind,
            detail: String::new(),
            load_time,
            subject: None,
        }
    }

    /// A violated `PatchInjectedNode` edge is visible in the text summary —
    /// it's `Hard`-strength but not `AssemblyRef`, the only kind the
    /// "Violated load-time assembly references" section covers.
    #[test]
    fn is_other_hard_edge_includes_patch_injected_node() {
        assert!(is_other_hard_edge(&edge(EdgeKind::PatchInjectedNode, true)));
    }

    #[test]
    fn is_other_hard_edge_includes_load_time_assembly_ref() {
        assert!(is_other_hard_edge(&edge(EdgeKind::AssemblyRef, true)));
    }

    #[test]
    fn is_other_hard_edge_excludes_lazy_assembly_ref() {
        assert!(!is_other_hard_edge(&edge(EdgeKind::AssemblyRef, false)));
    }

    /// `ForceLoadAfter`/`ForceLoadBefore` are `Hard` too, but they keep
    /// their own dedicated section (above, in `print_summary`) — this
    /// predicate must exclude them so they aren't listed twice.
    #[test]
    fn is_other_hard_edge_excludes_force_load_edges() {
        assert!(!is_other_hard_edge(&edge(EdgeKind::ForceLoadAfter, true)));
        assert!(!is_other_hard_edge(&edge(EdgeKind::ForceLoadBefore, true)));
    }

    #[test]
    fn is_other_hard_edge_excludes_awareness_and_declared_kinds() {
        assert!(!is_other_hard_edge(&edge(EdgeKind::UsesType, true)));
        assert!(!is_other_hard_edge(&edge(EdgeKind::ModDependency, true)));
    }

    /// A violated `Inferred`-strength edge has a dedicated section in the
    /// text summary.
    #[test]
    fn is_inferred_edge_includes_every_inferred_kind() {
        assert!(is_inferred_edge(&edge(EdgeKind::PatchRemovedNode, true)));
        assert!(is_inferred_edge(&edge(EdgeKind::RetextureAfterOwner, true)));
        assert!(is_inferred_edge(&edge(
            EdgeKind::DefOverrideAfterOrigin,
            true
        )));
        assert!(is_inferred_edge(&edge(
            EdgeKind::PatchRemovedNodeCosmetic,
            true
        )));
    }

    #[test]
    fn is_inferred_edge_excludes_awareness_and_declared_kinds() {
        assert!(!is_inferred_edge(&edge(EdgeKind::UsesType, true)));
        assert!(!is_inferred_edge(&edge(EdgeKind::ModDependency, true)));
    }

    fn def_override(overrides_vanilla: bool) -> DefOverride {
        DefOverride {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
            owners: vec![
                crate::domain::ModId::new("a"),
                crate::domain::ModId::new("b"),
            ],
            overrides_vanilla,
            same_author: false,
        }
    }

    fn mod_entry(id: &str, load_after: &[&str], is_framework_candidate: bool) -> Mod {
        let declared = crate::domain::DeclaredOrder {
            load_after: load_after
                .iter()
                .map(|other| crate::domain::ModId::new(*other))
                .collect(),
            ..crate::domain::DeclaredOrder::default()
        };
        let json = serde_json::json!({
            "id": id, "name": id, "authors": [], "url": null, "path": id,
            "source": "local", "supported_versions": [], "declared": declared,
            "loaded_folders": [], "hard_dependents": 0, "soft_dependents": 0,
            "awareness_dependents": 0, "is_framework_candidate": is_framework_candidate,
        });
        serde_json::from_value(json).expect("a valid mod literal")
    }

    fn category(c: &DefOverride, mods: &[Mod]) -> DefOverrideCategory {
        let by_id: ModsById<'_> = mods.iter().map(|m| (m.id.clone(), m)).collect();
        categorize_def_override(c, &by_id)
    }

    #[test]
    fn def_override_category_priority_is_vanilla_then_framework_then_declared_then_unexplained() {
        let framework_shadowed = [mod_entry("a", &[], true), mod_entry("b", &["a"], false)];
        let declared = [mod_entry("a", &[], false), mod_entry("b", &["a"], false)];
        let plain = [mod_entry("a", &[], false), mod_entry("b", &[], false)];

        assert!(matches!(
            category(&def_override(true), &framework_shadowed),
            DefOverrideCategory::Vanilla
        ));
        assert!(matches!(
            category(&def_override(false), &framework_shadowed),
            DefOverrideCategory::FrameworkShadowed
        ));
        assert!(matches!(
            category(&def_override(false), &declared),
            DefOverrideCategory::DeclaredIntent
        ));
        assert!(matches!(
            category(&def_override(false), &plain),
            DefOverrideCategory::Unexplained
        ));
    }
}
