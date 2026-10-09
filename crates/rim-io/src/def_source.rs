//! [`FileDefSourceReader`]: reads one XML element's text back by
//! [`XmlLocator`], from the real file on disk — the [`DefSourceReader`]
//! port's production adapter.

use std::borrow::Cow;
use std::fs;
use std::ops::Range;
use std::path::Path;
use std::sync::Arc;
use std::time::SystemTime;

use rim_analyzer::domain::XmlLocator;
use rim_analyzer::extract::{MAX_RAW_ELEMENT_DEPTH, raw_element_nesting_exceeds};
use rim_session::ports::{DefSourceError, DefSourceReader, ElementExpectation};
use roxmltree::Node;

use file_cache::FileCache;

mod file_cache;

/// Files larger than this are refused rather than read. Deliberately a
/// separate constant rather than a shared import:
/// `rim_analyzer::infra::mod_scan::MAX_READ_BYTES` (the same 256 MiB
/// value) is `pub(crate)` to that crate, so this is a matching copy, not
/// a reused one.
const MAX_READ_BYTES: u64 = 256 * 1024 * 1024;

/// Strips a leading UTF-8 BOM and lossily decodes the rest — a local
/// equivalent of `rim_analyzer::extract::xml_util::decode_lossy`, which is
/// `pub(crate)` to that crate's own `extract` module and so not reachable
/// from here.
fn decode_lossy(bytes: &[u8]) -> String {
    const UTF8_BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];
    let bytes = bytes.strip_prefix(&UTF8_BOM).unwrap_or(bytes);
    String::from_utf8_lossy(bytes).into_owned()
}

/// A file's length and modification time as of one read: what tells
/// whether a cached parse of it is still good.
///
/// `SystemTime` support is platform-dependent (and Windows' own
/// resolution is coarse); when it's unavailable, `len` alone is still
/// checked, and `None == None` never falsely matches an actual edit that
/// also changed the file's length.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileStamp {
    len: u64,
    modified: Option<SystemTime>,
}

/// What a read checks an element against, taken from the element once
/// when its file is parsed: its tag, its trimmed `defName` child text and
/// its `Name` attribute, plus where its own source text sits in the file.
#[derive(Debug, Clone)]
struct ElementSummary {
    range: Range<usize>,
    tag: Box<str>,
    def_name: Option<Box<str>>,
    name_attr: Option<Box<str>>,
}

impl ElementSummary {
    fn of(node: Node) -> Self {
        let def_name = node
            .children()
            .find(|child| child.is_element() && child.tag_name().name() == "defName")
            .and_then(|child| child.text())
            .map(|text| Box::from(text.trim()));
        Self {
            range: node.range(),
            tag: Box::from(node.tag_name().name()),
            def_name,
            name_attr: node.attribute("Name").map(Box::from),
        }
    }

    fn matches(&self, expected: &ElementExpectation) -> bool {
        if *self.tag != *expected.tag {
            return false;
        }
        if let Some(want) = &expected.def_name
            && self.def_name.as_deref() != Some(want.as_str())
        {
            return false;
        }
        if let Some(want) = &expected.name_attr
            && self.name_attr.as_deref() != Some(want.as_str())
        {
            return false;
        }
        true
    }

    /// Heap and inline bytes this summary holds.
    fn footprint_bytes(&self) -> usize {
        size_of::<Self>()
            + self.tag.len()
            + self.def_name.as_ref().map_or(0, |text| text.len())
            + self.name_attr.as_ref().map_or(0, |text| text.len())
    }
}

/// Why a file has no element outline: the message a
/// [`DefSourceError::Xml`] for it carries.
type OutlineFailure = String;

/// One decoded file plus the summaries of its root element's element
/// children, made by a single parse. Every [`XmlLocator`] `rim-session`
/// reads names one of those children (a def under `<Defs>`; patch
/// operations are read at their top-level `<Operation>`, see
/// `def_sources::top_level_operations`), so such a read is answered from
/// the outline. The analyzer also emits deeper locators (an operation
/// nested in a sequence); any such path parses the text again.
#[derive(Debug)]
struct ParsedFile {
    stamp: FileStamp,
    text: String,
    outline: Result<Box<[ElementSummary]>, OutlineFailure>,
}

impl ParsedFile {
    fn new(stamp: FileStamp, text: String) -> Self {
        let outline = outline_of(&text);
        Self {
            stamp,
            text,
            outline,
        }
    }

    /// Bytes this parse holds, as charged against the cache's budget.
    fn footprint_bytes(&self) -> usize {
        let outline_bytes = match &self.outline {
            Ok(children) => children.iter().map(ElementSummary::footprint_bytes).sum(),
            Err(message) => message.len(),
        };
        size_of::<Self>() + self.text.len() + outline_bytes
    }

    /// The element at `path` (element-child ordinals from the root
    /// element), or `None` when the path runs off the document.
    fn element_at(&self, path: &[u32]) -> Result<Option<Cow<'_, ElementSummary>>, OutlineFailure> {
        let children = self.outline.as_ref().map_err(Clone::clone)?;
        if let [ordinal] = path {
            return Ok(children.get(*ordinal as usize).map(Cow::Borrowed));
        }
        let doc = roxmltree::Document::parse(&self.text).map_err(|error| error.to_string())?;
        let mut node = doc.root_element();
        for &ordinal in path {
            let Some(child) = node
                .children()
                .filter(Node::is_element)
                .nth(ordinal as usize)
            else {
                return Ok(None);
            };
            node = child;
        }
        Ok(Some(Cow::Owned(ElementSummary::of(node))))
    }
}

/// Parses `text` once and summarizes its root element's element
/// children.
fn outline_of(text: &str) -> Result<Box<[ElementSummary]>, OutlineFailure> {
    // `roxmltree` recurses over element nesting and can overflow the
    // stack before any of its own limits apply, so a pathologically
    // deep file is refused by a linear scan first.
    if raw_element_nesting_exceeds(text, MAX_RAW_ELEMENT_DEPTH) {
        return Err(format!(
            "raw element nesting exceeds {MAX_RAW_ELEMENT_DEPTH} levels; refusing to parse"
        ));
    }
    let doc = roxmltree::Document::parse(text).map_err(|error| error.to_string())?;
    Ok(doc
        .root_element()
        .children()
        .filter(Node::is_element)
        .map(ElementSummary::of)
        .collect())
}

/// Reads one XML element's text back by [`XmlLocator`]: bounded file read,
/// lossy decode, `roxmltree` parse, walk `element_path` over element
/// children, validate the result against [`ElementExpectation`], and
/// return exactly that element's own source text.
///
/// Parsed files are kept in a bounded least-recently-used cache
/// (16 MiB of text and outlines, `file_cache::CACHE_BUDGET_BYTES`), so a file
/// many reads land in is read, decoded and parsed once rather than once
/// per read. Every read first checks the file's current length and
/// modification time against what was cached, so an edit made after the
/// scan (the case `DefSourceError::Stale`'s "reload the project" remedy
/// exists for) is always seen on the next read rather than served stale
/// forever.
///
/// The cache sits behind a `Mutex`, not a `RefCell`: this reader is held
/// behind an `Arc` in the desktop composition root's `AppState` (shared
/// across Tauri commands), `tauri::State` requires its payload to be
/// `Sync`, and `rim-session`'s replay pool reads through one reader from
/// several threads at once. The lock is held only for the map lookups,
/// never across a file read or a parse.
#[derive(Debug, Default)]
pub struct FileDefSourceReader {
    cache: FileCache,
}

impl FileDefSourceReader {
    /// Builds a reader with an empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A reader whose cache keeps at most `budget_bytes`.
    #[cfg(test)]
    fn with_cache_budget(budget_bytes: usize) -> Self {
        Self {
            cache: FileCache::with_budget(budget_bytes),
        }
    }

    fn parsed_file(&self, file: &Arc<Path>) -> Result<Arc<ParsedFile>, DefSourceError> {
        let io_error = |message: String| DefSourceError::Io {
            file: file.to_path_buf(),
            message,
        };
        // `fs::metadata` first, on every read: only its length/mtime say
        // whether a previously cached parse is still good, so there is
        // no way to answer that question without touching the
        // filesystem first even when the cache turns out to hit.
        let metadata = fs::metadata(file.as_ref()).map_err(|error| io_error(error.to_string()))?;
        let stamp = FileStamp {
            len: metadata.len(),
            modified: metadata.modified().ok(),
        };
        if let Some(cached) = self.cache.get(file, stamp) {
            return Ok(cached);
        }

        if stamp.len > MAX_READ_BYTES {
            return Err(io_error(format!(
                "{} bytes exceeds the {MAX_READ_BYTES}-byte read limit",
                stamp.len
            )));
        }
        let bytes = fs::read(file.as_ref()).map_err(|error| io_error(error.to_string()))?;
        let parsed = Arc::new(ParsedFile::new(stamp, decode_lossy(&bytes)));
        self.cache.insert(Arc::clone(file), Arc::clone(&parsed));
        Ok(parsed)
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

impl DefSourceReader for FileDefSourceReader {
    fn read_element(
        &self,
        locator: &XmlLocator,
        expected: &ElementExpectation,
    ) -> Result<String, DefSourceError> {
        let parsed = self.parsed_file(&locator.file)?;
        let element = parsed
            .element_at(&locator.element_path)
            .map_err(|message| DefSourceError::Xml {
                file: locator.file.to_path_buf(),
                message,
            })?;
        match element {
            Some(element) if element.matches(expected) => {
                Ok(parsed.text[element.range.clone()].to_string())
            }
            _ => Err(stale(locator, expected)),
        }
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
    fn an_unchanged_file_is_parsed_once_and_then_served_from_the_cache() {
        let dir = tempdir().expect("tempdir");
        let locator = write_and_locate(dir.path(), "defs.xml", TWO_DEFS, 0);
        let reader = FileDefSourceReader::new();

        let first = reader
            .parsed_file(&locator.file)
            .expect("first read must succeed");
        let second = reader
            .parsed_file(&locator.file)
            .expect("second read must succeed");

        assert!(
            Arc::ptr_eq(&first, &second),
            "an unchanged file must be served from the cache, not re-parsed"
        );
    }

    const TWO_DEFS: &[u8] = b"<Defs><ThingDef><defName>A</defName></ThingDef><ThingDef><defName>B</defName></ThingDef></Defs>";

    #[test]
    fn reads_of_different_elements_of_one_cached_file_each_return_their_own_text() {
        let dir = tempdir().expect("tempdir");
        let first = write_and_locate(dir.path(), "defs.xml", TWO_DEFS, 0);
        let second = XmlLocator::new(first.file.clone(), vec![1]);
        let reader = FileDefSourceReader::new();

        let a = reader.read_element(&first, &expectation("ThingDef", "A"));
        let b = reader.read_element(&second, &expectation("ThingDef", "B"));

        assert_eq!(a.expect("A"), "<ThingDef><defName>A</defName></ThingDef>");
        assert_eq!(b.expect("B"), "<ThingDef><defName>B</defName></ThingDef>");
    }

    #[test]
    fn a_name_attribute_expectation_is_checked_against_the_element() {
        let dir = tempdir().expect("tempdir");
        let locator = write_and_locate(
            dir.path(),
            "defs.xml",
            br#"<Defs><ThingDef Name="BaseThing" Abstract="True"><label>x</label></ThingDef></Defs>"#,
            0,
        );
        let reader = FileDefSourceReader::new();
        let named = |name: &str| ElementExpectation {
            tag: "ThingDef".to_string(),
            def_name: None,
            name_attr: Some(name.to_string()),
        };

        let matching = reader.read_element(&locator, &named("BaseThing"));
        let other = reader.read_element(&locator, &named("OtherThing"));

        assert!(matching.is_ok(), "{matching:?}");
        assert!(
            matches!(other, Err(DefSourceError::Stale { .. })),
            "{other:?}"
        );
    }

    #[test]
    fn a_path_below_the_root_children_is_read_from_a_full_parse() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("patch.xml");
        fs::write(
            &path,
            br#"<Patch><Operation Class="PatchOperationSequence"><operations><li Class="PatchOperationAdd"><xpath>/Defs</xpath></li></operations></Operation></Patch>"#,
        )
        .expect("seed");
        let file: Arc<Path> = StdArc::from(path);
        let reader = FileDefSourceReader::new();
        let tag_only = |tag: &str| ElementExpectation {
            tag: tag.to_string(),
            def_name: None,
            name_attr: None,
        };

        let nested = reader.read_element(
            &XmlLocator::new(file.clone(), vec![0, 0, 0]),
            &tag_only("li"),
        );
        let root = reader.read_element(&XmlLocator::new(file.clone(), vec![]), &tag_only("Patch"));
        let past_the_end = reader.read_element(&XmlLocator::new(file, vec![0, 1]), &tag_only("li"));

        assert_eq!(
            nested.expect("nested read"),
            r#"<li Class="PatchOperationAdd"><xpath>/Defs</xpath></li>"#
        );
        assert!(root.expect("root read").starts_with("<Patch>"));
        assert!(matches!(past_the_end, Err(DefSourceError::Stale { .. })));
    }

    #[test]
    fn a_malformed_file_reports_the_same_xml_error_on_every_read() {
        let dir = tempdir().expect("tempdir");
        let locator = write_and_locate(dir.path(), "bad.xml", b"<Defs><ThingDef></Defs>", 0);
        let reader = FileDefSourceReader::new();

        let first = reader.read_element(&locator, &expectation("ThingDef", "A"));
        let second = reader.read_element(&locator, &expectation("ThingDef", "A"));

        assert!(
            matches!(&first, Err(DefSourceError::Xml { .. })),
            "{first:?}"
        );
        assert_eq!(format!("{first:?}"), format!("{second:?}"));
    }

    #[test]
    fn def_name_is_the_trimmed_text_of_a_direct_child_only() {
        let dir = tempdir().expect("tempdir");
        let locator = write_and_locate(
            dir.path(),
            "defs.xml",
            b"<Defs><ThingDef><comps><li><defName>X</defName></li></comps><defName>  A  </defName></ThingDef></Defs>",
            0,
        );
        let reader = FileDefSourceReader::new();

        let direct = reader.read_element(&locator, &expectation("ThingDef", "A"));
        let nested = reader.read_element(&locator, &expectation("ThingDef", "X"));

        assert!(direct.is_ok(), "{direct:?}");
        assert!(
            matches!(nested, Err(DefSourceError::Stale { .. })),
            "{nested:?}"
        );
    }

    #[test]
    fn a_malformed_file_fixed_on_disk_is_read_again_and_succeeds() {
        let dir = tempdir().expect("tempdir");
        let locator = write_and_locate(dir.path(), "bad.xml", b"<Defs><ThingDef></Defs>", 0);
        let reader = FileDefSourceReader::new();
        let broken = reader.read_element(&locator, &expectation("ThingDef", "A"));

        fs::write(
            locator.file.as_ref(),
            b"<Defs><ThingDef><defName>A</defName></ThingDef></Defs>",
        )
        .expect("fix the file");
        let fixed = reader.read_element(&locator, &expectation("ThingDef", "A"));

        assert!(
            matches!(broken, Err(DefSourceError::Xml { .. })),
            "{broken:?}"
        );
        assert!(fixed.is_ok(), "a fixed file must be re-read: {fixed:?}");
    }

    #[test]
    fn a_same_length_edit_with_a_new_modification_time_is_seen() {
        let dir = tempdir().expect("tempdir");
        let locator = write_and_locate(
            dir.path(),
            "defs.xml",
            b"<Defs><ThingDef><defName>A</defName></ThingDef></Defs>",
            0,
        );
        let reader = FileDefSourceReader::new();
        reader
            .read_element(&locator, &expectation("ThingDef", "A"))
            .expect("first read must succeed");

        fs::write(
            locator.file.as_ref(),
            b"<Defs><ThingDef><defName>B</defName></ThingDef></Defs>",
        )
        .expect("overwrite");
        let later = SystemTime::now() + std::time::Duration::from_secs(120);
        fs::File::options()
            .write(true)
            .open(locator.file.as_ref())
            .and_then(|handle| handle.set_modified(later))
            .expect("set mtime");
        let second = reader.read_element(&locator, &expectation("ThingDef", "B"));

        assert!(
            second.is_ok(),
            "a same-length edit must be seen: {second:?}"
        );
    }

    #[test]
    fn a_fresh_reader_reads_a_file_changed_after_another_reader_cached_it() {
        let dir = tempdir().expect("tempdir");
        let locator = write_and_locate(dir.path(), "defs.xml", TWO_DEFS, 0);
        FileDefSourceReader::new()
            .read_element(&locator, &expectation("ThingDef", "A"))
            .expect("first read must succeed");
        fs::write(
            locator.file.as_ref(),
            b"<Defs><ThingDef><defName>Edited</defName></ThingDef></Defs>",
        )
        .expect("overwrite");

        // Guards against the cache becoming process-global: a second reader
        // must never share the first one's entries.
        let fresh =
            FileDefSourceReader::new().read_element(&locator, &expectation("ThingDef", "Edited"));

        assert!(fresh.is_ok(), "{fresh:?}");
    }

    #[test]
    fn a_file_evicted_by_the_byte_budget_is_parsed_again_on_its_next_read() {
        let dir = tempdir().expect("tempdir");
        let a = write_and_locate(dir.path(), "a.xml", TWO_DEFS, 0);
        let b = write_and_locate(dir.path(), "b.xml", TWO_DEFS, 0);
        // Exactly one entry's charge: a.xml and b.xml are the same size and
        // their paths the same length.
        let one_file_budget = FileDefSourceReader::new()
            .parsed_file(&a.file)
            .expect("measure")
            .footprint_bytes()
            + file_cache::entry_overhead(&a.file);
        let reader = FileDefSourceReader::with_cache_budget(one_file_budget);

        let a_first = reader.parsed_file(&a.file).expect("a");
        let a_again = reader.parsed_file(&a.file).expect("a again");
        reader.parsed_file(&b.file).expect("b");
        let a_after_b = reader.parsed_file(&a.file).expect("a after b");

        assert!(Arc::ptr_eq(&a_first, &a_again), "a fits, so it must hit");
        assert!(
            !Arc::ptr_eq(&a_first, &a_after_b),
            "b must have evicted a from a one-file budget"
        );
    }

    #[test]
    fn concurrent_readers_of_one_reader_get_the_same_text_as_a_sequential_read() {
        let dir = tempdir().expect("tempdir");
        let files: Vec<XmlLocator> = (0..6)
            .map(|index| {
                let content: String = (0..8)
                    .map(|def| format!("<ThingDef><defName>F{index}D{def}</defName></ThingDef>"))
                    .collect();
                write_and_locate(
                    dir.path(),
                    &format!("f{index}.xml"),
                    format!("<Defs>{content}</Defs>").as_bytes(),
                    0,
                )
            })
            .collect();
        let reads: Vec<(XmlLocator, ElementExpectation)> = files
            .iter()
            .enumerate()
            .flat_map(|(index, located)| {
                (0..8u32).map(move |def| {
                    (
                        XmlLocator::new(located.file.clone(), vec![def]),
                        expectation("ThingDef", &format!("F{index}D{def}")),
                    )
                })
            })
            .collect();
        let read_all = |reader: &FileDefSourceReader, offset: usize| -> Vec<String> {
            (0..reads.len())
                .map(|step| {
                    let (locator, expected) = &reads[(step + offset) % reads.len()];
                    reader.read_element(locator, expected).expect("read")
                })
                .collect()
        };
        let sequential = read_all(&FileDefSourceReader::new(), 0);
        // Two files' worth of budget among six files and four threads, so
        // reads evict, miss and re-parse while other threads hit.
        let small_budget = 2 * FileDefSourceReader::new()
            .parsed_file(&files[0].file)
            .expect("measure")
            .footprint_bytes()
            + 2048;
        let shared = FileDefSourceReader::with_cache_budget(small_budget);

        let per_thread: Vec<Vec<String>> = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..4)
                .map(|thread| {
                    let shared = &shared;
                    let read_all = &read_all;
                    scope.spawn(move || {
                        let mut texts = read_all(shared, thread * 11);
                        let shift = (thread * 11) % texts.len();
                        texts.rotate_right(shift);
                        texts
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|handle| handle.join().expect("reader thread"))
                .collect()
        });

        assert_eq!(sequential.len(), 48);
        for texts in per_thread {
            assert_eq!(texts, sequential);
        }
    }
}
