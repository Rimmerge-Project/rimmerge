//! [`TextureIndex`]: which texture keys exist and who ships them, kept
//! after the scan so a caller can resolve a def's graphic without the
//! scan's own (transient) [`Indices`](super::indices::Indices).
//!
//! Not part of the [`Report`](crate::domain::Report): no schema change.

use std::collections::{BTreeMap, BTreeSet};

use crate::domain::ModId;
use crate::extract::graphics::TextureCatalog;

/// Every texture key the scan saw, split by whether a loose file serves it.
#[derive(Debug, Clone, Default)]
pub struct TextureIndex {
    /// Loose-file owners per normalized key, in load order — the same fold
    /// as `Indices::texture_owners`. Last-loaded wins the engine's lookup.
    loose: BTreeMap<String, Vec<ModId>>,
    /// Keys served only by an asset bundle or Core's built-in resources.
    /// Never read as an image, but the engine still finds them.
    non_loose: BTreeSet<String>,
    /// Whether the Core resource index cleared
    /// `MIN_CORE_RESOURCE_TEXTURES`. When it did not, a key found nowhere
    /// may still be a built-in texture this index cannot see.
    is_core_index_trusted: bool,
}

impl TextureIndex {
    /// Builds an index from its parts.
    #[must_use]
    pub fn new(
        loose: BTreeMap<String, Vec<ModId>>,
        non_loose: BTreeSet<String>,
        is_core_index_trusted: bool,
    ) -> Self {
        Self {
            loose,
            non_loose,
            is_core_index_trusted,
        }
    }

    /// Mods shipping a loose file at `key`, in load order (empty if none).
    #[must_use]
    pub fn loose_owners(&self, key: &str) -> &[ModId] {
        self.loose.get(key).map_or(&[], Vec::as_slice)
    }

    /// Whether `key` is served by an asset bundle or a Core resource.
    #[must_use]
    pub fn is_non_loose(&self, key: &str) -> bool {
        self.non_loose.contains(key)
    }

    /// Whether the Core resource index is large enough to trust.
    #[must_use]
    pub fn is_core_index_trusted(&self) -> bool {
        self.is_core_index_trusted
    }

    /// Number of distinct keys across both sources.
    #[must_use]
    pub fn key_count(&self) -> usize {
        self.loose.len()
            + self
                .non_loose
                .iter()
                .filter(|key| !self.loose.contains_key(*key))
                .count()
    }
}

impl TextureCatalog for TextureIndex {
    fn contains(&self, key: &str) -> bool {
        self.loose.contains_key(key) || self.non_loose.contains(key)
    }

    fn members_under(&self, folder: &str) -> Vec<&str> {
        let prefix = format!("{folder}/");
        let loose = self
            .loose
            .range(prefix.clone()..)
            .take_while(|(key, _)| key.starts_with(&prefix))
            .map(|(key, _)| key.as_str());
        let non_loose = self
            .non_loose
            .range(prefix.clone()..)
            .take_while(|key| key.starts_with(&prefix))
            .map(String::as_str);
        loose
            .chain(non_loose)
            .collect::<BTreeSet<&str>>()
            .into_iter()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index() -> TextureIndex {
        let loose = BTreeMap::from([
            ("a/one".to_owned(), vec![ModId::new("m1"), ModId::new("m2")]),
            ("a/two".to_owned(), vec![ModId::new("m2")]),
            ("b/one".to_owned(), vec![ModId::new("m1")]),
        ]);
        let non_loose = BTreeSet::from(["a/one".to_owned(), "a/three".to_owned()]);
        TextureIndex::new(loose, non_loose, true)
    }

    #[test]
    fn catalog_is_the_union_of_loose_and_non_loose_keys() {
        let index = index();
        assert!(index.contains("a/three"));
        assert!(index.contains("a/two"));
        assert!(!index.contains("a/four"));
    }

    #[test]
    fn members_under_is_sorted_deduplicated_and_scoped_to_the_folder() {
        assert_eq!(
            index().members_under("a"),
            vec!["a/one", "a/three", "a/two"]
        );
        assert!(index().members_under("c").is_empty());
    }

    #[test]
    fn loose_owners_keep_load_order() {
        let index = index();
        assert_eq!(
            index.loose_owners("a/one"),
            [ModId::new("m1"), ModId::new("m2")]
        );
        assert!(index.loose_owners("a/three").is_empty());
        assert!(index.is_non_loose("a/three"));
    }

    #[test]
    fn key_count_counts_a_key_in_both_sources_once() {
        assert_eq!(index().key_count(), 4);
    }
}
