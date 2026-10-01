//! NTFS directory-entry collation, emulated — pure, no filesystem access.
//!
//! `infra::mod_scan::engine_enumeration_order` is the only caller: it
//! needs to sort one directory's own entries the way NTFS itself would
//! before walking them breadth-first, matching
//! `System.IO.Enumeration.FileSystemEnumerator<T>`, the game's own
//! directory-walk implementation. NTFS collates directory entries
//! case-insensitively, keyed by the volume's own `$UpCase` table — not in
//! any decompiled DLL, so this is disclosed as an emulation: a simple,
//! one-to-one per-code-unit uppercase fold over UTF-16, then an ordinal
//! (byte-wise) comparison, which is exactly what `.NET`'s own ordinal
//! string comparison does and a close match for NTFS on the common case.
//! It can mis-order two names differing only in a character outside the
//! simple uppercase mapping — rare, and still deterministic.

/// The sort key for one file or directory name, in NTFS collation order:
/// every `char` uppercased through Rust's own `to_uppercase`, keeping only
/// its *first* produced code point (a one-to-one fold, not Unicode's full
/// case mapping, which can expand a single character into several — German
/// `ß` into `SS`, for instance), then re-encoded as UTF-16 code units.
/// `Vec<u16>: Ord` then compares them ordinally, unit by unit, matching
/// `.NET`'s own `StringComparison.Ordinal`.
#[must_use]
pub fn ntfs_collation_key(name: &str) -> Vec<u16> {
    name.chars()
        .flat_map(|c| {
            let upper = c.to_uppercase().next().unwrap_or(c);
            let mut buf = [0u16; 2];
            upper.encode_utf16(&mut buf).to_vec()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collation_is_case_insensitive_ordinal_over_utf16() {
        assert_eq!(ntfs_collation_key("abc"), ntfs_collation_key("ABC"));
        assert!(ntfs_collation_key("Apple") < ntfs_collation_key("Banana"));
    }

    /// ASCII uppercase letters (`'A'..='Z'`, 65-90) sort below `'_'` (95)
    /// once both sides are folded to uppercase — a lowercase letter must
    /// therefore sort *before* an underscore too, not after, even though
    /// `'_'` (95) < `'a'` (97) in raw ASCII order.
    #[test]
    fn underscore_sorts_after_letters_under_uppercase_folding() {
        assert!(ntfs_collation_key("afoo") < ntfs_collation_key("_foo"));
    }

    #[test]
    fn non_ascii_letters_fold_by_simple_uppercase() {
        assert_eq!(ntfs_collation_key("café"), ntfs_collation_key("CAFÉ"));
        assert!(ntfs_collation_key("café") < ntfs_collation_key("zebra"));
    }
}
