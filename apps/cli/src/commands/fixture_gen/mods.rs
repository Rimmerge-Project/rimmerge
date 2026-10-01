//! One generated mod: the deterministic RNG, the in-memory mod tree, `About.xml`, and writing it to disk.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// A tiny, dependency-free deterministic PRNG (SplitMix64) — the spec's
/// own `seed` is the only source of any pseudo-randomness this generator
/// uses, and it's used sparingly (a handful of assignment choices);
/// everything else is plain sequential indexing, which is already
/// deterministic without needing a draw at all.
pub(super) struct Rng(u64);

impl Rng {
    pub(super) fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub(super) fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

/// Where the compiled synthetic assembly blobs live
/// (`crates/rim-resolve/tests/fixtures/synthetic/assemblies/bin/`,
/// `scripts/build-synthetic-assemblies.ps1` — see that directory's own
/// `README.md`). Resolved relative to the *spec* file's own path, since
/// `fixture gen` may be invoked from any working directory but the spec
/// always lives beside these assemblies in the same checkout.
pub(super) fn assemblies_dir(spec_path: &Path) -> Result<PathBuf> {
    let spec_dir = spec_path
        .parent()
        .context("spec path has no parent directory")?;
    Ok(spec_dir.join("synthetic").join("assemblies").join("bin"))
}

/// `(language, file name, [(key, value)])` for one `Keyed/*.xml` file
/// [`GenMod::keyed`] accumulates.
type KeyedFile = (String, String, Vec<(String, String)>);

/// One generated mod's complete on-disk content, accumulated then
/// written once by [`write_mod`]/[`write_workshop_mod`].
#[derive(Default)]
pub(super) struct GenMod {
    id: String,
    folder: String,
    name: String,
    pub(super) authors: Vec<String>,
    pub(super) supported_versions: Vec<String>,
    pub(super) load_after: Vec<String>,
    pub(super) load_before: Vec<String>,
    pub(super) force_load_after: Vec<String>,
    pub(super) force_load_before: Vec<String>,
    pub(super) dependencies: Vec<(String, String)>,
    pub(super) incompatible_with: Vec<String>,
    /// `(file name, body)` under `<version>/Defs/`.
    pub(super) defs: Vec<(String, String)>,
    /// `(file name, body)` under `<version>/Patches/`.
    pub(super) patches: Vec<(String, String)>,
    /// Relative paths (no extension) under `<version>/Textures/`, each
    /// written as a one-byte placeholder file.
    pub(super) textures: Vec<String>,
    /// Relative paths (with extension) under `<version>/Sounds/`.
    pub(super) sounds: Vec<String>,
    /// Under `<version>/Languages/<language>/Keyed/`.
    pub(super) keyed: Vec<KeyedFile>,
    /// `(file name in Assemblies/, source blob path)`.
    pub(super) assemblies: Vec<(String, PathBuf)>,
    /// Verbatim `<loadFolders>...</loadFolders>` body, when this mod
    /// needs the construct exercised explicitly (most mods rely on the
    /// version-folder fallback and ship none).
    pub(super) load_folders_xml: Option<String>,
    /// `(relative path under the mod root, file contents)` written
    /// verbatim, outside the usual `1.6/` version folder -- currently
    /// only `1.6/Compat/*` for the `IfModActive`-gated `LoadFolders.xml`
    /// construct, which needs a folder that genuinely exists to resolve.
    pub(super) extra_files: Vec<(String, String)>,
}

impl GenMod {
    pub(super) fn new(
        id: impl Into<String>,
        folder: impl Into<String>,
        name: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            folder: folder.into(),
            name: name.into(),
            authors: vec!["Synthetic Author".to_string()],
            supported_versions: vec!["1.6".to_string()],
            ..Self::default()
        }
    }
}

pub(super) fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn li_list(tag: &str, values: &[String]) -> String {
    if values.is_empty() {
        return String::new();
    }
    let mut out = format!("  <{tag}>\n");
    for value in values {
        let _ = writeln!(out, "    <li>{}</li>", escape_xml(value));
    }
    let _ = writeln!(out, "  </{tag}>");
    out
}

fn about_xml(m: &GenMod) -> String {
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<ModMetaData>\n");
    let _ = writeln!(out, "  <packageId>{}</packageId>", escape_xml(&m.id));
    let _ = writeln!(out, "  <name>{}</name>", escape_xml(&m.name));
    out.push_str("  <authors>\n");
    for author in &m.authors {
        let _ = writeln!(out, "    <li>{}</li>", escape_xml(author));
    }
    out.push_str("  </authors>\n");
    out.push_str(&li_list("supportedVersions", &m.supported_versions));
    out.push_str(&li_list("loadAfter", &m.load_after));
    out.push_str(&li_list("loadBefore", &m.load_before));
    out.push_str(&li_list("forceLoadAfter", &m.force_load_after));
    out.push_str(&li_list("forceLoadBefore", &m.force_load_before));
    out.push_str(&li_list("incompatibleWith", &m.incompatible_with));
    if !m.dependencies.is_empty() {
        out.push_str("  <modDependencies>\n");
        for (id, display_name) in &m.dependencies {
            out.push_str("    <li>\n");
            let _ = writeln!(out, "      <packageId>{}</packageId>", escape_xml(id));
            let _ = writeln!(
                out,
                "      <displayName>{}</displayName>",
                escape_xml(display_name)
            );
            out.push_str("    </li>\n");
        }
        out.push_str("  </modDependencies>\n");
    }
    out.push_str("</ModMetaData>\n");
    out
}

/// Writes `m`'s complete tree directly under `mod_dir` -- the one shared
/// writer both [`write_mod`] (`<out>/game/Mods/<folder>`) and
/// [`write_workshop_mod`] (`<out>/workshop/content/294100/<id>`) call
/// -- writing the final location directly rather than copying a scratch
/// tree over.
fn write_mod_tree(mod_dir: &Path, m: &GenMod) -> Result<()> {
    let about_dir = mod_dir.join("About");
    std::fs::create_dir_all(&about_dir)?;
    std::fs::write(about_dir.join("About.xml"), about_xml(m))?;

    if let Some(load_folders) = &m.load_folders_xml {
        std::fs::write(mod_dir.join("LoadFolders.xml"), load_folders)?;
    }
    for (relative, contents) in &m.extra_files {
        let path = mod_dir.join(relative);
        std::fs::create_dir_all(path.parent().context("extra file path has no parent")?)?;
        std::fs::write(path, contents)?;
    }

    let version_dir = mod_dir.join("1.6");
    for (file_name, body) in &m.defs {
        let dir = version_dir.join("Defs");
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join(file_name), format!("<Defs>\n{body}</Defs>\n"))?;
    }
    for (file_name, body) in &m.patches {
        let dir = version_dir.join("Patches");
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join(file_name), format!("<Patch>\n{body}</Patch>\n"))?;
    }
    for relative in &m.textures {
        let path = version_dir.join("Textures").join(format!("{relative}.png"));
        std::fs::create_dir_all(path.parent().context("texture path has no parent")?)?;
        // Content is never decoded by the analyzer (it only checks
        // existence at the normalized path) — a one-byte placeholder is
        // enough and keeps the fixture small.
        std::fs::write(path, [0u8])?;
    }
    for relative in &m.sounds {
        let path = version_dir.join("Sounds").join(relative);
        std::fs::create_dir_all(path.parent().context("sound path has no parent")?)?;
        std::fs::write(path, [0u8])?;
    }
    for (language, file_name, keys) in &m.keyed {
        let dir = version_dir.join("Languages").join(language).join("Keyed");
        std::fs::create_dir_all(&dir)?;
        let mut body = String::from("<LanguageData>\n");
        for (key, value) in keys {
            let _ = writeln!(
                body,
                "  <{}>{}</{}>",
                escape_xml(key),
                escape_xml(value),
                escape_xml(key)
            );
        }
        body.push_str("</LanguageData>\n");
        std::fs::write(dir.join(file_name), body)?;
    }
    for (file_name, source) in &m.assemblies {
        let dir = version_dir.join("Assemblies");
        std::fs::create_dir_all(&dir)?;
        std::fs::copy(source, dir.join(file_name))
            .with_context(|| format!("copying assembly blob {}", source.display()))?;
    }
    Ok(())
}

pub(super) fn write_mod(root: &Path, m: &GenMod) -> Result<()> {
    write_mod_tree(&root.join("Mods").join(&m.folder), m)
}

/// [`write_mod`], registering this mod in the workshop tree instead of
/// `Mods/` — used for the one deliberately shadowed inactive pair
/// (the `_steam` Workshop-shadowing construct).
pub(super) fn write_workshop_mod(root: &Path, m: &GenMod, workshop_id: u64) -> Result<()> {
    let dest = root
        .join("workshop")
        .join("content")
        .join("294100")
        .join(workshop_id.to_string());
    write_mod_tree(&dest, m)
}
