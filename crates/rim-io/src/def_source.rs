//! [`FileDefSourceReader`]: reads one XML element's text back by
//! [`XmlLocator`], from the real file on disk — the [`DefSourceReader`]
//! port's production adapter.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use rim_analyzer::domain::XmlLocator;
use rim_analyzer::extract::{MAX_RAW_ELEMENT_DEPTH, raw_element_nesting_exceeds};
use rim_session::ports::{DefSourceError, DefSourceReader, ElementExpectation};
use roxmltree::Node;

/// Files larger than this are refused rather than read. Deliberately a
/// separate constant rather than a shared import:
/// `rim_analyzer::infra::mod_scan::MAX_READ_BYTES` (the same 256 MiB
/// value) is `pub(crate)` to that crate, so this is a matching copy, not
/// a reused one.
const MAX_READ_BYTES: u64 = 256 * 1024 * 1024;

/// How many distinct files' decoded text this reader keeps at once — a
/// merge preview reads many defs/templates out of a handful of files (one
/// `Defs/*.xml` file often holds dozens of related defs), so caching
/// avoids re-reading and re-decoding the same file for each one.
const CACHE_CAPACITY: usize = 32;

/// Strips a leading UTF-8 BOM and lossily decodes the rest — a local
/// equivalent of `rim_analyzer::extract::xml_util::decode_lossy`, which is
/// `pub(crate)` to that crate's own `extract` module and so not reachable
/// from here.
fn decode_lossy(bytes: &[u8]) -> String {
    const UTF8_BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];
    let bytes = bytes.strip_prefix(&UTF8_BOM).unwrap_or(bytes);
    String::from_utf8_lossy(bytes).into_owned()
}

/// One decoded file's cached text, plus enough of the file's metadata
/// (as of the read that produced it) to tell whether the file has since
/// changed on disk.
#[derive(Debug, Clone)]
struct CachedFile {
    text: Arc<str>,
    len: u64,
    modified: Option<SystemTime>,
}

/// The reader's LRU cache: entries plus their access order (oldest
/// first), held under one lock so the two can never drift out of sync
/// with each other.
#[derive(Debug, Default)]
struct Lru {
    map: BTreeMap<Arc<Path>, CachedFile>,
    order: Vec<Arc<Path>>,
}

impl Lru {
    /// Moves `file` to the back of [`Self::order`] (most-recently-used),
    /// if it's tracked at all.
    fn touch(&mut self, file: &Arc<Path>) {
        if let Some(position) = self.order.iter().position(|cached| cached == file) {
            let moved = self.order.remove(position);
            self.order.push(moved);
        }
    }

    /// Records `entry` for `file`, evicting the least-recently-used entry
    /// first when the cache is full and `file` isn't already in it.
    fn insert(&mut self, file: Arc<Path>, entry: CachedFile) {
        let is_new = !self.map.contains_key(&file);
        if is_new && self.map.len() >= CACHE_CAPACITY && !self.order.is_empty() {
            let oldest = self.order.remove(0);
            self.map.remove(&oldest);
        }
        self.map.insert(file.clone(), entry);
        if is_new {
            self.order.push(file);
        } else {
            self.touch(&file);
        }
    }
}

/// Reads one XML element's text back by [`XmlLocator`]: bounded file read,
/// lossy decode, `roxmltree` parse, walk `element_path` over element
/// children, validate the result against [`ElementExpectation`], and
/// return exactly that element's own source text. A small
/// least-recently-used cache of decoded file text is kept across calls
/// (see [`CACHE_CAPACITY`]) — every read first checks the file's current
/// length and modification time against what was cached, so an edit made
/// after the scan (the case `DefSourceError::Stale`'s "reload the
/// project" remedy exists for) is always seen on the next read rather
/// than served stale forever.
///
/// The cache uses `Mutex`, not `RefCell`: this reader is held behind an
/// `Arc` in the desktop composition root's `AppState` (shared across
/// Tauri commands), and `tauri::State` requires its payload to be `Sync`
/// — `RefCell` isn't. Contention is a non-issue in practice: every actual
/// call already runs inside `rim-session`'s single-writer session lock.
#[derive(Debug, Default)]
pub struct FileDefSourceReader {
    lru: Mutex<Lru>,
}

impl FileDefSourceReader {
    /// Builds a reader with an empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn read_file_text(&self, file: &Arc<Path>) -> Result<Arc<str>, DefSourceError> {
        // `fs::metadata` first, on every read: only its length/mtime say
        // whether a previously cached decode is still good, so there is
        // no way to answer that question without touching the
        // filesystem first even when the cache turns out to hit.
        let metadata = fs::metadata(file.as_ref()).map_err(|error| DefSourceError::Io {
            file: file.to_path_buf(),
            message: error.to_string(),
        })?;
        let len = metadata.len();
        // `SystemTime` support is platform-dependent (and Windows' own
        // resolution is coarse); when it's unavailable, `len` alone is
        // still checked, and `None == None` never falsely matches an
        // actual edit that also changed the file's length.
        let modified = metadata.modified().ok();

        {
            let mut lru = self
                .lru
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(cached) = lru.map.get(file)
                && cached.len == len
                && cached.modified == modified
            {
                let text = cached.text.clone();
                lru.touch(file);
                return Ok(text);
            }
        }

        if len > MAX_READ_BYTES {
            return Err(DefSourceError::Io {
                file: file.to_path_buf(),
                message: format!("{len} bytes exceeds the {MAX_READ_BYTES}-byte read limit"),
            });
        }
        let bytes = fs::read(file.as_ref()).map_err(|error| DefSourceError::Io {
            file: file.to_path_buf(),
            message: error.to_string(),
        })?;
        let text: Arc<str> = Arc::from(decode_lossy(&bytes));

        let mut lru = self
            .lru
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        lru.insert(
            file.clone(),
            CachedFile {
                text: text.clone(),
                len,
                modified,
            },
        );
        Ok(text)
    }
}

fn expectation_description(expected: &ElementExpectation) -> String {
    format!(
        "tag={} defName={:?} name={:?}",
        expected.tag, expected.def_name, expected.name_attr
    )
}

fn stale(locator: &XmlLocator, expected: &ElementExpectation) -> DefSourceError {
    DefSourceError::Stale {
        file: locator.file.to_path_buf(),
        path: locator.element_path.clone(),
        expected: expectation_description(expected),
    }
}

fn expectation_matches(node: Node, expected: &ElementExpectation) -> bool {
    if node.tag_name().name() != expected.tag {
        return false;
    }
    if let Some(want) = &expected.def_name {
        let actual = node
            .children()
            .find(|child| child.is_element() && child.tag_name().name() == "defName")
            .and_then(|child| child.text())
            .map(str::trim);
        if actual != Some(want.as_str()) {
            return false;
        }
    }
    if let Some(want) = &expected.name_attr
        && node.attribute("Name") != Some(want.as_str())
    {
        return false;
    }
    true
}

impl DefSourceReader for FileDefSourceReader {
    fn read_element(
        &self,
        locator: &XmlLocator,
        expected: &ElementExpectation,
    ) -> Result<String, DefSourceError> {
        let text = self.read_file_text(&locator.file)?;
        // `roxmltree` recurses over element nesting and can overflow the
        // stack before any of its own limits apply, so a pathologically
        // deep file is refused by a linear scan first.
        if raw_element_nesting_exceeds(&text, MAX_RAW_ELEMENT_DEPTH) {
            return Err(DefSourceError::Xml {
                file: locator.file.to_path_buf(),
                message: format!(
                    "raw element nesting exceeds {MAX_RAW_ELEMENT_DEPTH} levels; refusing to parse"
                ),
            });
        }
        let doc = roxmltree::Document::parse(&text).map_err(|error| DefSourceError::Xml {
            file: locator.file.to_path_buf(),
            message: error.to_string(),
        })?;

        let mut node = doc.root_element();
        for &ordinal in &locator.element_path {
            let Some(child) = node
                .children()
                .filter(Node::is_element)
                .nth(ordinal as usize)
            else {
                return Err(stale(locator, expected));
            };
            node = child;
        }

        if !expectation_matches(node, expected) {
            return Err(stale(locator, expected));
        }
        Ok(text[node.range()].to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc as StdArc;

    use tempfile::tempdir;

    use super::*;

    fn write_and_locate(dir: &Path, name: &str, content: &[u8], ordinal: u32) -> XmlLocator {
        let path = dir.join(name);
        fs::write(&path, content).expect("seed fixture file");
        XmlLocator::new(StdArc::from(path), vec![ordinal])
    }

    fn expectation(tag: &str, def_name: &str) -> ElementExpectation {
        ElementExpectation {
            tag: tag.to_string(),
            def_name: Some(def_name.to_string()),
            name_attr: None,
        }
    }

    #[test]
    fn reads_the_element_at_the_given_ordinal() {
        let dir = tempdir().expect("tempdir");
        let locator = write_and_locate(
            dir.path(),
            "defs.xml",
            b"<Defs><ThingDef><defName>A</defName></ThingDef><ThingDef><defName>B</defName></ThingDef></Defs>",
            1,
        );
        let reader = FileDefSourceReader::new();

        let text = reader
            .read_element(&locator, &expectation("ThingDef", "B"))
            .expect("read must succeed");

        assert!(text.contains("<defName>B</defName>"));
        assert!(!text.contains(">A<"));
    }

    /// `roxmltree` overflows a 1 MiB stack well below 100,000 nested
    /// elements; the pre-parse guard must refuse the file before it gets
    /// there (a missing guard aborts the whole test process).
    #[test]
    fn a_pathologically_deep_file_is_refused_without_overflowing_a_small_stack() {
        let dir = tempdir().expect("tempdir");
        let depth = 100_000;
        let content = format!("{}{}", "<a>".repeat(depth), "</a>".repeat(depth));
        let locator = write_and_locate(dir.path(), "deep.xml", content.as_bytes(), 0);

        let result = std::thread::Builder::new()
            .stack_size(1024 * 1024)
            .spawn(move || {
                FileDefSourceReader::new().read_element(&locator, &expectation("a", "x"))
            })
            .expect("spawn")
            .join()
            .expect("the reader must not overflow the stack");

        assert!(
            matches!(&result, Err(DefSourceError::Xml { message, .. }) if message.contains("nesting")),
            "{result:?}"
        );
    }

    #[test]
    fn strips_a_leading_bom_before_parsing() {
        let dir = tempdir().expect("tempdir");
        let mut content = vec![0xEF, 0xBB, 0xBF];
        content.extend_from_slice(b"<Defs><ThingDef><defName>A</defName></ThingDef></Defs>");
        let locator = write_and_locate(dir.path(), "bom.xml", &content, 0);
        let reader = FileDefSourceReader::new();

        let text = reader
            .read_element(&locator, &expectation("ThingDef", "A"))
            .expect("a BOM'd file must still parse");

        assert!(text.contains("<defName>A</defName>"));
    }

    #[test]
    fn handles_crlf_line_endings() {
        let dir = tempdir().expect("tempdir");
        let content =
            b"<Defs>\r\n  <ThingDef>\r\n    <defName>A</defName>\r\n  </ThingDef>\r\n</Defs>\r\n"
                .to_vec();
        let locator = write_and_locate(dir.path(), "crlf.xml", &content, 0);
        let reader = FileDefSourceReader::new();

        let text = reader
            .read_element(&locator, &expectation("ThingDef", "A"))
            .expect("CRLF line endings must still parse");

        assert!(text.contains("<defName>A</defName>"));
    }

    #[test]
    fn a_changed_def_name_at_the_same_locator_is_stale() {
        let dir = tempdir().expect("tempdir");
        let locator = write_and_locate(
            dir.path(),
            "defs.xml",
            b"<Defs><ThingDef><defName>Changed</defName></ThingDef></Defs>",
            0,
        );
        let reader = FileDefSourceReader::new();

        let result = reader.read_element(&locator, &expectation("ThingDef", "Original"));

        assert!(matches!(result, Err(DefSourceError::Stale { .. })));
    }

    #[test]
    fn an_ordinal_past_the_end_of_the_document_is_stale() {
        let dir = tempdir().expect("tempdir");
        let locator = write_and_locate(
            dir.path(),
            "defs.xml",
            b"<Defs><ThingDef><defName>A</defName></ThingDef></Defs>",
            5,
        );
        let reader = FileDefSourceReader::new();

        let result = reader.read_element(&locator, &expectation("ThingDef", "A"));

        assert!(matches!(result, Err(DefSourceError::Stale { .. })));
    }

    #[test]
    fn a_missing_file_is_an_io_error() {
        let locator = XmlLocator::new(StdArc::from(Path::new("does/not/exist.xml")), vec![0]);
        let reader = FileDefSourceReader::new();

        let result = reader.read_element(&locator, &expectation("ThingDef", "A"));

        assert!(matches!(result, Err(DefSourceError::Io { .. })));
    }

    #[test]
    fn an_on_disk_overwrite_is_seen_on_the_next_read() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("defs.xml");
        fs::write(
            &path,
            b"<Defs><ThingDef><defName>A</defName></ThingDef></Defs>",
        )
        .expect("seed");
        let reader = FileDefSourceReader::new();
        let locator = XmlLocator::new(StdArc::from(path.clone()), vec![0]);

        reader
            .read_element(&locator, &expectation("ThingDef", "A"))
            .expect("first read must succeed");
        // Change the length too, not just the content: Windows' mtime
        // resolution is coarse enough that a same-length edit within the
        // same tick could otherwise slip past the staleness check.
        fs::write(
            &path,
            b"<Defs><ThingDef><defName>Changed</defName></ThingDef></Defs>",
        )
        .expect("overwrite");

        let second = reader.read_element(&locator, &expectation("ThingDef", "Changed"));

        assert!(
            second.is_ok(),
            "an edited file must be re-read, not served stale: {second:?}"
        );
    }

    #[test]
    fn an_unchanged_file_is_served_from_the_cache() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("defs.xml");
        fs::write(
            &path,
            b"<Defs><ThingDef><defName>A</defName></ThingDef></Defs>",
        )
        .expect("seed");
        let reader = FileDefSourceReader::new();
        let file: Arc<Path> = StdArc::from(path);

        let first = reader
            .read_file_text(&file)
            .expect("first read must succeed");
        let second = reader
            .read_file_text(&file)
            .expect("second read must succeed");

        assert!(
            Arc::ptr_eq(&first, &second),
            "an unchanged file must be served from the cache, not re-read"
        );
    }
}
