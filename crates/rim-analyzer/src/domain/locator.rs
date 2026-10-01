//! [`XmlLocator`]: where one XML element lives, so a scan result can point
//! back at a def, template, or patch op without embedding a copy of its
//! source text.

use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// The file and the path of element-child ordinals from the document root
/// element down to one element. `[3]` is the 4th element child of
/// `<Defs>`; `[0, 2, 1]` is `<Patch>` -> 1st `<Operation>` -> its
/// `<operations>` -> 2nd `<li>`.
///
/// Ordinal paths, not byte offsets: `decode_lossy`'s BOM-strip and lossy
/// UTF-8 replacement mean byte offsets in the decoded text don't map back
/// to the file, while ordinals survive a re-read of an unchanged file
/// exactly (a replaced byte changes a text node's *content*, never an
/// element's position among its parent's children). A reader is expected
/// to validate the located element's tag and `defName`/`Name` against
/// what it expected to find there, and report staleness rather than
/// silently mis-reading when a file has changed since the scan that
/// produced this locator.
///
/// `Serialize`/`Deserialize` are hand-written, not derived: `Arc<Path>`
/// has no such impl to derive one from (`Path` isn't `Sized`, so serde's
/// blanket `Arc<T>` impl doesn't apply). Used only where a finding
/// genuinely needs to point a user at a file and line — `BrokenInheritance`
/// and `NearMissModReference`'s own evidence, and `UnresolvedFindMod.locator` —
/// never on `DefEntry`/`TemplateEntry`/`PatchOp` themselves, which stay
/// unserialized.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct XmlLocator {
    /// Shared per file: tens of thousands of defs in one install must not
    /// each clone the file path.
    pub file: Arc<Path>,
    /// The ordinal path itself — see the type's own doc comment.
    pub element_path: Vec<u32>,
}

impl Serialize for XmlLocator {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Repr<'a> {
            file: &'a Path,
            element_path: &'a [u32],
        }
        Repr {
            file: &self.file,
            element_path: &self.element_path,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for XmlLocator {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Repr {
            file: std::path::PathBuf,
            element_path: Vec<u32>,
        }
        let repr = Repr::deserialize(deserializer)?;
        Ok(XmlLocator {
            file: Arc::from(repr.file),
            element_path: repr.element_path,
        })
    }
}

impl XmlLocator {
    #[must_use]
    pub fn new(file: Arc<Path>, element_path: Vec<u32>) -> Self {
        Self { file, element_path }
    }
}

#[cfg(test)]
impl XmlLocator {
    /// A locator good enough for tests that only need *a* value, not a
    /// specific one — a stand-in file path an arbitrary element path,
    /// shared across the crate's unit tests so every `DefEntry`/`PatchOp`
    /// test fixture doesn't hand-roll its own.
    pub(crate) fn for_test() -> Self {
        Self {
            file: Arc::from(Path::new("test.xml")),
            element_path: vec![0],
        }
    }
}
