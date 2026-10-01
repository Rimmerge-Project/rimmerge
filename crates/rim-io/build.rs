//! Fails the build early, with a clear message, when `rules/` (the
//! `rimmerge-rules` git submodule) hasn't been checked out — a build
//! must never silently ship without the mod-knowledge bundle
//! `mod_knowledge` embeds via `include_str!`. `include_str!` itself
//! would eventually fail too, but with a bare "file not found" pointing
//! at a relative path, not at the actual fix.
//!
//! Also hashes the bundle and exposes it as `RIMMERGE_RULES_BUNDLE_SHA256`
//! (`env!`-readable from `mod_knowledge.rs`), so `db status`/the desktop's
//! Databases card can show the embedded snapshot's own identity next to
//! a fetched cache's sha256 — cheap, since the file is a few KB.

use std::path::Path;

use sha2::{Digest, Sha256};

fn main() {
    // Relative to `crates/rim-io`, the same base `mod_knowledge.rs`'s own
    // `include_str!(concat!(env!("CARGO_MANIFEST_DIR"), ...))` resolves
    // from, so the two can never disagree about which file this is.
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let bundle_path = Path::new(manifest_dir).join("../../rules/rimmerge-rules.json");

    println!("cargo:rerun-if-changed={}", bundle_path.display());

    if !bundle_path.is_file() {
        panic!("rules/rimmerge-rules.json not found — run: git submodule update --init");
    }

    let bytes = std::fs::read(&bundle_path).unwrap_or_else(|error| {
        panic!("{}: {error}", bundle_path.display());
    });
    let sha256 = hex(&Sha256::digest(&bytes));
    println!("cargo:rustc-env=RIMMERGE_RULES_BUNDLE_SHA256={sha256}");
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
