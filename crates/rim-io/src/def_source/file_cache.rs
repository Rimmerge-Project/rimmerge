//! [`FileCache`]: [`super::FileDefSourceReader`]'s bounded,
//! least-recently-used cache of parsed files, keyed by path and checked
//! against each file's current [`FileStamp`] on every lookup.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use super::{FileStamp, ParsedFile};

/// How many bytes of parsed files the cache keeps at once, counted by
/// [`ParsedFile::footprint_bytes`] plus each entry's own bookkeeping.
///
/// Sized from a real install of about 1,000 active mods: one verify makes
/// about 46,500 reads into about 6,900 distinct files holding 51 MB of
/// text (a `Patches/*.xml` file is read once per operation it holds). The
/// reads into one file come close together, so 16 MiB already parses
/// only about 6% more files than a budget holding all of them (64 MiB),
/// and verify, merge render and the def inspector measured the same at
/// 16, 32 and 64 MiB. The desktop app keeps one reader for its lifetime,
/// so the smallest of those is what it holds on to.
pub(super) const CACHE_BUDGET_BYTES: usize = 16 * 1024 * 1024;

/// Bytes charged per entry on top of the file itself: its path, the two
/// map nodes that track it, and the `Arc` header.
pub(super) fn entry_overhead(path: &Path) -> usize {
    path.as_os_str().len() + 2 * size_of::<(Arc<Path>, Slot)>() + 2 * size_of::<usize>()
}

/// One cached file, with the recency tick it was last used at and the
/// bytes it was charged when inserted (subtracted exactly on removal).
#[derive(Debug)]
struct Slot {
    file: Arc<ParsedFile>,
    last_used: u64,
    charged_bytes: usize,
}

/// Everything behind the lock: the entries, their recency order (oldest
/// tick first), and the bytes charged so far, so the three can never
/// drift apart.
#[derive(Debug, Default)]
struct CacheState {
    entries: BTreeMap<Arc<Path>, Slot>,
    recency: BTreeMap<u64, Arc<Path>>,
    next_tick: u64,
    used_bytes: usize,
}

impl CacheState {
    fn tick(&mut self) -> u64 {
        let tick = self.next_tick;
        self.next_tick += 1;
        tick
    }

    fn remove(&mut self, path: &Path) {
        if let Some(slot) = self.entries.remove(path) {
            self.recency.remove(&slot.last_used);
            self.used_bytes -= slot.charged_bytes;
        }
    }

    /// Evicts the least recently used entry; `false` when there was none.
    fn evict_oldest(&mut self) -> bool {
        let Some((_, path)) = self.recency.pop_first() else {
            return false;
        };
        if let Some(slot) = self.entries.remove(&path) {
            self.used_bytes -= slot.charged_bytes;
        }
        true
    }
}

/// The cache itself. The lock is held only for map operations, never
/// across reading, decoding or parsing a file, so threads replaying defs
/// at once never wait on each other's parse. Two threads missing the same
/// file at the same moment both parse it; the second insert replaces the
/// first, which is identical.
#[derive(Debug)]
pub(super) struct FileCache {
    state: Mutex<CacheState>,
    budget_bytes: usize,
}

impl Default for FileCache {
    fn default() -> Self {
        Self::with_budget(CACHE_BUDGET_BYTES)
    }
}

impl FileCache {
    /// An empty cache that keeps at most `budget_bytes` of parsed files.
    pub(super) fn with_budget(budget_bytes: usize) -> Self {
        Self {
            state: Mutex::default(),
            budget_bytes,
        }
    }

    fn lock(&self) -> MutexGuard<'_, CacheState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The cached parse of `path`, if there is one and it was made from a
    /// file with exactly `stamp`; marks it most recently used. An entry
    /// whose stamp no longer matches is dropped, since the file changed
    /// under it.
    pub(super) fn get(&self, path: &Path, stamp: FileStamp) -> Option<Arc<ParsedFile>> {
        let mut state = self.lock();
        let tick = state.tick();
        let state = &mut *state;
        let slot = state.entries.get_mut(path)?;
        if slot.file.stamp != stamp {
            state.remove(path);
            return None;
        }
        if let Some(key) = state.recency.remove(&slot.last_used) {
            state.recency.insert(tick, key);
        }
        slot.last_used = tick;
        Some(Arc::clone(&slot.file))
    }

    /// Records `file` for `path` as most recently used, evicting the least
    /// recently used entries until it fits the budget. A file that alone
    /// exceeds the budget is not kept at all, so every read of it pays the
    /// full read, decode, nesting scan and parse again (the cache this
    /// replaced kept such a file's text and re-parsed only).
    pub(super) fn insert(&self, path: Arc<Path>, file: Arc<ParsedFile>) {
        let charged_bytes = file.footprint_bytes() + entry_overhead(&path);
        if charged_bytes > self.budget_bytes {
            return;
        }
        let mut state = self.lock();
        state.remove(&path);
        while state.used_bytes + charged_bytes > self.budget_bytes && state.evict_oldest() {}
        let last_used = state.tick();
        state.recency.insert(last_used, Arc::clone(&path));
        state.entries.insert(
            path,
            Slot {
                file,
                last_used,
                charged_bytes,
            },
        );
        state.used_bytes += charged_bytes;
    }

    /// Bytes currently charged against the budget.
    #[cfg(test)]
    pub(super) fn used_bytes(&self) -> usize {
        self.lock().used_bytes
    }

    /// Whether `path` has an entry, whatever its stamp, without touching
    /// its recency.
    #[cfg(test)]
    pub(super) fn contains(&self, path: &Path) -> bool {
        self.lock().entries.contains_key(path)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime};

    use super::*;

    fn stamp(len: u64) -> FileStamp {
        FileStamp {
            len,
            modified: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(len)),
        }
    }

    fn parsed(text: &str) -> Arc<ParsedFile> {
        let len = u64::try_from(text.len()).expect("test text fits u64");
        Arc::new(ParsedFile::new(stamp(len), text.to_string()))
    }

    fn path(name: &str) -> Arc<Path> {
        Arc::from(PathBuf::from(name))
    }

    fn charge(name: &str, file: &ParsedFile) -> usize {
        file.footprint_bytes() + entry_overhead(&path(name))
    }

    const DOC: &str = "<Defs><ThingDef><defName>A</defName></ThingDef></Defs>";

    #[test]
    fn a_lookup_with_the_inserted_stamp_returns_the_same_parse() {
        let cache = FileCache::default();
        let file = parsed(DOC);
        cache.insert(path("a.xml"), Arc::clone(&file));

        let hit = cache.get(&path("a.xml"), file.stamp);

        assert!(hit.is_some_and(|hit| Arc::ptr_eq(&hit, &file)));
    }

    #[test]
    fn a_lookup_with_a_different_stamp_misses_and_drops_the_entry() {
        let cache = FileCache::default();
        let file = parsed(DOC);
        cache.insert(path("a.xml"), Arc::clone(&file));

        let changed = FileStamp {
            len: file.stamp.len,
            modified: None,
        };
        let miss = cache.get(&path("a.xml"), changed);

        assert!(miss.is_none());
        assert!(!cache.contains(&path("a.xml")));
        assert_eq!(cache.used_bytes(), 0);
    }

    #[test]
    fn the_least_recently_used_entry_is_evicted_when_the_budget_is_full() {
        let (a, b, c) = (parsed(DOC), parsed(DOC), parsed(DOC));
        let budget = charge("a.xml", &a) + charge("b.xml", &b) + charge("c.xml", &c) - 1;
        let cache = FileCache::with_budget(budget);
        cache.insert(path("a.xml"), Arc::clone(&a));
        cache.insert(path("b.xml"), Arc::clone(&b));
        // Using `a` makes `b` the oldest.
        assert!(cache.get(&path("a.xml"), a.stamp).is_some());

        cache.insert(path("c.xml"), Arc::clone(&c));

        assert!(cache.contains(&path("a.xml")));
        assert!(!cache.contains(&path("b.xml")));
        assert!(cache.contains(&path("c.xml")));
        assert!(cache.used_bytes() <= budget);
    }

    #[test]
    fn re_inserting_a_path_replaces_its_entry_without_charging_it_twice() {
        let a = parsed(DOC);
        let b = parsed(DOC);
        let budget = charge("a.xml", &a) + charge("b.xml", &b);
        let cache = FileCache::with_budget(budget);
        cache.insert(path("a.xml"), Arc::clone(&a));
        cache.insert(path("b.xml"), Arc::clone(&b));
        // `b` is now the oldest, so charging `a` twice would evict it.
        assert!(cache.get(&path("a.xml"), a.stamp).is_some());

        let replacement = parsed(DOC);
        cache.insert(path("a.xml"), Arc::clone(&replacement));

        assert_eq!(cache.used_bytes(), budget);
        assert!(
            cache.contains(&path("b.xml")),
            "replacing a must not evict b"
        );
        assert!(
            cache
                .get(&path("a.xml"), replacement.stamp)
                .is_some_and(|hit| Arc::ptr_eq(&hit, &replacement))
        );
    }

    #[test]
    fn a_file_larger_than_the_whole_budget_is_not_kept() {
        let small = parsed(DOC);
        let cache = FileCache::with_budget(charge("small.xml", &small));
        cache.insert(path("small.xml"), Arc::clone(&small));

        let large = parsed(&DOC.repeat(4));
        cache.insert(path("large.xml"), large);

        assert!(!cache.contains(&path("large.xml")));
        assert!(
            cache.contains(&path("small.xml")),
            "an uncacheable file must not evict anything"
        );
    }
}
