//! Reading the install: the scanner, def-source reader, and asset locator ports.

use std::path::{Path, PathBuf};

use rim_analyzer::analysis::SourceIndex;
use rim_analyzer::domain::{GameVersion, ModId, Report, Source, XmlLocator};
use rim_analyzer::extract::about_xml::AboutDetails;
use rim_resolve::domain::TagEvidence;

use crate::ProjectPaths;

/// Which phase of [`ModScanner::scan_and_analyze`] a [`ScanProgress`]
/// reports. A closed set so an interface renders (and translates) the
/// phase itself instead of receiving prose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanProgressStage {
    /// Walking the install to find mod folders.
    Discovering,
    /// Scanning each active mod's files; ticks once per mod.
    Scanning,
    /// Post-scan work over the whole set (the analyzer's own analysis).
    Analyzing,
    /// Collecting the tag evidence the tagging rules read.
    CollectingTagEvidence,
    /// The scan has produced every artifact.
    Done,
}

impl ScanProgressStage {
    /// The English label a text interface (the CLI's progress line) prints.
    #[must_use]
    pub fn english_label(self) -> &'static str {
        match self {
            Self::Discovering => "discovering mods",
            Self::Scanning => "scanning mods",
            Self::Analyzing => "analyzing",
            Self::CollectingTagEvidence => "collecting tag evidence",
            Self::Done => "done",
        }
    }
}

/// One step of progress during [`ModScanner::scan_and_analyze`], forwarded
/// to the UI as-is by the composition root (a Tauri event, a CLI progress
/// line, ...).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanProgress {
    /// The phase this report belongs to.
    pub stage: ScanProgressStage,
    /// How many units of this stage are complete.
    pub done: usize,
    /// The total units in this stage.
    pub total: usize,
}

/// A scan/analysis failure. Adapters convert their own error types into
/// this at the port boundary; the message is already human-readable.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct ScanError(pub String);

/// What one scan yields: the analyzer's report, the tag evidence collected
/// alongside it, and where every def/template/patch op lives on disk
/// (the merge editor needs the last one to find a def's XML
/// again after the scan). A struct, not a tuple, so adding a field never
/// ripples through every caller's destructuring pattern.
#[derive(Debug, Clone)]
pub struct ScanArtifacts {
    /// The analyzer's full report.
    pub report: Report,
    /// Tag evidence collected alongside the report.
    pub evidence: Vec<TagEvidence>,
    /// Where every def, template, and mutating patch op lives on disk.
    pub sources: SourceIndex,
}

/// Scans a RimWorld install and runs the analyzer over it.
pub trait ModScanner {
    /// Scans `paths` and returns the analyzer's report, tag evidence, and
    /// source index, reporting progress through `progress` as it goes.
    ///
    /// `active_override`, when `Some`, is scanned as the active-mod list
    /// verbatim instead of `paths.mods_config`'s own `<activeMods>`
    /// — the file
    /// still must exist, but its active list is never read. Deliberately
    /// not a defaulted method: a default that silently ignored the
    /// override would be exactly the kind of bug this parameter exists to
    /// make impossible to skip implementing.
    ///
    /// # Errors
    ///
    /// Returns [`ScanError`] when the scan or analysis fails.
    fn scan_and_analyze(
        &self,
        paths: &ProjectPaths,
        active_override: Option<&[ModId]>,
        progress: &mut dyn FnMut(ScanProgress),
    ) -> Result<ScanArtifacts, ScanError>;
}

/// What one element the merge editor needs to read back is expected to be:
/// validated against the located element (tag name, and `defName`/`Name`
/// when given) so a file that changed since the scan is detected as
/// [`DefSourceError::Stale`] rather than silently mis-read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElementExpectation {
    /// The element's tag name (`ThingDef`, `Operation`, ...).
    pub tag: String,
    /// The element's `defName` child text, when the located element is a
    /// concrete def.
    pub def_name: Option<String>,
    /// The element's `Name` attribute, when the located element is a
    /// template or is itself named.
    pub name_attr: Option<String>,
}

/// A [`DefSourceReader::read_element`] or [`AssetLocator::read_texture`]
/// failure.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DefSourceError {
    /// The file couldn't be read.
    #[error("{file}: {message}")]
    Io {
        /// The file that couldn't be read.
        file: PathBuf,
        /// A human-readable message.
        message: String,
    },
    /// The file no longer contains the expected element at the locator's
    /// position — it changed since the scan that produced the locator.
    #[error("{file}: element at {path:?} is not the expected {expected}")]
    Stale {
        /// The file whose content no longer matches.
        file: PathBuf,
        /// The ordinal path that no longer resolves to the expected element.
        path: Vec<u32>,
        /// A human-readable description of what was expected there.
        expected: String,
    },
    /// The file's XML couldn't be parsed.
    #[error("{file}: {message}")]
    Xml {
        /// The file that failed to parse.
        file: PathBuf,
        /// A human-readable message.
        message: String,
    },
    /// [`AssetLocator::read_texture`] refused a file larger than
    /// [`MAX_TEXTURE_BYTES`]. Kept distinct from [`Self::Io`] (rather than
    /// folded into it the way `rim-io`'s `FileDefSourceReader` reuses `Io`
    /// for its own, much larger, read cap) because the desktop command
    /// maps this to `invalid_input` — a file this large is a bad request,
    /// not an I/O failure — while every `Io` stays `MergeSourceFailed`.
    #[error("{file}: {actual_bytes} bytes exceeds the {max_bytes}-byte texture cap")]
    TooLarge {
        /// The file that was too large to read.
        file: PathBuf,
        /// The cap that was exceeded ([`MAX_TEXTURE_BYTES`]).
        max_bytes: u64,
        /// The file's actual size.
        actual_bytes: u64,
    },
    /// [`AssetLocator::read_texture`] read a file whose leading bytes
    /// don't match a supported format ([`TextureFormat::sniff`]).
    #[error("{file}: not a supported texture format (expected PNG or JPEG)")]
    UnsupportedFormat {
        /// The file whose content didn't sniff as PNG or JPEG.
        file: PathBuf,
    },
}

/// One use-case call's view of a [`DefSourceReader`]: see
/// [`DefSourceReader::call_view`].
pub type DefSourceCallView<'a> = Box<dyn DefSourceReader + Send + Sync + 'a>;

/// Reads one element's XML text back by locator. Sync: run under
/// `spawn_blocking` by the interface layer, like every other port.
pub trait DefSourceReader {
    /// Reads the element `locator` points to, validating it matches
    /// `expected`.
    ///
    /// # Errors
    ///
    /// Returns [`DefSourceError::Io`] when the file can't be read,
    /// [`DefSourceError::Xml`] when it can't be parsed, and
    /// [`DefSourceError::Stale`] when the located element no longer
    /// matches `expected`.
    fn read_element(
        &self,
        locator: &XmlLocator,
        expected: &ElementExpectation,
    ) -> Result<String, DefSourceError>;

    /// Opens a view of this reader for one use-case call, or `None` when
    /// the reader has nothing to gain from one (the default) and the call
    /// reads through the reader itself.
    ///
    /// A view may check whether a file changed on disk only the first time
    /// it reads that file and trust that check for as long as it lives, so
    /// an edit made while the call runs may be seen only by the next call;
    /// an edit made before a view is opened is always seen through it. The
    /// use cases that read many elements (`VerifyOrder`, `RenderMergeMod`
    /// and its clean-merge prefetch, `InspectDef`) open one view per call
    /// and drop it when the call returns; a reader used without a view
    /// keeps its own rule.
    fn call_view(&self) -> Option<DefSourceCallView<'_>> {
        None
    }
}

/// Forwards to `T`'s own impl — lets a shared, cache-carrying reader (e.g.
/// `rim-io`'s `FileDefSourceReader`, which isn't `Copy`) live behind an
/// `Arc` in a composition root's adapter set while still satisfying every
/// `Reader: DefSourceReader` bound the use cases in this crate declare.
impl<T: DefSourceReader + ?Sized> DefSourceReader for std::sync::Arc<T> {
    fn read_element(
        &self,
        locator: &XmlLocator,
        expected: &ElementExpectation,
    ) -> Result<String, DefSourceError> {
        (**self).read_element(locator, expected)
    }

    fn call_view(&self) -> Option<DefSourceCallView<'_>> {
        (**self).call_view()
    }
}

/// [`std::sync::Arc<T>`]'s own blanket impl's sibling: lets a use case
/// that owns one `Reader: DefSourceReader` (e.g. `CreateAssignment`)
/// hand a *borrow* of it to another use case it composes
/// internally (`AssignmentInstances`) without cloning or wrapping it in an
/// `Arc` just to satisfy that callee's own `Reader: DefSourceReader`
/// bound.
impl<T: DefSourceReader + ?Sized> DefSourceReader for &T {
    fn read_element(
        &self,
        locator: &XmlLocator,
        expected: &ElementExpectation,
    ) -> Result<String, DefSourceError> {
        (**self).read_element(locator, expected)
    }

    fn call_view(&self) -> Option<DefSourceCallView<'_>> {
        (**self).call_view()
    }
}

/// Files larger than this are refused by [`AssetLocator::read_texture`] —
/// the texture change-summary panel only ever needs a small preview
/// image, and a multi-hundred-MB "texture" reaching an IPC command's
/// base64-encoded response would be its own kind of failure.
pub const MAX_TEXTURE_BYTES: u64 = 8 * 1024 * 1024;

/// A texture image format [`AssetLocator::read_texture`] recognizes, by
/// sniffing the file's leading magic bytes rather than trusting its
/// extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureFormat {
    /// PNG (`89 50 4E 47 ...`).
    Png,
    /// JPEG/JFIF (`FF D8 FF ...`).
    Jpeg,
}

impl TextureFormat {
    /// Sniffs `bytes`' leading magic bytes, returning `None` when they
    /// match neither PNG nor JPEG.
    #[must_use]
    pub fn sniff(bytes: &[u8]) -> Option<Self> {
        if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
            Some(Self::Png)
        } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
            Some(Self::Jpeg)
        } else {
            None
        }
    }
}

/// A texture file's raw bytes plus its sniffed format — what
/// [`AssetLocator::read_texture`] returns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextureBytes {
    /// The sniffed image format.
    pub format: TextureFormat,
    /// The file's raw bytes, unmodified.
    pub bytes: Vec<u8>,
}

/// Which `About/` image a caller wants located.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AboutImage {
    /// `About/Preview.png` — `Verse.ModMetaData.PreviewImagePath`.
    Preview,
    /// `About/ModIcon.png` — the file-based half of
    /// `Verse.ModMetaData.ModIconImagePath`. The engine's full order also
    /// tries a `<modIconPath>`-named texture key first (via
    /// `ContentFinder`, searched across the mod's own loaded folders) and
    /// falls back to a built-in default icon when neither exists; this
    /// port only ever locates the on-disk `ModIcon.png` file — the
    /// `<modIconPath>` texture-key case and the built-in default are a
    /// disclosed, narrower gap (a mod using only `<modIconPath>`, with no
    /// `ModIcon.png` file, shows no icon here), not a claim of the full
    /// engine order.
    Icon,
}

/// Finds the real on-disk file for a normalized texture key of one mod,
/// and reads a located file's bytes back.
pub trait AssetLocator {
    /// Searches `mod_loaded_folders` — priority order, highest-priority
    /// folder first (`Mod::loaded_folders`'s own contract) — for a texture
    /// file whose normalized key equals `texture_key`. A `.dds` anywhere
    /// among the folders shadows a non-`.dds` file with the same key,
    /// whatever folder either one sits in; only once no `.dds` matches
    /// does the first non-`.dds` match, in priority order, win.
    ///
    /// # Errors
    ///
    /// Returns [`DefSourceError`] when a folder can't be read.
    fn locate_texture(
        &self,
        mod_loaded_folders: &[PathBuf],
        texture_key: &str,
    ) -> Result<Option<PathBuf>, DefSourceError>;

    /// Like [`Self::locate_texture`], but only the PNG/JPEG pass: a `.dds`
    /// at the same key is ignored rather than shadowing. This is how a
    /// caller finds the image copy that sits beside a `.dds` the game
    /// loads instead (a texture optimizer leaves both). Same exact-key,
    /// priority-order matching; the key is never joined into a path.
    ///
    /// # Errors
    ///
    /// Returns [`DefSourceError`] when a folder can't be read.
    fn locate_non_dds_texture(
        &self,
        mod_loaded_folders: &[PathBuf],
        texture_key: &str,
    ) -> Result<Option<PathBuf>, DefSourceError>;

    /// Reads `path`'s bytes back, for display (the texture-override
    /// change summary shows both owners' images side by side).
    ///
    /// # Errors
    ///
    /// Returns [`DefSourceError::Io`] when `path` can't be read,
    /// [`DefSourceError::TooLarge`] when it exceeds [`MAX_TEXTURE_BYTES`],
    /// and [`DefSourceError::UnsupportedFormat`] when its content doesn't
    /// sniff as PNG or JPEG.
    fn read_texture(&self, path: &Path) -> Result<TextureBytes, DefSourceError>;

    /// Locates `image` under `mod_root`'s own `About/` folder — a plain
    /// path join plus an existence check, the same resolution
    /// `Verse.ModMetaData` itself uses (case-insensitive on Windows/NTFS
    /// for free, nothing added on top of it — see [`AboutImage`]'s own
    /// doc comment for why no case-insensitive fallback is layered on
    /// here). `None` when the file doesn't exist (or can't be stat'd); the
    /// caller (the mod info panel's `ModPreview` state) treats that as "no
    /// preview image", not an error, and a file that exists but can't be
    /// read is reported later, by [`Self::read_texture`].
    fn locate_about_image(&self, mod_root: &Path, image: AboutImage) -> Option<PathBuf>;
}

/// Forwards to `T`'s own impl — the same seam [`DefSourceReader`]'s own
/// `Arc<T>` impl offers, so a composition root can hold `asset_locator`
/// behind `Arc<dyn AssetLocator + Send + Sync>` (swappable for a test's
/// in-memory fake) while every use case in this crate still just declares
/// `Locator: AssetLocator`.
impl<T: AssetLocator + ?Sized> AssetLocator for std::sync::Arc<T> {
    fn locate_texture(
        &self,
        mod_loaded_folders: &[PathBuf],
        texture_key: &str,
    ) -> Result<Option<PathBuf>, DefSourceError> {
        (**self).locate_texture(mod_loaded_folders, texture_key)
    }

    fn locate_non_dds_texture(
        &self,
        mod_loaded_folders: &[PathBuf],
        texture_key: &str,
    ) -> Result<Option<PathBuf>, DefSourceError> {
        (**self).locate_non_dds_texture(mod_loaded_folders, texture_key)
    }

    fn read_texture(&self, path: &Path) -> Result<TextureBytes, DefSourceError> {
        (**self).read_texture(path)
    }

    fn locate_about_image(&self, mod_root: &Path, image: AboutImage) -> Option<PathBuf> {
        (**self).locate_about_image(mod_root, image)
    }
}

/// A [`ModAboutReader::read_details`] failure — kept distinct from
/// [`DefSourceError`] (a different port, a different read) rather than
/// reused: this read is small, About.xml-specific, and needs to tell "the
/// folder changed since the scan" (`NotFound`) apart from a genuine read
/// or parse failure, which `rim-session`'s `ReadModAbout` use case maps
/// to different [`crate::mod_info`] outcomes.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AboutReadError {
    /// `<mod_root>/About/About.xml` doesn't exist.
    #[error("About.xml does not exist")]
    NotFound,
    /// The file exists but couldn't be read.
    #[error("cannot read About.xml: {0}")]
    Io(String),
    /// The file was read but its XML is invalid.
    #[error("invalid About.xml: {0}")]
    Xml(String),
}

/// Reads one mod's `About.xml` details (description, mod version, icon
/// path) for the mod info panel's lazy, per-selection read. Sync, like
/// every other port — run under `spawn_blocking` by the interface layer.
pub trait ModAboutReader {
    /// Reads `mod_root`'s own `About/About.xml`, resolving
    /// `descriptionsByVersion` against `game_version`. `source` is
    /// accepted so an adapter can fold in a vanilla mod's Core/DLC-only
    /// description source (`Defs/Misc/ExpansionDefs/ExpansionDefs.xml`,
    /// under `game_dir`, which a Core/DLC's own `About.xml` never
    /// carries) without a second, separate call; `game_dir` is otherwise
    /// unused for a non-vanilla `source`.
    ///
    /// # Errors
    ///
    /// Returns [`AboutReadError::NotFound`] when the file doesn't exist,
    /// [`AboutReadError::Io`] when it exists but can't be read, and
    /// [`AboutReadError::Xml`] when it can't be parsed.
    fn read_details(
        &self,
        mod_root: &Path,
        source: Source,
        game_version: GameVersion,
        game_dir: &Path,
    ) -> Result<AboutDetails, AboutReadError>;
}

/// Forwards to `T`'s own impl — the same seam [`AssetLocator`]'s own
/// `Arc<T>` impl offers.
impl<T: ModAboutReader + ?Sized> ModAboutReader for std::sync::Arc<T> {
    fn read_details(
        &self,
        mod_root: &Path,
        source: Source,
        game_version: GameVersion,
        game_dir: &Path,
    ) -> Result<AboutDetails, AboutReadError> {
        (**self).read_details(mod_root, source, game_version, game_dir)
    }
}
