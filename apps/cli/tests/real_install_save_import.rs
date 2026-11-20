//! Real-install check for `rimmerge order import` on a RimWorld **save**: a
//! real save is hundreds of megabytes, and only its `<meta>` header may be
//! read. Reads one real save strictly read-only (`--dry-run`, a copy of the
//! real `ModsConfig.xml`, a scratch profile) and checks that
//!
//! - the header reads as a list with as many entries as an independent
//!   count of `<li>` under `modIds` in the same header prefix finds,
//! - reading it stays fast (a regression check against the tier's 2 s command
//!   budget; the proof that the body is not read is `rim-io`'s read-count
//!   tests, not the clock), and
//! - the CLI preview succeeds and prints the same plan on two runs.
//!
//! The save is `RIMMERGE_REAL_SAVE` when set, else the newest `.rws` in the
//! `Saves` folder beside the tier's `ModsConfig.xml`'s `Config` folder; with
//! neither, the test prints a `skipping:` line. The save is only ever read.
//!
//! Run with:
//! `RIMMERGE_GAME_DIR=<your install> RIMMERGE_PERF_PROFILE_DIR=<scratch dir> cargo nextest run -p rimmerge-cli --all-features --release --run-ignored ignored-only -E 'binary(real_install_save_import)'`

mod common;

use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use rim_session::mod_list::ModListLimits;
use rim_session::ports::{ModListFileStore, ModListRead};

const RERUN_COMMAND: &str = "RIMMERGE_GAME_DIR=<your install> RIMMERGE_PERF_PROFILE_DIR=<scratch dir> cargo nextest run \
     -p rimmerge-cli --all-features --release --run-ignored ignored-only \
     -E 'binary(real_install_save_import)'";
const SAVE_VAR: &str = "RIMMERGE_REAL_SAVE";
/// A header is a few hundred kilobytes; the whole command budget is 2 s.
const READ_BUDGET: Duration = Duration::from_millis(500);
const END_OF_HEADER: &[u8] = b"</meta>";

/// `RIMMERGE_REAL_SAVE`, else the newest `.rws` in the `Saves` folder next
/// to the real `Config` folder. `None` when there is neither.
fn find_save(real_mods_config: &Path) -> Option<PathBuf> {
    if let Some(named) = std::env::var_os(SAVE_VAR).map(PathBuf::from) {
        assert!(
            named.is_file(),
            "{SAVE_VAR} names {}, which is not a file",
            named.display()
        );
        return Some(named);
    }
    let saves = real_mods_config.parent()?.parent()?.join("Saves");
    fs::read_dir(saves)
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| {
            let path = entry.path();
            path.is_file()
                && path
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("rws"))
        })
        .filter_map(|entry| Some((entry.metadata().ok()?.modified().ok()?, entry.path())))
        .max()
        .map(|(_, path)| path)
}

/// The header prefix of `save`, read independently of the product: chunks
/// until `</meta>` appears.
fn header_prefix(save: &Path) -> Vec<u8> {
    let mut file = File::open(save).unwrap_or_else(|error| panic!("open the save: {error}"));
    let mut prefix = Vec::new();
    let mut chunk = vec![0_u8; 64 * 1024];
    while !prefix
        .windows(END_OF_HEADER.len())
        .any(|w| w == END_OF_HEADER)
    {
        let read = file
            .read(&mut chunk)
            .unwrap_or_else(|error| panic!("read the save: {error}"));
        assert!(read > 0, "the save ended before its </meta>");
        prefix.extend_from_slice(&chunk[..read]);
        assert!(
            prefix.len() <= ModListLimits::MAX_INPUT_BYTES + 1,
            "no </meta> within the input limit"
        );
    }
    prefix
}

/// How many `<li>` the first `<modIds>` list of `header` holds.
fn independent_entry_count(header: &[u8]) -> usize {
    let text = String::from_utf8_lossy(header);
    let after_open = text
        .split("<modIds>")
        .nth(1)
        .unwrap_or_else(|| panic!("the header has no <modIds>"));
    let list = after_open
        .split("</modIds>")
        .next()
        .unwrap_or_else(|| panic!("the header's <modIds> never closes"));
    list.matches("<li>").count()
}

fn dry_run_stdout(
    save: &Path,
    game_dir: &Path,
    workshop_dir: &Path,
    mods_config_copy: &Path,
    profile_dir: &Path,
) -> String {
    let mut command = common::rimmerge();
    command
        .arg("order")
        .arg("import")
        .arg(save)
        .arg("--dry-run")
        .arg("--game-dir")
        .arg(game_dir)
        .arg("--workshop-dir")
        .arg(workshop_dir)
        .arg("--mods-config")
        .arg(mods_config_copy)
        .arg("--profile-dir")
        .arg(profile_dir);
    let output = command
        .output()
        .unwrap_or_else(|error| panic!("run the CLI: {error}"));
    assert!(
        output.status.success(),
        "order import --dry-run failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap_or_else(|error| panic!("stdout was not utf8: {error}"))
}

#[test]
#[ignore = "needs the real install plus RIMMERGE_PERF_PROFILE_DIR pointing at a scratch dir"]
fn a_real_save_previews_from_its_header_alone() {
    let Some(profile_dir) = common::require_profile_dir(RERUN_COMMAND) else {
        return;
    };
    let game_dir = common::require_game_dir(RERUN_COMMAND)
        .unwrap_or_else(|| panic!("require_profile_dir already confirmed the install is present"));
    let workshop_dir = common::real_workshop_dir(&game_dir);
    let real_mods_config = common::real_mods_config_path();
    common::assert_active_mod_floor(&real_mods_config, RERUN_COMMAND);
    let Some(save) = find_save(&real_mods_config) else {
        eprintln!(
            "skipping: no save found (set {SAVE_VAR}, or keep a .rws in the Saves folder next to Config)"
        );
        return;
    };
    fs::create_dir_all(&profile_dir)
        .unwrap_or_else(|error| panic!("create scratch profile dir: {error}"));
    let mods_config_copy = profile_dir.join("real_mods_config_copy.xml");
    fs::copy(&real_mods_config, &mods_config_copy)
        .unwrap_or_else(|error| panic!("copy the real ModsConfig.xml: {error}"));

    let prefix = header_prefix(&save);
    let header_bytes = prefix
        .windows(END_OF_HEADER.len())
        .position(|window| window == END_OF_HEADER)
        .map(|at| at + END_OF_HEADER.len())
        .unwrap_or_else(|| panic!("header_prefix returned a prefix without </meta>"));
    let expected_entries = independent_entry_count(&prefix);
    let started = Instant::now();
    let read = rim_io::RmlFileStore::new()
        .read(&save)
        .unwrap_or_else(|error| panic!("read the save: {error}"));
    let elapsed = started.elapsed();
    let ModListRead::Parsed(parsed) = read else {
        panic!("a real save's header must read as a list, got {read:?}");
    };

    assert!(expected_entries > 0, "the independent count found no mods");
    assert_eq!(
        parsed.list.entries().len() + parsed.skipped.len() + parsed.omitted_skipped,
        expected_entries,
        "every <li> is an entry or a reported skip"
    );
    assert!(
        elapsed < READ_BUDGET,
        "reading the header took {elapsed:?}: the body was read"
    );
    // Proof the test ran on a real save, without naming the file or any mod.
    eprintln!(
        "read a real save header: {header_bytes} header bytes, {expected_entries} entries ({} listed, {} skipped), read in {elapsed:?}",
        parsed.list.entries().len(),
        parsed.skipped.len() + parsed.omitted_skipped
    );
    let first = dry_run_stdout(
        &save,
        &game_dir,
        &workshop_dir,
        &mods_config_copy,
        &profile_dir,
    );
    let second = dry_run_stdout(
        &save,
        &game_dir,
        &workshop_dir,
        &mods_config_copy,
        &profile_dir,
    );
    assert!(
        first.contains("source: RimWorld save (header only)"),
        "got:\n{first}"
    );
    assert_eq!(first, second, "the preview must be identical across runs");
}
