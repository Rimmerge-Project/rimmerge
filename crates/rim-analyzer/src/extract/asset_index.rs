//! Pure parsing for the two byte sources `infra::mod_scan`'s
//! `AssetBundles/` walk and the Core resource scan feed into the
//! non-loose texture index (`analysis::indices::Indices
//! ::non_loose_textures`): a Unity `.manifest` sidecar's own `Assets:`
//! list, and a bounded scan of `RimWorldWin64_Data/globalgamemanagers` for
//! `textures/...` container-path strings. Ground-truthed against
//! `Verse.ContentFinder<T>.TryFindAssetInModBundles`/
//! `ModAssetBundlesHandler`.

use std::path::Path;

/// Extension-independent, case-insensitive, `/`-separated normalization —
/// the same convention [`crate::extract::textures::normalize`] applies to a
/// loose file's own relative path.
fn normalize_key(path: &str) -> String {
    let without_extension = Path::new(path).with_extension("");
    without_extension
        .to_string_lossy()
        .replace('\\', "/")
        .to_lowercase()
}

/// Every entry under a Unity `.manifest` sidecar's `Assets:` list, in
/// document order, each with its leading `- ` stripped and trimmed —
/// **not** a YAML parse, just the one shape this file ever has:
///
/// ```text
/// Assets:
/// - Assets/Data/Core/Textures/Foo.png
/// - Assets/Data/Core/Textures/Bar.png
/// Dependencies: {}
/// ```
///
/// Returns an empty list for text with no `Assets:` header at all, or one
/// whose list is empty. Stops collecting once a line at or past the
/// `Assets:` header's own indent no longer starts with `-` (the next
/// top-level key), so `Dependencies:`/anything after is never mistaken for
/// another asset entry.
#[must_use]
pub fn parse_bundle_manifest(text: &str) -> Vec<String> {
    let mut in_assets = false;
    let mut entries = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if !in_assets {
            if trimmed == "Assets:" {
                in_assets = true;
            }
            continue;
        }
        match trimmed.strip_prefix('-') {
            Some(rest) => entries.push(rest.trim().to_string()),
            None => break,
        }
    }
    entries
}

/// Whether a bundle file `name` (as read directly out of `AssetBundles/`,
/// never a `.manifest` sidecar) loads on Windows — `ModAssetBundlesHandler
/// .GetBundleNameWithoutOsSpecifier`'s own rule: no OS suffix, or `_win`,
/// loads everywhere/on Windows; `_mac`/`_linux` never does. Case-insensitive,
/// matching the engine's own comparison.
#[must_use]
pub fn is_os_eligible_bundle_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    !(lower.ends_with("_mac") || lower.ends_with("_linux"))
}

/// The normalized, extension-stripped texture key `entry` (one line out of
/// [`parse_bundle_manifest`], e.g. `Assets/Data/Core/Textures/Foo.png`)
/// resolves to under this mod's own bundle prefix — `Verse
/// .ContentFinder<T>.TryFindAssetInModBundles`'s own two accepted prefixes:
/// `Assets/Data/<folder_name>/Textures/...` always, and
/// `Assets/Data/<package_id>/Textures/...` as a second, package-id-keyed
/// path for a non-official mod. `package_id` must be `None` for Core and
/// every DLC — the game never accepts a DLC's own `packageId` as this
/// second prefix. Both prefixes are matched case-insensitively (Unity's own
/// bundle-container path strings are already lowercase; `folder_name`/
/// `package_id` are lowercased here so a caller doesn't have to). Returns
/// `None` when `entry` matches neither prefix, or when the prefix strips
/// down to nothing (`.../Textures/` naming no file at all).
#[must_use]
pub fn bundle_asset_key(
    entry: &str,
    folder_name: &str,
    package_id: Option<&str>,
) -> Option<String> {
    let lower = entry.to_lowercase();
    let folder_prefix = format!("assets/data/{}/textures/", folder_name.to_lowercase());
    let rest = match lower.strip_prefix(folder_prefix.as_str()) {
        Some(rest) => rest,
        None => {
            let package_prefix = format!("assets/data/{}/textures/", package_id?.to_lowercase());
            lower.strip_prefix(package_prefix.as_str())?
        }
    };
    if rest.is_empty() {
        return None;
    }
    let key = normalize_key(rest);
    if key.is_empty() { None } else { Some(key) }
}

/// The character set a `globalgamemanagers` container-path run may
/// contain — Unity's own serialized asset path strings are lowercase
/// ASCII, `/`-separated, with the ordinary filename punctuation a texture
/// path can carry.
fn is_run_byte(byte: u8) -> bool {
    byte.is_ascii_lowercase()
        || byte.is_ascii_digit()
        || matches!(byte, b'_' | b'/' | b' ' | b'.' | b'(' | b')' | b'-')
}

/// The longest a single [`resource_container_paths`] candidate run is ever
/// allowed to grow before it's cut — a defensive bound against a large
/// stretch of the binary happening to fall inside `is_run_byte`'s
/// charset and turning an O(bytes) scan into an unbounded allocation.
const MAX_RUN_BYTES: usize = 512;

/// Every distinct container-path string in `bytes` (a raw
/// `globalgamemanagers` read, or any other buffer for the same bounded-scan
/// shape) that starts with `prefix` — a bounded byte scan, not a real
/// asset-bundle/serialized-file parse: it finds every maximal run of
/// `is_run_byte` bytes (capped at `MAX_RUN_BYTES` each) and keeps the
/// ones beginning with `prefix`. `prefix` itself must already satisfy
/// `is_run_byte` for every one of its own bytes, or nothing can ever
/// match it.
#[must_use]
pub fn resource_container_paths(bytes: &[u8], prefix: &str) -> std::collections::BTreeSet<String> {
    let mut found = std::collections::BTreeSet::new();
    let flush = |start: usize, end: usize, found: &mut std::collections::BTreeSet<String>| {
        if end <= start {
            return;
        }
        // Every byte in `bytes[start..end]` passed `is_run_byte`, which
        // only ever admits ASCII, so this is always valid UTF-8 — no
        // lossy conversion needed.
        let run = std::str::from_utf8(&bytes[start..end]).unwrap_or_default();
        if run.starts_with(prefix) {
            found.insert(run.to_string());
        }
    };

    let mut run_start: Option<usize> = None;
    for (index, &byte) in bytes.iter().enumerate() {
        if is_run_byte(byte) {
            let start = *run_start.get_or_insert(index);
            // A run at the cap is flushed and immediately restarted at
            // the next byte — bounding both memory and, for a candidate
            // that legitimately matches `prefix`, the entry's own length.
            // A restarted run's own content is checked against `prefix`
            // like any other; it only ever matches by coincidence, which
            // is harmless (see this function's own doc comment).
            if index - start + 1 >= MAX_RUN_BYTES {
                flush(start, index + 1, &mut found);
                run_start = None;
            }
        } else if let Some(start) = run_start.take() {
            flush(start, index, &mut found);
        }
    }
    if let Some(start) = run_start {
        flush(start, bytes.len(), &mut found);
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn parses_every_asset_line_after_the_assets_header() {
        let text = "ManifestFileVersion: 0\nAssets:\n- Assets/Data/Core/Textures/Foo.png\n- Assets/Data/Core/Textures/Bar.dds\nDependencies: {}\n";
        assert_eq!(
            parse_bundle_manifest(text),
            vec![
                "Assets/Data/Core/Textures/Foo.png".to_string(),
                "Assets/Data/Core/Textures/Bar.dds".to_string(),
            ]
        );
    }

    #[test]
    fn ignores_lines_before_the_assets_header() {
        let text =
            "- Assets/Not/Really/An/Asset.png\nAssets:\n- Assets/Data/Core/Textures/Foo.png\n";
        assert_eq!(
            parse_bundle_manifest(text),
            vec!["Assets/Data/Core/Textures/Foo.png".to_string()]
        );
    }

    #[test]
    fn no_assets_header_yields_nothing() {
        assert!(parse_bundle_manifest("ManifestFileVersion: 0\nCRC: 123\n").is_empty());
    }

    #[test]
    fn bundle_key_strips_folder_prefix_and_extension_case_insensitively() {
        let key = bundle_asset_key("Assets/Data/CORE/Textures/Things/Wall.png", "Core", None);
        assert_eq!(key, Some("things/wall".to_string()));
    }

    #[test]
    fn bundle_key_rejects_an_entry_for_another_folder_name() {
        let key = bundle_asset_key("Assets/Data/Biotech/Textures/Things/Wall.png", "Core", None);
        assert_eq!(key, None);
    }

    #[test]
    fn bundle_key_accepts_package_id_prefix_only_for_non_official_mods() {
        let via_package = bundle_asset_key(
            "Assets/Data/example.mod/Textures/Things/Wall.png",
            "SomeFolderName",
            Some("example.mod"),
        );
        assert_eq!(via_package, Some("things/wall".to_string()));

        let vanilla_has_no_package_fallback = bundle_asset_key(
            "Assets/Data/ludeon.rimworld/Textures/Things/Wall.png",
            "Core",
            None,
        );
        assert_eq!(vanilla_has_no_package_fallback, None);
    }

    #[test]
    fn bundle_key_rejects_a_prefix_naming_no_file() {
        assert_eq!(
            bundle_asset_key("Assets/Data/Core/Textures/", "Core", None),
            None
        );
    }

    #[test]
    fn os_eligible_accepts_no_suffix_and_win_rejects_mac_and_linux() {
        assert!(is_os_eligible_bundle_name("things"));
        assert!(is_os_eligible_bundle_name("things_win"));
        assert!(!is_os_eligible_bundle_name("things_mac"));
        assert!(!is_os_eligible_bundle_name("things_linux"));
        assert!(!is_os_eligible_bundle_name("Things_MAC"));
    }

    #[test]
    fn container_scan_finds_texture_paths_and_ignores_binary_noise() {
        let mut bytes = vec![0xFFu8, 0x00, 0x01, 0x02];
        bytes.extend_from_slice(b"textures/things/wall");
        bytes.push(0x00);
        bytes.extend_from_slice(b"unrelated/other/path");
        bytes.push(0xFF);
        bytes.extend_from_slice(b"textures/things/door");

        let found = resource_container_paths(&bytes, "textures/");
        assert_eq!(
            found,
            BTreeSet::from([
                "textures/things/wall".to_string(),
                "textures/things/door".to_string(),
            ])
        );
    }

    #[test]
    fn container_scan_bounds_run_length() {
        // A single run of 1000 valid bytes never grows past
        // `MAX_RUN_BYTES` in any one returned entry, and never panics or
        // hangs.
        let mut bytes = b"textures/".to_vec();
        bytes.extend(std::iter::repeat_n(b'a', 1000));
        let found = resource_container_paths(&bytes, "textures/");
        assert!(found.iter().all(|s| s.len() <= MAX_RUN_BYTES));
        assert!(!found.is_empty());
    }

    #[test]
    fn container_scan_prefix_not_at_run_start_does_not_match() {
        let bytes = b"nottextures/things/wall".to_vec();
        assert!(resource_container_paths(&bytes, "textures/").is_empty());
    }
}
