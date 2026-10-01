//! `rimmerge startup`: prints the per-mod startup-cost table from a JSON
//! `Report` — one row per active mod plus a totals row, no timings
//! (Rimmerge never runs the game, and a guessed cost model would be
//! decoration; `log import` supplies real timers when the user has a game
//! log). JSON-in only, the same shape `sort`/`ledger`
//! already are — `Report.mod_costs` is computed once at scan time, so
//! this command does no computation of its own beyond sorting/rendering.

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::Context;
use clap::{Args, ValueEnum};
use rim_analyzer::domain::{Mod, ModCost, ModId};
use serde::Serialize;

use crate::common::{TerminalSafe, read_report};

#[derive(Debug, Clone, Copy, ValueEnum)]
enum SortByArg {
    TextureBytes,
    TextureFiles,
    DdsFiles,
    PatchOps,
    SlowXpathOps,
    AssemblyBytes,
    DefCount,
    OverriddenTextureBytes,
}

#[derive(Debug, Args)]
pub struct StartupArgs {
    /// Path to a JSON `Report` (e.g. from `rimmerge load` or `rim-analyzer
    /// analyze --json`).
    #[arg(long)]
    report: PathBuf,
    /// Which column to sort descending by. Defaults to `texture-bytes` —
    /// real-install measurement found texture loading the dominant
    /// startup cost.
    #[arg(long, value_enum, default_value_t = SortByArg::TextureBytes)]
    sort_by: SortByArg,
    /// Print machine-readable JSON instead of text.
    #[arg(long)]
    json: bool,
}

pub fn run(args: &StartupArgs) -> anyhow::Result<()> {
    let report = read_report(&args.report)?;
    // `Report.mod_costs` is `#[serde(default)]`
    // for the same reason `Edge.subject`/`workshop_id`/... are — an older
    // cached report must still deserialize. But unlike those fields, an
    // *empty* `mod_costs` on a report that plainly scanned real mods is
    // not "the right default", it's silent data loss: rendering it
    // verbatim would print "this install costs nothing" (exit 0) for
    // every report scanned before startup costs existed. Fail fast
    // instead — before any
    // sorting/rendering — rather than let `#[serde(default)]`'s emptiness
    // read as fact. Checked against `report.mods`, not `schema_version`:
    // that field is itself `#[serde(default)]` and reads back `0` for any
    // report predating it, which would misname the actual schema gap in
    // the error text.
    anyhow::ensure!(
        !(report.mod_costs.is_empty() && !report.mods.is_empty()),
        "this report predates startup costs (schema {} < {}); regenerate it with \
         `cargo run -p rim-analyzer --release -- analyze --json {}`",
        report.metadata.schema_version,
        rim_analyzer::domain::REPORT_SCHEMA_VERSION,
        args.report.display()
    );
    let mods_by_id: BTreeMap<ModId, &Mod> = report.mods.iter().map(|m| (m.id.clone(), m)).collect();

    let mut rows: Vec<&ModCost> = report.mod_costs.iter().collect();
    rows.sort_by_key(|cost| std::cmp::Reverse(sort_key(cost, args.sort_by)));

    if args.json {
        return print_json(&rows, &mods_by_id);
    }
    print_text(&rows, &mods_by_id);
    Ok(())
}

fn sort_key(cost: &ModCost, sort_by: SortByArg) -> u64 {
    match sort_by {
        SortByArg::TextureBytes => cost.texture_bytes,
        SortByArg::TextureFiles => cost.texture_files,
        SortByArg::DdsFiles => cost.dds_files,
        SortByArg::PatchOps => cost.patch_ops as u64,
        SortByArg::SlowXpathOps => cost.slow_xpath_ops as u64,
        SortByArg::AssemblyBytes => cost.assembly_bytes,
        SortByArg::DefCount => cost.def_count as u64,
        SortByArg::OverriddenTextureBytes => cost.overridden_texture_bytes,
    }
}

fn mod_name<'a>(mods_by_id: &BTreeMap<ModId, &'a Mod>, id: &'a ModId) -> &'a str {
    mods_by_id
        .get(id)
        .map_or_else(|| id.as_str(), |m| m.name.as_str())
}

/// `1536` -> `"1.50 KiB"`, `0` -> `"0 B"` — human-readable byte formatting
/// for the text table; `--json` carries the raw `u64` instead. `KiB`/
/// `MiB`/`GiB`, not `KB`/`MB`/`GB` — the divisor is 1024, and those
/// suffixes are reserved for a decimal (1000-based) scale.
fn format_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;
    let bytes_f = bytes as f64;
    if bytes_f >= GIB {
        format!("{:.2} GiB", bytes_f / GIB)
    } else if bytes_f >= MIB {
        format!("{:.2} MiB", bytes_f / MIB)
    } else if bytes_f >= KIB {
        format!("{:.2} KiB", bytes_f / KIB)
    } else {
        format!("{bytes} B")
    }
}

fn flags(cost: &ModCost) -> String {
    let mut parts = Vec::new();
    if cost.content_only {
        parts.push("content-only".to_string());
    }
    if cost.overridden_texture_bytes > 0 {
        parts.push(format!(
            "overridden:{}",
            format_bytes(cost.overridden_texture_bytes)
        ));
    }
    parts.join(", ")
}

fn print_text(rows: &[&ModCost], mods_by_id: &BTreeMap<ModId, &Mod>) {
    println!(
        "Startup cost ({} mods) — patch ops (mutating), slow-shape xpaths, textures, DLLs, defs; no timings (import a Player.log for those)",
        rows.len()
    );
    println!(
        "{:<40.40} {:>6} {:>5} {:>10} {:>10} {:>6} {:>4} {:>10} {:>5}  FLAGS",
        "MOD", "PATCH", "SLOW", "TEX_FILES", "TEX_BYTES", "DDS", "DLL", "DLL_BYTES", "DEFS"
    );
    let mut totals = Totals::default();
    for cost in rows {
        totals.add(cost);
        println!(
            "{:<40.40} {:>6} {:>5} {:>10} {:>10} {:>6} {:>4} {:>10} {:>5}  {}",
            TerminalSafe::line(mod_name(mods_by_id, &cost.mod_id)).to_string(),
            cost.patch_ops,
            cost.slow_xpath_ops,
            cost.texture_files,
            format_bytes(cost.texture_bytes),
            cost.dds_files,
            cost.assembly_count,
            format_bytes(cost.assembly_bytes),
            cost.def_count,
            flags(cost)
        );
    }
    println!(
        "{:<40.40} {:>6} {:>5} {:>10} {:>10} {:>6} {:>4} {:>10} {:>5}",
        "TOTAL",
        totals.patch_ops,
        totals.slow_xpath_ops,
        totals.texture_files,
        format_bytes(totals.texture_bytes),
        totals.dds_files,
        totals.assembly_count,
        format_bytes(totals.assembly_bytes),
        totals.def_count
    );
}

#[derive(Debug, Default, Serialize)]
struct Totals {
    patch_ops: usize,
    slow_xpath_ops: usize,
    texture_files: u64,
    texture_bytes: u64,
    dds_files: u64,
    assembly_count: usize,
    assembly_bytes: u64,
    def_count: usize,
}

impl Totals {
    fn add(&mut self, cost: &ModCost) {
        self.patch_ops += cost.patch_ops;
        self.slow_xpath_ops += cost.slow_xpath_ops;
        self.texture_files += cost.texture_files;
        self.texture_bytes += cost.texture_bytes;
        self.dds_files += cost.dds_files;
        self.assembly_count += cost.assembly_count;
        self.assembly_bytes += cost.assembly_bytes;
        self.def_count += cost.def_count;
    }
}

#[derive(Debug, Serialize)]
struct ModCostRowJson {
    mod_id: String,
    name: String,
    patch_ops: usize,
    slow_xpath_ops: usize,
    texture_files: u64,
    texture_bytes: u64,
    dds_files: u64,
    assembly_count: usize,
    assembly_bytes: u64,
    def_count: usize,
    content_only: bool,
    overridden_texture_bytes: u64,
}

#[derive(Debug, Serialize)]
struct StartupJson {
    mods: Vec<ModCostRowJson>,
    totals: Totals,
}

fn print_json(rows: &[&ModCost], mods_by_id: &BTreeMap<ModId, &Mod>) -> anyhow::Result<()> {
    // CQS: fold the totals in their own pass
    // first, rather than accumulating as a side effect inside the `.map`
    // that builds each JSON row below.
    let mut totals = Totals::default();
    for cost in rows {
        totals.add(cost);
    }
    let mods = rows
        .iter()
        .map(|cost| ModCostRowJson {
            mod_id: cost.mod_id.to_string(),
            name: mod_name(mods_by_id, &cost.mod_id).to_string(),
            patch_ops: cost.patch_ops,
            slow_xpath_ops: cost.slow_xpath_ops,
            texture_files: cost.texture_files,
            texture_bytes: cost.texture_bytes,
            dds_files: cost.dds_files,
            assembly_count: cost.assembly_count,
            assembly_bytes: cost.assembly_bytes,
            def_count: cost.def_count,
            content_only: cost.content_only,
            overridden_texture_bytes: cost.overridden_texture_bytes,
        })
        .collect();
    let json = StartupJson { mods, totals };
    let text =
        serde_json::to_string_pretty(&json).context("serializing the startup table as JSON")?;
    println!("{text}");
    Ok(())
}
