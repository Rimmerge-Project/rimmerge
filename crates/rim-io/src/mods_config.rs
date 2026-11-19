//! [`ModsConfigFileStore`]: reads `ModsConfig.xml` via the analyzer's own
//! `parse_mods_config` extractor, and writes it back in RimWorld's exact
//! element shape with a timestamped backup, preserving the source file's
//! line-ending style.

use std::fs;
use std::path::{Path, PathBuf};

use rim_analyzer::extract::mods_config::parse_mods_config;
use rim_analyzer::infra::{ReadError, read_bounded_with_limit};
use rim_session::ports::{ConfigError, ConfigErrorKind, ModsConfigFile, ModsConfigStore};

use crate::atomic::write_atomically;
use crate::xml_text::xml_escape;

/// Reads and writes `ModsConfig.xml`.
#[derive(Debug, Default, Clone, Copy)]
pub struct ModsConfigFileStore;

impl ModsConfigFileStore {
    /// Builds the store. Stateless — every call re-reads/writes the path
    /// it's given.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

/// The largest `ModsConfig.xml` a read accepts. A real file is a few
/// hundred kilobytes at most (about 50 bytes per mod); the bound keeps a
/// hostile or runaway file from being held whole, and matters now that the
/// desktop re-reads the file every few seconds.
const MAX_MODS_CONFIG_BYTES: u64 = 4 * 1024 * 1024;

fn to_config_error(
    path: &Path,
    kind: ConfigErrorKind,
    error: impl std::fmt::Display,
) -> ConfigError {
    ConfigError::new(kind, format!("{}: {error}", path.display()))
}

/// An I/O failure: a missing file is its own kind, everything else is
/// unreadable.
fn io_config_error(path: &Path, error: &std::io::Error) -> ConfigError {
    let kind = if error.kind() == std::io::ErrorKind::NotFound {
        ConfigErrorKind::Missing
    } else {
        ConfigErrorKind::Unreadable
    };
    to_config_error(path, kind, error)
}

fn read_error(path: &Path, error: ReadError) -> ConfigError {
    match error {
        ReadError::Io(io) => io_config_error(path, &io),
        too_large @ ReadError::TooLarge { .. } => {
            to_config_error(path, ConfigErrorKind::Malformed, too_large)
        }
    }
}

/// The declaration line RimWorld itself writes on a brand-new
/// `ModsConfig.xml` — used only when there is no existing file (or its
/// first line isn't a declaration) to preserve one from.
const DEFAULT_DECLARATION: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>";

/// The source file's own XML declaration line, verbatim: its first
/// line, BOM-stripped and with trailing whitespace trimmed, when that
/// line starts with `<?xml` and ends with `?>`. `None` when `bytes` is
/// empty or its first line isn't a declaration, so the caller can fall
/// back to [`DEFAULT_DECLARATION`]. RimWorld and RimSort write a bare
/// `<?xml version="1.0" ?>` (no `encoding`); a hand-edited or
/// differently-sourced file may carry an `encoding` attribute or other
/// pseudo-attributes, and this preserves whatever is actually there
/// rather than assuming RimWorld's own shape.
fn declaration_of(bytes: &[u8]) -> Option<String> {
    let without_bom = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
    let first_line = without_bom
        .split(|&b| b == b'\n')
        .next()
        .unwrap_or(without_bom);
    let text = String::from_utf8_lossy(first_line);
    let trimmed = text.trim_end();
    if trimmed.starts_with("<?xml") && trimmed.ends_with("?>") {
        Some(trimmed.to_string())
    } else {
        None
    }
}

/// `"\r\n"` when `bytes` contains at least one CRLF, else `"\n"`.
fn eol_of(bytes: &[u8]) -> &'static str {
    if bytes.windows(2).any(|w| w == b"\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}

/// Renders `file` in RimWorld's exact `ModsConfigData` element shape,
/// two-space indented, using `eol` as every line's terminator and
/// `declaration` as the first line (without its own trailing `eol`).
fn render(file: &ModsConfigFile, eol: &str, declaration: &str) -> String {
    let mut out = String::new();
    out.push_str(declaration);
    out.push_str(eol);
    out.push_str("<ModsConfigData>");
    out.push_str(eol);
    out.push_str("  <version>");
    out.push_str(&xml_escape(&file.version));
    out.push_str("</version>");
    out.push_str(eol);
    out.push_str("  <activeMods>");
    out.push_str(eol);
    for id in &file.active_mods {
        out.push_str("    <li>");
        out.push_str(&xml_escape(id.as_str()));
        out.push_str("</li>");
        out.push_str(eol);
    }
    out.push_str("  </activeMods>");
    out.push_str(eol);
    out.push_str("  <knownExpansions>");
    out.push_str(eol);
    for id in &file.known_expansions {
        out.push_str("    <li>");
        out.push_str(&xml_escape(id.as_str()));
        out.push_str("</li>");
        out.push_str(eol);
    }
    out.push_str("  </knownExpansions>");
    out.push_str(eol);
    out.push_str("</ModsConfigData>");
    out.push_str(eol);
    out
}

/// `<path>.bak-<rfc3339 with colons replaced>`, next to `path`.
fn backup_path(path: &Path) -> PathBuf {
    let stamp = jiff::Timestamp::now().to_string().replace(':', "-");
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("ModsConfig.xml");
    path.with_file_name(format!("{file_name}.bak-{stamp}"))
}

impl ModsConfigStore for ModsConfigFileStore {
    fn read(&self, path: &Path) -> Result<ModsConfigFile, ConfigError> {
        let bytes = read_bounded_with_limit(path, MAX_MODS_CONFIG_BYTES)
            .map_err(|error| read_error(path, error))?;
        let document = parse_mods_config(&bytes)
            .map_err(|error| to_config_error(path, ConfigErrorKind::Malformed, error))?;
        Ok(ModsConfigFile {
            version: document.version,
            active_mods: document.active_mods,
            known_expansions: document.known_expansions,
        })
    }

    /// Copies the existing file to a timestamped backup, then writes
    /// `file` in RimWorld's exact shape, reusing the existing file's
    /// line-ending style (defaulting to CRLF, RimWorld's own convention,
    /// when there's nothing to detect it from) and its exact XML
    /// declaration line (defaulting to [`DEFAULT_DECLARATION`] when
    /// there's no existing file, or its first line isn't a declaration).
    ///
    /// What survives a write verbatim: the declaration line and the
    /// line ending. What survives in value only, not layout: `version`
    /// and `known_expansions` (round-tripped, but re-indented/
    /// re-terminated like the rest of the document). What does not
    /// survive: any element this parser doesn't know about is dropped,
    /// since `ModsConfigFile` has no field to hold it.
    ///
    /// The backup is skipped only when there is provably no existing
    /// file. Any other read failure (a permission or sharing violation,
    /// say) is returned as an error before anything is written, rather
    /// than silently treated as "no file".
    ///
    /// Both the backup copy and the new content are written atomically
    /// (temp file plus rename), so a failure partway through never
    /// leaves either file half-written.
    fn write_with_backup(
        &self,
        path: &Path,
        file: &ModsConfigFile,
    ) -> Result<PathBuf, ConfigError> {
        let existing = match fs::read(path) {
            Ok(bytes) => Some(bytes),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(io_config_error(path, &e)),
        };
        let eol = existing.as_deref().map_or("\r\n", eol_of);
        let declaration = existing
            .as_deref()
            .and_then(declaration_of)
            .unwrap_or_else(|| DEFAULT_DECLARATION.to_string());
        let backup = backup_path(path);
        if let Some(bytes) = &existing {
            write_atomically(&backup, bytes)
                .map_err(|e| to_config_error(&backup, ConfigErrorKind::Unreadable, e))?;
        }
        let rendered = render(file, eol, &declaration);
        write_atomically(path, rendered.as_bytes())
            .map_err(|e| to_config_error(path, ConfigErrorKind::Unreadable, e))?;
        Ok(backup)
    }
}

#[cfg(test)]
mod tests {
    use rim_analyzer::domain::ModId;
    use tempfile::tempdir;

    use super::*;

    const SAMPLE: &str = "<?xml version=\"1.0\" ?>\r\n<ModsConfigData>\r\n  <version>1.6.4871 rev590</version>\r\n  <activeMods>\r\n    <li>ludeon.rimworld</li>\r\n    <li>example.patchlib</li>\r\n  </activeMods>\r\n  <knownExpansions>\r\n    <li>ludeon.rimworld.royalty</li>\r\n  </knownExpansions>\r\n</ModsConfigData>\r\n";

    #[test]
    fn read_parses_version_active_mods_and_known_expansions() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("ModsConfig.xml");
        fs::write(&path, SAMPLE).expect("write fixture");

        let file = ModsConfigFileStore::new()
            .read(&path)
            .expect("read must succeed");

        assert_eq!(file.version, "1.6.4871 rev590");
        assert_eq!(
            file.active_mods,
            vec![
                ModId::new("ludeon.rimworld"),
                ModId::new("example.patchlib")
            ]
        );
        assert_eq!(
            file.known_expansions,
            vec![ModId::new("ludeon.rimworld.royalty")]
        );
    }

    fn kind_of_read(path: &Path) -> ConfigErrorKind {
        ModsConfigFileStore::new()
            .read(path)
            .expect_err("the read must fail")
            .kind()
    }

    #[test]
    fn read_reports_a_missing_file_as_missing() {
        let dir = tempdir().expect("tempdir");

        assert_eq!(
            kind_of_read(&dir.path().join("ModsConfig.xml")),
            ConfigErrorKind::Missing
        );
    }

    #[test]
    fn read_reports_a_path_that_cannot_be_read_as_unreadable() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("ModsConfig.xml");
        fs::create_dir(&path).expect("a directory where the file should be");

        assert_eq!(kind_of_read(&path), ConfigErrorKind::Unreadable);
    }

    #[test]
    fn read_reports_a_truncated_file_as_malformed() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("ModsConfig.xml");
        let cut = SAMPLE.len() / 2;
        fs::write(&path, &SAMPLE.as_bytes()[..cut]).expect("write fixture");

        assert_eq!(kind_of_read(&path), ConfigErrorKind::Malformed);
    }

    #[test]
    fn read_reports_a_file_without_an_active_mods_list_as_malformed() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("ModsConfig.xml");
        fs::write(
            &path,
            "<ModsConfigData><version>1.6</version></ModsConfigData>",
        )
        .expect("write fixture");

        assert_eq!(kind_of_read(&path), ConfigErrorKind::Malformed);
    }

    #[test]
    fn read_refuses_a_file_over_the_size_bound_without_parsing_it() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("ModsConfig.xml");
        let padding = " ".repeat(usize::try_from(MAX_MODS_CONFIG_BYTES).expect("fits"));
        fs::write(&path, format!("{SAMPLE}{padding}")).expect("write fixture");

        let error = ModsConfigFileStore::new()
            .read(&path)
            .expect_err("over the bound");

        assert_eq!(error.kind(), ConfigErrorKind::Malformed);
        assert!(error.to_string().contains("read limit"), "{error}");
    }

    #[test]
    fn read_accepts_a_file_exactly_at_the_size_bound() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("ModsConfig.xml");
        let room = usize::try_from(MAX_MODS_CONFIG_BYTES).expect("fits") - SAMPLE.len();
        fs::write(&path, format!("{SAMPLE}{}", " ".repeat(room))).expect("write fixture");

        let file = ModsConfigFileStore::new()
            .read(&path)
            .expect("at the bound is fine");

        assert_eq!(file.active_mods.len(), 2);
    }

    /// A file replaced by Rimmerge's own writer (`write_atomically`) while it
    /// is being polled reads as one complete list or fails; it is never a
    /// blend of the two, and a half-written file is never accepted. This pins
    /// only our writer: another program (RimWorld, RimSort) may write the file
    /// non-atomically, and a read that lands mid-write sees a truncated
    /// document, which is `Malformed`. The desktop's baseline refresh absorbs
    /// that as a stale status and recovers on the next good read.
    #[test]
    fn a_file_replaced_while_it_is_read_is_one_whole_list_or_a_failure() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("ModsConfig.xml");
        fs::write(&path, SAMPLE).expect("write fixture");
        let other = SAMPLE.replace("example.patchlib", "example.other");
        let writer_path = path.clone();
        let writer_content = [SAMPLE.to_string(), other.clone()];
        let writer = std::thread::spawn(move || {
            for round in 0..200 {
                // A replacement that loses a race with a reader is retried
                // by the next round; only what the reader sees matters.
                let _ = write_atomically(&writer_path, writer_content[round % 2].as_bytes());
            }
        });

        let first = vec![
            ModId::new("ludeon.rimworld"),
            ModId::new("example.patchlib"),
        ];
        let second = vec![ModId::new("ludeon.rimworld"), ModId::new("example.other")];
        for _ in 0..200 {
            match ModsConfigFileStore::new().read(&path) {
                Ok(file) => assert!(
                    file.active_mods == first || file.active_mods == second,
                    "a blend: {:?}",
                    file.active_mods
                ),
                Err(error) => assert_ne!(
                    error.kind(),
                    ConfigErrorKind::Malformed,
                    "a partial file was seen: {error}"
                ),
            }
        }
        writer.join().expect("writer thread");
    }

    #[test]
    fn write_with_backup_creates_a_backup_and_preserves_crlf() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("ModsConfig.xml");
        fs::write(&path, SAMPLE).expect("write fixture");

        let new_file = ModsConfigFile {
            version: "1.6.4871 rev590".to_string(),
            active_mods: vec![
                ModId::new("example.patchlib"),
                ModId::new("ludeon.rimworld"),
            ],
            known_expansions: vec![ModId::new("ludeon.rimworld.royalty")],
        };
        let backup = ModsConfigFileStore::new()
            .write_with_backup(&path, &new_file)
            .expect("write must succeed");

        assert!(backup.is_file(), "backup file must exist");
        let backup_contents = fs::read_to_string(&backup).expect("read backup");
        assert_eq!(
            backup_contents, SAMPLE,
            "backup must hold the pre-write content"
        );

        let written = fs::read_to_string(&path).expect("read written file");
        assert!(written.contains("\r\n"), "must preserve CRLF line endings");
        assert!(written.starts_with("<?xml version=\"1.0\" ?>\r\n"));
        assert!(written.contains("<li>example.patchlib</li>\r\n    <li>ludeon.rimworld</li>"));
        assert!(written.contains("<knownExpansions>\r\n    <li>ludeon.rimworld.royalty</li>"));
    }

    #[test]
    fn write_with_backup_skips_the_backup_when_nothing_exists_yet() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("ModsConfig.xml");
        let file = ModsConfigFile {
            version: "1.6".to_string(),
            active_mods: vec![ModId::new("a")],
            known_expansions: Vec::new(),
        };

        let backup = ModsConfigFileStore::new()
            .write_with_backup(&path, &file)
            .expect("write must succeed");

        assert!(
            !backup.is_file(),
            "no backup was made since nothing existed"
        );
        assert!(path.is_file(), "the new file must still be written");
    }

    #[test]
    fn write_with_backup_preserves_an_encoding_declaration() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("ModsConfig.xml");
        let sample_with_encoding = SAMPLE.replacen(
            "<?xml version=\"1.0\" ?>",
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>",
            1,
        );
        fs::write(&path, &sample_with_encoding).expect("write fixture");

        let file = ModsConfigFile {
            version: "1.6.4871 rev590".to_string(),
            active_mods: vec![ModId::new("ludeon.rimworld")],
            known_expansions: Vec::new(),
        };
        ModsConfigFileStore::new()
            .write_with_backup(&path, &file)
            .expect("write must succeed");

        let written = fs::read_to_string(&path).expect("read written file");
        assert!(written.starts_with("<?xml version=\"1.0\" encoding=\"utf-8\"?>\r\n"));
    }

    #[test]
    fn write_with_backup_uses_the_default_declaration_when_nothing_exists_yet() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("ModsConfig.xml");
        let file = ModsConfigFile {
            version: "1.6".to_string(),
            active_mods: vec![ModId::new("a")],
            known_expansions: Vec::new(),
        };

        ModsConfigFileStore::new()
            .write_with_backup(&path, &file)
            .expect("write must succeed");

        let written = fs::read_to_string(&path).expect("read written file");
        assert!(written.starts_with(DEFAULT_DECLARATION));
    }

    #[test]
    fn write_with_backup_preserves_a_declaration_from_a_file_with_a_bom() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("ModsConfig.xml");
        let mut with_bom = b"\xef\xbb\xbf".to_vec();
        with_bom.extend_from_slice(SAMPLE.as_bytes());
        fs::write(&path, &with_bom).expect("write fixture");

        let file = ModsConfigFile {
            version: "1.6.4871 rev590".to_string(),
            active_mods: vec![ModId::new("ludeon.rimworld")],
            known_expansions: Vec::new(),
        };
        ModsConfigFileStore::new()
            .write_with_backup(&path, &file)
            .expect("write must succeed");

        let written = fs::read_to_string(&path).expect("read written file");
        assert!(written.starts_with("<?xml version=\"1.0\" ?>\r\n"));
    }

    /// A directory occupying the target path makes `fs::read` fail with
    /// something other than `NotFound` (a "not a file"/access error,
    /// depending on the OS) — this must propagate as an error rather
    /// than being treated as "no existing file to back up", and nothing
    /// may be written.
    #[test]
    fn write_with_backup_fails_without_writing_when_a_directory_occupies_the_path() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("ModsConfig.xml");
        fs::create_dir(&path).expect("create a directory at the target path");
        let file = ModsConfigFile {
            version: "1.6".to_string(),
            active_mods: vec![ModId::new("a")],
            known_expansions: Vec::new(),
        };

        let result = ModsConfigFileStore::new().write_with_backup(&path, &file);

        assert!(result.is_err());
        assert!(path.is_dir(), "the directory must be left untouched");
        let entries: Vec<_> = fs::read_dir(dir.path())
            .expect("read tempdir")
            .filter_map(Result::ok)
            .collect();
        assert_eq!(
            entries.len(),
            1,
            "nothing (no backup, no temp file) must have been written into the tempdir"
        );
    }

    #[test]
    fn render_escapes_xml_special_characters() {
        let file = ModsConfigFile {
            version: "1.6 <beta> & \"friends\"".to_string(),
            active_mods: vec![ModId::new("a")],
            known_expansions: Vec::new(),
        };

        let rendered = render(&file, "\n", DEFAULT_DECLARATION);

        assert!(rendered.contains("1.6 &lt;beta&gt; &amp; &quot;friends&quot;"));
        assert!(!rendered.contains("<beta>"));
    }
}
