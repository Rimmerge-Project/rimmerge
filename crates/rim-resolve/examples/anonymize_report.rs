//! Deterministically maps every `ModId`, display name, author, url,
//! path, def name, texture/sound path, translation key, assembly name,
//! and workshop id in a `Report` to an invented token, preserving the
//! mapping's injectivity and the report's structure exactly, so
//! `sort_equivalence` can prove a rename cannot silently change the
//! sorter's own output.
//!
//! Generic and name-free: this file names no real mod, and its own
//! output (the anonymized report and the mapping) is never committed —
//! both are safe to publish, but nothing here needs publishing to do its
//! job, which is `sort_equivalence`'s own input.
//!
//! ```text
//! cargo run -p rim-resolve --example anonymize_report -- \
//!     .journal/local/report.json .journal/local/report.anon.json .journal/local/mapping.json
//! ```
//!
//! # What is and isn't mapped
//!
//! `Report.mods`/`Report.inactive_mods` are the authoritative universe of
//! identifiers — every id/name/author/url/path/workshop id anywhere else
//! in the report is one of theirs, so those two arrays are walked first,
//! typed field by field, to build the maps; every other field
//! (`edges`/`conflicts`/`constraints`/`warnings`/...) is then walked
//! generically, dispatching purely on each JSON object key's own name
//! (`Conflict`'s internally-tagged shape means every variant's fields sit
//! flat beside `kind`, so a key-based dispatch sees the same key set
//! regardless of which conflict kind it's in). Free-text fields
//! (`Edge.detail`, `Warning.message`) get a second, substring-based pass
//! over the same maps (longest key first) so a mention *inside* a
//! sentence is caught too, not just a field holding nothing but an id.
//!
//! **Known, disclosed gaps** (this tool's own output is never committed,
//! so these are acceptable): `def_type` (e.g. `ThingDef`, but also a
//! custom type like `example.PartAssignmentDef`), runtime-patch `target_type`/
//! `target_method`, and `Conflict::MissingTexturePath`'s own `field` name
//! are left unmapped — all three are C#/XML identifiers, not mod
//! identities, and mapping them would need a much larger, less certain
//! rename surface for a tool whose only job is proving `sort_equivalence`
//! holds under identifier substitution.

use std::collections::BTreeMap;
use std::env;
use std::path::PathBuf;

use serde_json::{Map, Value};

/// One category's deterministic `original -> token` map, assigned in
/// sorted order over every original value discovered — sorting first is
/// what makes the whole run reproducible byte-for-byte given the same
/// input.
#[derive(Default)]
struct TokenMap {
    forward: BTreeMap<String, String>,
}

impl TokenMap {
    fn get_or_assign(&mut self, original: &str, prefix: &str) -> String {
        if let Some(token) = self.forward.get(original) {
            return token.clone();
        }
        let token = format!("{prefix}{:04}", self.forward.len() + 1);
        self.forward.insert(original.to_string(), token.clone());
        token
    }

    /// Looks up an existing mapping without assigning a new one --
    /// `mods`/`inactive_mods` are walked first specifically so every id
    /// this can be asked about later already has one.
    fn get(&self, original: &str) -> Option<&str> {
        self.forward.get(original).map(String::as_str)
    }
}

/// Every category's map, plus the two path-prefix maps that need
/// special (substring, not equality) handling.
#[derive(Default)]
struct Maps {
    mod_id: TokenMap,
    name: TokenMap,
    author: TokenMap,
    url: TokenMap,
    path: TokenMap,
    asset_path: TokenMap,
    def_name: TokenMap,
    translation_key: TokenMap,
    assembly: TokenMap,
    /// `DuplicateTemplateName.name` -- a distinct namespace from
    /// `def_name` (a template `Name` attribute, not a `defName`), kept
    /// separate so the two never collide under one shared token space.
    template_name: TokenMap,
    /// Original workshop id -> synthetic replacement, assigned in the
    /// same sorted-first-seen order as every other category.
    workshop_id: BTreeMap<u64, u64>,
}

fn str_field<'a>(obj: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    obj.get(key).and_then(Value::as_str)
}

fn str_array(obj: &Map<String, Value>, key: &str) -> Vec<String> {
    obj.get(key)
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Pass 1: walk `mods`/`inactive_mods`, assigning every id/name/author/
/// url/path/workshop id a token in first-seen (i.e. sorted, since both
/// arrays are already `ModId`-sorted by the analyzer) order.
fn collect_mod_maps(report: &Value, maps: &mut Maps) {
    for array_key in ["mods", "inactive_mods"] {
        let Some(entries) = report.get(array_key).and_then(Value::as_array) else {
            continue;
        };
        for entry in entries {
            let Some(obj) = entry.as_object() else {
                continue;
            };
            if let Some(id) = str_field(obj, "id") {
                maps.mod_id.get_or_assign(id, "mod.");
            }
            if let Some(name) = str_field(obj, "name").filter(|n| !n.is_empty()) {
                maps.name.get_or_assign(name, "Anonymized Mod ");
            }
            for author in str_array(obj, "authors") {
                maps.author.get_or_assign(&author, "Author ");
            }
            if let Some(url) = str_field(obj, "url") {
                maps.url.get_or_assign(url, "https://mods.example/");
            }
            if let Some(path) = str_field(obj, "path") {
                maps.path.get_or_assign(path, "path.");
            }
            if let Some(workshop_id) = obj.get("workshop_id").and_then(Value::as_u64) {
                let next = 2_000_000_000 + u64::try_from(maps.workshop_id.len()).unwrap_or(0);
                maps.workshop_id.entry(workshop_id).or_insert(next);
            }
            // `declared.dependencies[].display_name` is the author's own
            // free text naming a dependency -- collected into the same
            // `name` map so it substitutes consistently whether or not
            // it happens to match a real mod's own registered name.
            if let Some(declared) = obj.get("declared").and_then(Value::as_object)
                && let Some(deps) = declared.get("dependencies").and_then(Value::as_array)
            {
                for dep in deps {
                    let Some(dep_obj) = dep.as_object() else {
                        continue;
                    };
                    if let Some(id) = str_field(dep_obj, "id") {
                        maps.mod_id.get_or_assign(id, "mod.");
                    }
                    if let Some(display_name) = str_field(dep_obj, "display_name") {
                        maps.name.get_or_assign(display_name, "Anonymized Mod ");
                    }
                }
            }
        }
    }
}

/// Longest-real-path-first substring substitution -- used for
/// `loaded_folders`, which are a mod's own `path` plus a suffix
/// (`\1.6\Compat`), not an exact match against `path` itself.
fn substitute_path_prefix(value: &str, maps: &Maps) -> String {
    let mut candidates: Vec<(&String, &String)> = maps.path.forward.iter().collect();
    candidates.sort_by_key(|(original, _)| std::cmp::Reverse(original.len()));
    for (original, token) in candidates {
        if let Some(rest) = value.strip_prefix(original.as_str()) {
            return format!("{token}{rest}");
        }
    }
    value.to_string()
}

/// The free-text substring pass (`Edge.detail`, `Warning.message`):
/// every known mod id, name, and author, longest first so a longer id
/// that happens to contain a shorter one substitutes correctly.
fn substitute_mentions(text: &str, maps: &Maps) -> String {
    let mut needles: Vec<(&str, &str)> = Vec::new();
    for (original, token) in &maps.mod_id.forward {
        needles.push((original.as_str(), token.as_str()));
    }
    for (original, token) in &maps.name.forward {
        needles.push((original.as_str(), token.as_str()));
    }
    for (original, token) in &maps.author.forward {
        needles.push((original.as_str(), token.as_str()));
    }
    // Filesystem paths too -- a warning naming a malformed file (e.g.
    // `"C:\Game\Mods\SomeMod\Keys.xml: skipped: ..."`) embeds the full
    // path mid-sentence, not as a standalone field, so it needs the same
    // substring treatment as a mod id or name mention.
    for (original, token) in &maps.path.forward {
        needles.push((original.as_str(), token.as_str()));
    }
    needles.sort_by_key(|(original, _)| std::cmp::Reverse(original.len()));

    let mut out = text.to_string();
    for (original, token) in needles {
        if out.contains(original) {
            out = out.replace(original, token);
        }
    }
    out
}

/// Rewrites one `mods`/`inactive_mods` entry using the already-collected
/// maps -- every field this type carries, typed by field name rather
/// than generic key dispatch (this is the one place `path` unambiguously
/// means "this mod's own folder", so it gets its own handling rather
/// than sharing the generic walk's `path` -> asset-path rule).
fn anonymize_mod_entry(entry: &mut Value, maps: &Maps) {
    let Some(obj) = entry.as_object_mut() else {
        return;
    };
    if let Some(Value::String(id)) = obj.get_mut("id")
        && let Some(token) = maps.mod_id.get(id)
    {
        *id = token.to_string();
    }
    if let Some(Value::String(name)) = obj.get_mut("name")
        && !name.is_empty()
        && let Some(token) = maps.name.get(name)
    {
        *name = token.to_string();
    }
    if let Some(Value::Array(authors)) = obj.get_mut("authors") {
        for author in authors {
            if let Value::String(a) = author
                && let Some(token) = maps.author.get(a)
            {
                *a = token.to_string();
            }
        }
    }
    if let Some(Value::String(url)) = obj.get_mut("url")
        && let Some(token) = maps.url.get(url)
    {
        *url = token.to_string();
    }
    if let Some(Value::String(path)) = obj.get_mut("path")
        && let Some(token) = maps.path.get(path)
    {
        *path = token.to_string();
    }
    if let Some(Value::Array(folders)) = obj.get_mut("loaded_folders") {
        for folder in folders {
            if let Value::String(f) = folder {
                *f = substitute_path_prefix(f, maps);
            }
        }
    }
    if let Some(Value::Number(workshop_id)) = obj.get("workshop_id").cloned()
        && let Some(original) = workshop_id.as_u64()
        && let Some(replacement) = maps.workshop_id.get(&original)
    {
        obj.insert(
            "workshop_id".to_string(),
            Value::Number((*replacement).into()),
        );
    }
    if let Some(declared) = obj.get_mut("declared").and_then(Value::as_object_mut) {
        for key in [
            "load_after",
            "load_before",
            "force_load_after",
            "force_load_before",
            "incompatible_with",
        ] {
            if let Some(Value::Array(ids)) = declared.get_mut(key) {
                for id in ids {
                    if let Value::String(s) = id
                        && let Some(token) = maps.mod_id.get(s)
                    {
                        *s = token.to_string();
                    }
                }
            }
        }
        if let Some(Value::Array(deps)) = declared.get_mut("dependencies") {
            for dep in deps {
                let Some(dep_obj) = dep.as_object_mut() else {
                    continue;
                };
                if let Some(Value::String(id)) = dep_obj.get_mut("id")
                    && let Some(token) = maps.mod_id.get(id)
                {
                    *id = token.to_string();
                }
                if let Some(Value::String(name)) = dep_obj.get_mut("display_name")
                    && let Some(token) = maps.name.get(name)
                {
                    *name = token.to_string();
                }
            }
        }
    }
}

/// Keys holding a bare `ModId` string (equality, never a substring) --
/// used by the generic walk over every field outside `mods`/
/// `inactive_mods`.
const MOD_ID_KEYS: &[&str] = &["after", "before", "winner", "mod_id", "a", "b", "referrer"];
/// Keys holding an array of bare `ModId` strings.
const MOD_ID_ARRAY_KEYS: &[&str] = &[
    "owners",
    "candidates",
    "missing_mods",
    "unsupported_version_mods",
];

/// The generic pass over everything except `mods`/`inactive_mods`
/// (already handled -- see [`anonymize_mod_entry`]): dispatches purely on
/// each object key's own name.
fn anonymize_generic(value: &mut Value, maps: &Maps) {
    match value {
        Value::Array(items) => {
            for item in items {
                anonymize_generic(item, maps);
            }
        }
        Value::Object(obj) => {
            for (key, entry) in obj.iter_mut() {
                match key.as_str() {
                    _ if MOD_ID_KEYS.contains(&key.as_str()) => {
                        if let Value::String(s) = entry
                            && let Some(token) = maps.mod_id.get(s)
                        {
                            *s = token.to_string();
                        }
                    }
                    _ if MOD_ID_ARRAY_KEYS.contains(&key.as_str()) => {
                        if let Value::Array(items) = entry {
                            for item in items {
                                if let Value::String(s) = item
                                    && let Some(token) = maps.mod_id.get(s)
                                {
                                    *s = token.to_string();
                                }
                            }
                        }
                    }
                    "id" => {
                        if let Value::String(s) = entry
                            && let Some(token) = maps.mod_id.get(s)
                        {
                            *s = token.to_string();
                        }
                    }
                    "display_name" => {
                        if let Value::String(s) = entry
                            && let Some(token) = maps.name.get(s)
                        {
                            *s = token.to_string();
                        }
                    }
                    "assembly" | "assembly_name" => {
                        if let Value::String(s) = entry {
                            *s = maps_get_or_placeholder(&maps.assembly, s, "assembly.");
                        }
                    }
                    "texture_path" | "path" => {
                        if let Value::String(s) = entry {
                            *s = maps_get_or_placeholder(&maps.asset_path, s, "asset.");
                        }
                    }
                    "def_name" | "subject" => {
                        if let Value::String(s) = entry {
                            *s = maps_get_or_placeholder(&maps.def_name, s, "SynthDef");
                        }
                    }
                    "key" => {
                        if let Value::String(s) = entry {
                            *s = maps_get_or_placeholder(&maps.translation_key, s, "SynthKey");
                        }
                    }
                    // `Conflict::DuplicateTemplateName.name` -- the only
                    // bare "name" key outside `mods`/`inactive_mods`
                    // (already handled separately, see
                    // `anonymize_mod_entry`), so this arm is unambiguous.
                    "name" => {
                        if let Value::String(s) = entry {
                            *s = maps_get_or_placeholder(&maps.template_name, s, "SynthTemplate");
                        }
                    }
                    // `DuplicateAssembly.versions: Vec<(ModId,
                    // AssemblyVersion)>` -- a tuple, so serde emits a
                    // bare `[id, version]` array with no key naming the
                    // id half; map that first element only.
                    "versions" => {
                        if let Value::Array(pairs) = entry {
                            for pair in pairs {
                                if let Value::Array(fields) = pair
                                    && let Some(Value::String(id)) = fields.first_mut()
                                    && let Some(token) = maps.mod_id.get(id)
                                {
                                    *id = token.to_string();
                                }
                            }
                        }
                    }
                    "detail" | "message" => {
                        if let Value::String(s) = entry {
                            *s = substitute_mentions(s, maps);
                        }
                    }
                    _ => anonymize_generic(entry, maps),
                }
            }
        }
        _ => {}
    }
}

/// `TokenMap::get_or_assign` needs `&mut self`, but the generic walk only
/// holds `&Maps` (it never adds a brand-new original for these leaf
/// categories -- `def_name`/`key`/`texture_path`/`assembly` are read-only
/// lookups here). A miss returns the original text verbatim rather than
/// panicking: these categories are a best-effort pass (see the module
/// doc's "known gaps"), not a completeness guarantee.
fn maps_get_or_placeholder(map: &TokenMap, original: &str, _prefix: &str) -> String {
    map.get(original).unwrap_or(original).to_string()
}

/// Second collection pass, over the same generic-walk field set, filling
/// `asset_path`/`def_name`/`translation_key`/`assembly` maps before the
/// mutating pass above runs (so [`maps_get_or_placeholder`] can stay a
/// read-only lookup).
fn collect_generic_maps(value: &Value, maps: &mut Maps) {
    match value {
        Value::Array(items) => {
            for item in items {
                collect_generic_maps(item, maps);
            }
        }
        Value::Object(obj) => {
            for (key, entry) in obj {
                match key.as_str() {
                    "assembly" | "assembly_name" => {
                        if let Some(s) = entry.as_str() {
                            maps.assembly.get_or_assign(s, "assembly.");
                        }
                    }
                    "texture_path" | "path" => {
                        if let Some(s) = entry.as_str() {
                            maps.asset_path.get_or_assign(s, "asset.");
                        }
                    }
                    "def_name" => {
                        if let Some(s) = entry.as_str() {
                            maps.def_name.get_or_assign(s, "SynthDef");
                        }
                    }
                    "key" => {
                        if let Some(s) = entry.as_str() {
                            maps.translation_key.get_or_assign(s, "SynthKey");
                        }
                    }
                    // `UnresolvedFindMod.display_name` -- a
                    // `PatchOperationFindMod` display name that may not
                    // match any real mod's own `name` (that's the whole
                    // reason it's unresolved), so it needs collecting
                    // here too, not only via `declared.dependencies`
                    // (`collect_mod_maps`).
                    "display_name" => {
                        if let Some(s) = entry.as_str() {
                            maps.name.get_or_assign(s, "Anonymized Mod ");
                        }
                    }
                    // `Edge.subject` -- the structured type/path name an
                    // edge is evidence about (see that field's own doc
                    // comment); treated as best-effort, same
                    // `def_name`-shaped category.
                    "subject" => {
                        if let Some(s) = entry.as_str() {
                            maps.def_name.get_or_assign(s, "SynthDef");
                        }
                    }
                    "name" => {
                        if let Some(s) = entry.as_str() {
                            maps.template_name.get_or_assign(s, "SynthTemplate");
                        }
                    }
                    _ => collect_generic_maps(entry, maps),
                }
            }
        }
        _ => {}
    }
}

fn anonymize_metadata(report: &mut Value) {
    let Some(metadata) = report.get_mut("metadata").and_then(Value::as_object_mut) else {
        return;
    };
    metadata.insert("game_dir".to_string(), Value::String("C:\\Game".into()));
    metadata.insert(
        "workshop_dir".to_string(),
        Value::String("C:\\Game\\Workshop".into()),
    );
    metadata.insert(
        "mods_config".to_string(),
        Value::String("C:\\Game\\ModsConfig.xml".into()),
    );
}

fn main() {
    let mut args = env::args().skip(1);
    let (Some(input), Some(output_report), Some(output_mapping)) =
        (args.next(), args.next(), args.next())
    else {
        eprintln!(
            "usage: anonymize_report <input report.json> <output report.json> <output mapping.json>"
        );
        std::process::exit(2);
    };
    let input = PathBuf::from(input);
    let bytes = std::fs::read(&input).unwrap_or_else(|e| {
        eprintln!("failed to read {}: {e}", input.display());
        std::process::exit(1);
    });
    let mut report: Value = serde_json::from_slice(&bytes).unwrap_or_else(|e| {
        eprintln!("failed to parse {}: {e}", input.display());
        std::process::exit(1);
    });

    let mut maps = Maps::default();
    collect_mod_maps(&report, &mut maps);
    collect_generic_maps(&report, &mut maps);

    if let Some(Value::Array(entries)) = report.get_mut("mods") {
        for entry in entries {
            anonymize_mod_entry(entry, &maps);
        }
    }
    if let Some(Value::Array(entries)) = report.get_mut("inactive_mods") {
        for entry in entries {
            anonymize_mod_entry(entry, &maps);
        }
    }
    anonymize_metadata(&mut report);

    if let Some(obj) = report.as_object_mut() {
        for (key, value) in obj.iter_mut() {
            if key != "mods" && key != "inactive_mods" && key != "metadata" {
                anonymize_generic(value, &maps);
            }
        }
    }

    let report_bytes = serde_json::to_vec_pretty(&report).unwrap_or_else(|e| {
        eprintln!("failed to serialize anonymized report: {e}");
        std::process::exit(1);
    });
    if let Err(e) = std::fs::write(&output_report, report_bytes) {
        eprintln!("failed to write {output_report}: {e}");
        std::process::exit(1);
    }

    let mapping = serde_json::json!({
        "mod_id": maps.mod_id.forward,
        "name": maps.name.forward,
        "author": maps.author.forward,
        "url": maps.url.forward,
        "path": maps.path.forward,
        "asset_path": maps.asset_path.forward,
        "def_name": maps.def_name.forward,
        "template_name": maps.template_name.forward,
        "translation_key": maps.translation_key.forward,
        "assembly": maps.assembly.forward,
        "workshop_id": maps
            .workshop_id
            .iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect::<BTreeMap<_, _>>(),
    });
    let mapping_bytes = serde_json::to_vec_pretty(&mapping).unwrap_or_else(|e| {
        eprintln!("failed to serialize mapping: {e}");
        std::process::exit(1);
    });
    if let Err(e) = std::fs::write(&output_mapping, mapping_bytes) {
        eprintln!("failed to write {output_mapping}: {e}");
        std::process::exit(1);
    }

    println!(
        "anonymized {} mod ids, {} names, {} authors -> {output_report} (mapping: {output_mapping})",
        maps.mod_id.forward.len(),
        maps.name.forward.len(),
        maps.author.forward.len(),
    );
}
