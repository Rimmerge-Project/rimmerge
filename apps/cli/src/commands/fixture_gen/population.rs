//! The population: how archetypes and planted situations become concrete mods, defs, and patches.

use std::fmt::Write as _;
use std::path::PathBuf;

use super::GenSummary;
use super::Spec;
use super::mods::{GenMod, Rng, escape_xml, write_mod, write_workshop_mod};
use anyhow::{Context, Result};

/// Every mutable, cross-method piece of state one `generate` run
/// accumulates — the spec, the RNG, the assembly-blob directory, and the
/// growing active/inactive-mod lists and cross-references (framework
/// ids, content-mod folders) later `plant_*` methods need. Not a
/// reusable type outside one `generate` call.
pub(super) struct Population<'a> {
    spec: &'a Spec,
    game_dir: PathBuf,
    out: PathBuf,
    bin: PathBuf,
    rng: Rng,
    active_order: Vec<String>,
    inactive_ids: Vec<String>,
    framework_ids: Vec<String>,
    /// `(id, folder)` for every content mod, in generation order.
    content_ids: Vec<(String, String)>,
}

impl<'a> Population<'a> {
    pub(super) fn new(spec: &'a Spec, bin: PathBuf, out: PathBuf) -> Self {
        Self {
            spec,
            game_dir: out.join("game"),
            out,
            bin,
            rng: Rng::new(spec.seed),
            active_order: Vec::new(),
            inactive_ids: Vec::new(),
            framework_ids: Vec::new(),
            content_ids: Vec::new(),
        }
    }

    fn write_mod(&self, m: &GenMod) -> Result<()> {
        write_mod(&self.game_dir, m)
    }

    /// Writes `m`'s extra def file(s) straight into an already-written
    /// mod's own `Defs/` directory, beside whatever [`write_mod`] already
    /// wrote for it -- used by constructs that add a second, independent
    /// def to a mod another `plant_*` method already created (def
    /// overrides, duplicate template names, the cross-mod parent-template
    /// child), so the mod is never written twice.
    fn extra_def(&self, folder: &str, file_name: &str, body: &str) -> Result<()> {
        let defs_dir = self
            .game_dir
            .join("Mods")
            .join(folder)
            .join("1.6")
            .join("Defs");
        std::fs::create_dir_all(&defs_dir)?;
        std::fs::write(defs_dir.join(file_name), format!("<Defs>\n{body}</Defs>\n"))?;
        Ok(())
    }

    /// [`Self::extra_def`]'s texture-folder sibling: writes one more
    /// placeholder texture straight into an already-written mod's own
    /// `Textures/` directory.
    fn extra_texture(&self, folder: &str, relative: &str) -> Result<()> {
        let path = self
            .game_dir
            .join("Mods")
            .join(folder)
            .join("1.6")
            .join("Textures")
            .join(format!("{relative}.png"));
        std::fs::create_dir_all(path.parent().context("texture path has no parent")?)?;
        std::fs::write(path, [0u8])?;
        Ok(())
    }

    // --- Core + 5 DLCs ---------------------------------------------------
    pub(super) fn plant_core_and_dlcs(&mut self) -> Result<()> {
        std::fs::create_dir_all(&self.game_dir)?;
        std::fs::write(
            self.game_dir.join("Version.txt"),
            format!("{}.4000 rev0\n", self.spec.game_version),
        )?;
        // An empty `RimWorldWin64_Data/Managed` so the vanilla-assembly
        // scan's own `std::fs::read_dir` succeeds with zero entries
        // instead of failing -- a *missing* directory produces a `Warning`
        // whose text embeds the raw OS error message, which is
        // locale-dependent and so not safe to commit into the golden
        // fixture.
        std::fs::create_dir_all(self.game_dir.join("RimWorldWin64_Data").join("Managed"))?;

        let dlcs = [
            ("ludeon.rimworld", "Core"),
            ("ludeon.rimworld.royalty", "Royalty"),
            ("ludeon.rimworld.ideology", "Ideology"),
            ("ludeon.rimworld.biotech", "Biotech"),
            ("ludeon.rimworld.anomaly", "Anomaly"),
            ("ludeon.rimworld.odyssey", "Odyssey"),
        ];
        let mut expansion_defs = String::from("<Defs>\n");
        for (id, label) in dlcs {
            let _ = writeln!(
                expansion_defs,
                "  <ExpansionDef>\n    <defName>{label}</defName>\n    <label>{label}</label>\n    \
                 <linkedMod>{id}</linkedMod>\n  </ExpansionDef>"
            );
        }
        expansion_defs.push_str("</Defs>\n");
        let expansion_dir = self
            .game_dir
            .join("Data")
            .join("Core")
            .join("Defs")
            .join("Misc")
            .join("ExpansionDefs");
        std::fs::create_dir_all(&expansion_dir)?;
        std::fs::write(expansion_dir.join("ExpansionDefs.xml"), expansion_defs)?;

        for (id, folder) in dlcs {
            let data_dir = self.game_dir.join("Data").join(folder).join("About");
            std::fs::create_dir_all(&data_dir)?;
            std::fs::write(
                data_dir.join("About.xml"),
                format!(
                    "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<ModMetaData>\n  \
                     <packageId>{id}</packageId>\n  <supportedVersions>\n    <li>1.6</li>\n  \
                     </supportedVersions>\n</ModMetaData>\n"
                ),
            )?;
        }

        self.active_order = dlcs.iter().map(|(id, ..)| (*id).to_string()).collect();
        Ok(())
    }

    // --- keep-list infrastructure -----------------------------------------
    pub(super) fn plant_keep_list_infrastructure(&mut self) -> Result<()> {
        // A real early-loading mod hooks assembly loading and legitimately
        // loads before Core on the real install
        // (`crates/rim-session/CLAUDE.md`'s "Activating and deactivating
        // mods" note) -- reproduced here so the synthetic install exercises
        // the same real, non-Core-first shape rather than a simplified one.
        // Both ids are synthetic (the keep list covers ids/shapes, not real
        // mods) -- only `example.earlyloader`/`example.patchlib` themselves
        // are on the keep list.
        let mut early_loader = GenMod::new("example.earlyloader", "EarlyLoader", "EarlyLoader");
        early_loader.load_before = vec![
            "ludeon.rimworld".to_string(),
            "example.patchlib".to_string(),
        ];
        self.write_mod(&early_loader)?;

        let mut patch_lib = GenMod::new("example.patchlib", "PatchLib", "PatchLib");
        patch_lib.load_after = vec!["example.earlyloader".to_string()];
        self.write_mod(&patch_lib)?;

        self.active_order.insert(0, "example.patchlib".to_string());
        self.active_order
            .insert(0, "example.earlyloader".to_string());
        Ok(())
    }

    // --- frameworks --------------------------------------------------------
    /// 8 frameworks. `example.framework01`/`02` ship the deliberately
    /// *ambiguous* `Exports` assembly (two owners -> `AnyOf`/
    /// `DuplicateAssembly`), at two real, distinct versions (1.0.0.0 /
    /// 1.1.0.0 -> `AssemblyVersionPrecedence`).
    /// `example.framework03` ships the *unambiguous* `ExportsSolo`
    /// (single owner -> a real `AssemblyRef` edge and, once
    /// `plant_framework_dependents` gives it 3 dependents,
    /// `is_framework_candidate`).
    pub(super) fn plant_frameworks(&mut self) -> Result<()> {
        for i in 1..=self.spec.archetypes.frameworks {
            let id = format!("example.framework{i:02}");
            let folder = format!("ExampleFramework{i:02}");
            let mut m = GenMod::new(&id, &folder, format!("Example Framework {i:02}"));
            let template = format!("SynthFrameworkBase{i:02}");
            m.defs.push((
                "FrameworkDefs.xml".to_string(),
                format!(
                    "  <ThingDef Name=\"{template}\" Abstract=\"True\">\n    \
                     <statBases>\n      <MaxHitPoints>100</MaxHitPoints>\n    </statBases>\n  \
                     </ThingDef>\n  <ThingDef ParentName=\"{template}\">\n    \
                     <defName>SynthFrameworkThing{i:02}</defName>\n  </ThingDef>\n"
                ),
            ));
            if i == 1 {
                m.assemblies
                    .push(("Exports.dll".to_string(), self.bin.join("Exports.dll")));
            } else if i == 2 {
                m.assemblies.push((
                    "Exports.dll".to_string(),
                    self.bin.join("versioned").join("Exports.dll"),
                ));
            } else if i == 3 {
                m.assemblies.push((
                    "ExportsSolo.dll".to_string(),
                    self.bin.join("ExportsSolo.dll"),
                ));
            }
            self.write_mod(&m)?;
            self.active_order.push(id.clone());
            self.framework_ids.push(id);
        }
        Ok(())
    }

    // --- framework dependents -----------------------------------------------
    /// The first `EXTENDS_HARD_OWNERS` dependents each ship the "extends
    /// Exports" assembly (ambiguous Hard AssemblyRef + AnyOf against the
    /// two `example.framework01`/`02`); the next `RUNTIME_PATCH_OWNERS` each
    /// ship one distinct runtime-patch-shaped assembly (six target
    /// pairs); two more ship the lazy-reference ("calls Exports in a
    /// method body") assembly; the next `EXTENDS_SOLO_OWNERS` ship the
    /// *unambiguous* "extends ExportsSolo" assembly (real `AssemblyRef`
    /// edges, and enough of them to make `example.framework03` a real
    /// framework candidate).
    pub(super) fn plant_framework_dependents(&mut self) -> Result<()> {
        const EXTENDS_HARD_OWNERS: usize = 4;
        const RUNTIME_PATCH_OWNERS: usize = 12;
        const EXTENDS_SOLO_OWNERS: usize = 3;

        for i in 1..=self.spec.archetypes.framework_dependents {
            let id = format!("synth.dependent.{i:03}");
            let folder = format!("SynthDependent{i:03}");
            let framework = &self.framework_ids[(i - 1) % self.framework_ids.len()];
            let mut m = GenMod::new(&id, &folder, format!("Synthetic Dependent {i:03}"));
            m.dependencies
                .push((framework.clone(), "Example Framework".to_string()));
            m.load_after.push(framework.clone());
            m.defs.push((
                "DependentDefs.xml".to_string(),
                format!(
                    "  <ThingDef>\n    <defName>SynthDependentThing{i:03}</defName>\n  </ThingDef>\n"
                ),
            ));

            if i <= EXTENDS_HARD_OWNERS {
                m.assemblies.push((
                    "ExtendsHard.dll".to_string(),
                    self.bin.join("ExtendsHard.dll"),
                ));
            } else if i <= EXTENDS_HARD_OWNERS + RUNTIME_PATCH_OWNERS {
                let n = i - EXTENDS_HARD_OWNERS;
                let name = format!("SynthPatch{n:02}.dll");
                m.assemblies.push((name.clone(), self.bin.join(&name)));
            } else if i <= EXTENDS_HARD_OWNERS + RUNTIME_PATCH_OWNERS + 2 {
                m.assemblies
                    .push(("CallsSoft.dll".to_string(), self.bin.join("CallsSoft.dll")));
            } else if i <= EXTENDS_HARD_OWNERS + RUNTIME_PATCH_OWNERS + 2 + EXTENDS_SOLO_OWNERS {
                m.assemblies.push((
                    "ExtendsSolo.dll".to_string(),
                    self.bin.join("ExtendsSolo.dll"),
                ));
            }

            self.write_mod(&m)?;
            self.active_order.push(id);
        }
        Ok(())
    }

    // --- content mods --------------------------------------------------------
    pub(super) fn plant_content_mods(&mut self) -> Result<()> {
        for i in 1..=self.spec.archetypes.content_mods {
            let id = format!("synth.content.{i:03}");
            let folder = format!("SynthContent{i:03}");
            let mut m = GenMod::new(&id, &folder, format!("Synthetic Content {i:03}"));
            m.defs.push((
                "ContentDefs.xml".to_string(),
                format!(
                    "  <ThingDef>\n    <defName>SynthContentThing{i:03}</defName>\n  </ThingDef>\n"
                ),
            ));
            self.content_ids.push((id.clone(), folder));
            self.write_mod(&m)?;
            self.active_order.push(id);
        }
        Ok(())
    }

    // --- retextures: texture overrides ---------------------------------------
    /// The first `2 * texture_overrides` retexture mods are paired up,
    /// each pair sharing its own distinct texture path -- **except**
    /// pair 1's second member, which gets its own unique path instead:
    /// `plant_retexture_after_owner` gives a *content* mod ownership of
    /// pair 1's shared path instead, which is what makes that pair's
    /// `RetextureAfterOwner` edge possible (that edge needs exactly one
    /// texture-only owner at a shared path, and two retexture-only mods
    /// sharing it never has exactly one).
    pub(super) fn plant_retextures(&mut self) -> Result<()> {
        for i in 1..=self.spec.archetypes.retextures {
            let id = format!("synth.retexture.{i:03}");
            let folder = format!("SynthRetexture{i:03}");
            let mut m = GenMod::new(&id, &folder, format!("Synthetic Retexture {i:03}"));
            if i == 2 {
                m.textures.push(format!("Things/SynthRetexture{i:03}"));
            } else if i <= self.spec.planted.texture_overrides * 2 {
                let pair = (i - 1) / 2 + 1;
                m.textures
                    .push(format!("Things/SynthSharedTexture{pair:03}"));
            } else {
                m.textures.push(format!("Things/SynthRetexture{i:03}"));
            }
            self.write_mod(&m)?;
            self.active_order.push(id);
        }
        Ok(())
    }

    /// `missing_texture_paths`: dedicated mods whose def names a texPath
    /// with no file behind it at all.
    pub(super) fn plant_missing_texture_paths(&mut self) -> Result<()> {
        for i in 1..=self.spec.planted.missing_texture_paths {
            let id = format!("synth.missingtex.{i:03}");
            let folder = format!("SynthMissingTex{i:03}");
            let mut m = GenMod::new(&id, &folder, format!("Synthetic Missing Texture {i:03}"));
            m.defs.push((
                "MissingTexDefs.xml".to_string(),
                format!(
                    "  <ThingDef>\n    <defName>SynthMissingTexThing{i:03}</defName>\n    \
                     <graphicData>\n      <texPath>Things/SynthMissingTexture{i:03}</texPath>\n    \
                     </graphicData>\n  </ThingDef>\n"
                ),
            ));
            self.write_mod(&m)?;
            self.active_order.push(id);
        }
        Ok(())
    }

    /// `sound_overrides`: pairs of mods shipping the same normalized
    /// sound path.
    pub(super) fn plant_sound_overrides(&mut self) -> Result<()> {
        for i in 1..=self.spec.planted.sound_overrides {
            for owner in 0..2 {
                let id = format!("synth.sound.{i:03}.{owner}");
                let folder = format!("SynthSound{i:03}_{owner}");
                let mut m = GenMod::new(&id, &folder, format!("Synthetic Sound {i:03}{owner}"));
                m.sounds.push(format!("Ambient/SynthSound{i:03}.ogg"));
                self.write_mod(&m)?;
                self.active_order.push(id);
            }
        }
        Ok(())
    }

    // --- translations: keyed translation collisions -----------------------
    /// Same pairing shape as [`Self::plant_retextures`]: the first
    /// `2 * keyed_translation_collisions` translation mods are paired up,
    /// each pair sharing its own distinct key.
    pub(super) fn plant_translations(&mut self) -> Result<()> {
        for i in 1..=self.spec.archetypes.translations {
            let id = format!("synth.translation.{i:03}");
            let folder = format!("SynthTranslation{i:03}");
            let mut m = GenMod::new(&id, &folder, format!("Synthetic Translation {i:03}"));
            let mut keys = vec![(format!("SynthKey{i:03}"), format!("Value {i:03}"))];
            if i <= self.spec.planted.keyed_translation_collisions * 2 {
                let pair = (i - 1) / 2 + 1;
                keys.push((
                    format!("SynthSharedKey{pair:03}"),
                    format!("Shared value {i:03}"),
                ));
            }
            m.keyed
                .push(("English".to_string(), "Keys.xml".to_string(), keys));
            self.write_mod(&m)?;
            self.active_order.push(id);
        }
        Ok(())
    }

    /// `analysis::conflicts::likely_duplicate_mods`'s real gate: >= 10
    /// identical `(def_type, def_name)` keys shared by both mods, covering
    /// at least half of the smaller mod's own def count, no declared
    /// relation between them, and *no* shared author (a shared author is
    /// read as "a deliberate split-out addon", not a duplicate) -- so both
    /// owners below ship the exact same 10 defNames, under different
    /// authors and unrelated identities.
    pub(super) fn plant_likely_duplicate_mods(&mut self) -> Result<()> {
        const SHARED_DEF_COUNT: usize = 10;
        for i in 1..=self.spec.planted.likely_duplicate_mods {
            let mut shared_defs = String::new();
            for n in 1..=SHARED_DEF_COUNT {
                let _ = writeln!(
                    shared_defs,
                    "  <ThingDef>\n    <defName>SynthDuplicateThing{i:03}_{n:02}</defName>\n  \
                     </ThingDef>"
                );
            }
            for owner in 0..2 {
                let id = format!("synth.duplicate.{i:03}.{owner}");
                let folder = format!("SynthDuplicate{i:03}_{owner}");
                let mut m = GenMod::new(
                    &id,
                    &folder,
                    format!("Duplicate Content Pack {i:03} ({owner})"),
                );
                m.authors = vec![format!("Author {i:03}{owner}")];
                m.defs
                    .push(("DupDefs.xml".to_string(), shared_defs.clone()));
                self.write_mod(&m)?;
                self.active_order.push(id);
            }
        }
        Ok(())
    }

    /// Reuses the content-mod population: mod `k` and mod `k +
    /// content_count/2` both define the same synthetic defName. Extra
    /// def files are added straight to each mod's already-written
    /// `Defs/` directory rather than re-running `write_mod`.
    pub(super) fn plant_def_overrides(&mut self) -> Result<()> {
        let half = self.content_ids.len() / 2;
        for i in 0..self.spec.planted.def_overrides.min(half) {
            let (_, folder_a) = self.content_ids[i].clone();
            let (_, folder_b) = self.content_ids[i + half].clone();
            let shared = format!("SynthOverride{i:03}Def");
            let body = format!("  <ThingDef>\n    <defName>{shared}</defName>\n  </ThingDef>\n");
            self.extra_def(&folder_a, &format!("Override{i:03}A.xml"), &body)?;
            self.extra_def(&folder_b, &format!("Override{i:03}B.xml"), &body)?;
        }
        Ok(())
    }

    /// A `Name`-attributed template registered by two different content
    /// mods -- ambiguous by construction (`DuplicateTemplateName`), so
    /// (per `rim-analyzer`'s own documented behaviour: "two or more
    /// foreign owners is genuinely an any-of and produces no
    /// `ParentTemplate` edge") a child inheriting from *this* template
    /// deliberately produces no edge either -- see
    /// [`Self::plant_parent_template_cross_mod_child`] for the
    /// unambiguous case that does.
    pub(super) fn plant_duplicate_template_names(&mut self) -> Result<()> {
        for i in 0..self.spec.planted.duplicate_template_names {
            let (_, folder_a) = self.content_ids[(i * 2) % self.content_ids.len()].clone();
            let (_, folder_b) = self.content_ids[(i * 2 + 1) % self.content_ids.len()].clone();
            let template = format!("SynthTemplate{i:03}");
            let body = format!(
                "  <ThingDef Name=\"{template}\" Abstract=\"True\">\n    \
                 <statBases>\n      <MaxHitPoints>50</MaxHitPoints>\n    </statBases>\n  \
                 </ThingDef>\n"
            );
            self.extra_def(&folder_a, &format!("Template{i:03}A.xml"), &body)?;
            self.extra_def(&folder_b, &format!("Template{i:03}B.xml"), &body)?;
            // One of the two duplicate registrations gets a real child --
            // "a child of a duplicated template". The
            // ambiguity means this child produces no `ParentTemplate`
            // edge (see this method's own doc comment); it still
            // exercises the *structural* shape (a real `ParentName`
            // attribute resolving through `XmlInheritance`'s own
            // ambiguous-any-of path), just not the edge itself.
            if i == 0 {
                self.extra_def(
                    &folder_a,
                    "TemplateChild.xml",
                    &format!(
                        "  <ThingDef ParentName=\"{template}\">\n    \
                         <defName>SynthTemplateChild</defName>\n  </ThingDef>\n"
                    ),
                )?;
            }
        }
        Ok(())
    }

    /// A def in the *last* content mod inherits `SynthFrameworkBase02`
    /// (`example.framework02`'s own, unambiguously-single-owner abstract
    /// template) -- a genuine cross-mod `ParentTemplate` edge: the
    /// content mod must load after `example.framework02`
    /// for `XmlInheritance.GetBestParentFor` to resolve the name.
    pub(super) fn plant_parent_template_cross_mod_child(&mut self) -> Result<()> {
        let (_, folder) = self
            .content_ids
            .last()
            .context("plant_content_mods must run before plant_parent_template_cross_mod_child")?
            .clone();
        self.extra_def(
            &folder,
            "CrossModTemplateChild.xml",
            "  <ThingDef ParentName=\"SynthFrameworkBase02\">\n    \
             <defName>SynthCrossModTemplateChild</defName>\n  </ThingDef>\n",
        )
    }

    /// Gives the *first* content mod ownership of texture-override pair
    /// 1's own shared path (`plant_retextures` leaves that path with
    /// exactly one texture-only owner, `synth.retexture.001`, once pair
    /// 1's second member was redirected to its own unique path) -- a
    /// real `RetextureAfterOwner` edge:
    /// `retexture_after_owner_edges` needs exactly one texture-only
    /// owner and at least one "content" owner (any mod that isn't purely
    /// texture-only) sharing the same path, and a content mod already
    /// shipping a def satisfies the second half for free.
    pub(super) fn plant_retexture_after_owner(&mut self) -> Result<()> {
        if self.spec.planted.texture_overrides == 0 {
            return Ok(());
        }
        let (_, folder) = self
            .content_ids
            .first()
            .context("plant_content_mods must run before plant_retexture_after_owner")?
            .clone();
        self.extra_texture(&folder, "Things/SynthSharedTexture001")
    }

    /// One pair stating the same relationship both ways --
    /// `forceLoadAfter`/`forceLoadBefore` are otherwise never exercised
    /// anywhere in this generated install.
    pub(super) fn plant_force_load_pair(&mut self) -> Result<()> {
        let a = "synth.forceload.001.a".to_string();
        let b = "synth.forceload.001.b".to_string();
        let mut ma = GenMod::new(&a, "SynthForceLoad001A", "Synthetic Force Load 001A");
        ma.force_load_after.push(b.clone());
        let mut mb = GenMod::new(&b, "SynthForceLoad001B", "Synthetic Force Load 001B");
        mb.force_load_before.push(a.clone());
        self.write_mod(&ma)?;
        self.write_mod(&mb)?;
        self.active_order.push(a);
        self.active_order.push(b);
        Ok(())
    }

    /// `patch_collisions` distinct target fields, two dedicated owners
    /// each -- one real `PatchCollision` conflict per pair, not one big
    /// conflict sharing a single literal xpath across every pair. The
    /// shape cycles through [`CollisionShape`] by pair index so the
    /// collision classifier sees every severity and the `removed_by`
    /// path. Two `Add`s of *different* `<li>` items would not collide at
    /// all (the analyzer keys an append by the items it injects), so the
    /// additive shape appends the *same* item from both owners.
    pub(super) fn plant_patch_collisions(&mut self) -> Result<()> {
        for k in 1..=self.spec.planted.patch_collisions {
            let shape = CollisionShape::for_pair(k);
            for owner in 0..2 {
                let id = format!("synth.patchcollision.{k:03}.{owner}");
                let folder = format!("SynthPatchCollision{k:03}_{owner}");
                let mut m = GenMod::new(
                    &id,
                    &folder,
                    format!("Synthetic Patch Collision {k:03}{owner}"),
                );
                m.patches
                    .push(("Collision.xml".to_string(), shape.operation_xml(k, owner)));
                self.write_mod(&m)?;
                self.active_order.push(id);
            }
        }
        Ok(())
    }

    /// The xpath/grammar zoo: one construct per index, 1..=13 (validated
    /// by `validate_spec` against `archetypes.patch_only`).
    pub(super) fn plant_patch_only_zoo(&mut self) -> Result<()> {
        let target_mod = self.framework_ids[0].clone();
        let target_def_type = "ThingDef";
        let target_def_name = "SynthFrameworkThing01";
        for i in 1..=self.spec.archetypes.patch_only {
            let id = format!("synth.patchonly.{i:03}");
            let folder = format!("SynthPatchOnly{i:03}");
            let mut m = GenMod::new(&id, &folder, format!("Synthetic Patch Only {i:03}"));

            if i == 1 {
                // `PatchOperationSequence`.
                m.patches.push((
                    "Sequence.xml".to_string(),
                    format!(
                        "  <Operation Class=\"PatchOperationSequence\">\n    <operations>\n      \
                         <li Class=\"PatchOperationAdd\">\n        \
                         <xpath>Defs/{target_def_type}[defName=\"{target_def_name}\"]/comps</xpath>\n        \
                         <value>\n          <li Class=\"SynthCompProperties\" />\n        </value>\n      \
                         </li>\n      <li Class=\"PatchOperationAdd\">\n        \
                         <xpath>Defs/{target_def_type}[defName=\"{target_def_name}\"]/description</xpath>\n        \
                         <value>Patched by sequence</value>\n      </li>\n    </operations>\n  </Operation>\n"
                    ),
                ));
            }
            if i == 2 {
                // `PatchOperationConditional`.
                m.patches.push((
                    "Conditional.xml".to_string(),
                    format!(
                        "  <Operation Class=\"PatchOperationConditional\">\n    \
                         <xpath>Defs/{target_def_type}[defName=\"{target_def_name}\"]</xpath>\n    \
                         <match Class=\"PatchOperationAdd\">\n      \
                         <xpath>Defs/{target_def_type}[defName=\"{target_def_name}\"]/synthConditionalTags</xpath>\n      \
                         <value>\n        <li>SynthConditionalTag</li>\n      </value>\n    </match>\n  \
                         </Operation>\n"
                    ),
                ));
            }
            if i == 3 {
                // A custom sequence-shaped class (`<operations>` child, not
                // literally `PatchOperationSequence`) -- structural
                // detection, no real mod's class name.
                m.patches.push((
                    "CustomSequence.xml".to_string(),
                    format!(
                        "  <Operation Class=\"example.CustomOperations.PatchOperationCustomSequence\">\n    \
                         <operations>\n      <li Class=\"PatchOperationAdd\">\n        \
                         <xpath>Defs/{target_def_type}[defName=\"{target_def_name}\"]/synthCustomSequenceTags</xpath>\n        \
                         <value>\n          <li>SynthCustomSequenceTag</li>\n        </value>\n      \
                         </li>\n    </operations>\n  </Operation>\n"
                    ),
                ));
            }
            if i == 4 {
                // `PatchOperationFindMod` naming a display name.
                m.patches.push((
                    "FindMod.xml".to_string(),
                    format!(
                        "  <Operation Class=\"PatchOperationFindMod\">\n    <mods>\n      \
                         <li>Example Framework 01</li>\n    </mods>\n    \
                         <match Class=\"PatchOperationAdd\">\n      \
                         <xpath>Defs/{target_def_type}[defName=\"{target_def_name}\"]/synthFindModTags</xpath>\n      \
                         <value>\n        <li>SynthFindModTag</li>\n      </value>\n    </match>\n  \
                         </Operation>\n"
                    ),
                ));
            }
            if i == 5 {
                // `MayRequire`/`MayRequireAnyOf` on def elements.
                m.defs.push((
                    "MayRequireDefs.xml".to_string(),
                    format!(
                        "  <ThingDef MayRequire=\"{target_mod}\">\n    \
                         <defName>SynthMayRequireThing</defName>\n  </ThingDef>\n  \
                         <ThingDef MayRequireAnyOf=\"{target_mod},{a}\">\n    \
                         <defName>SynthMayRequireAnyOfThing</defName>\n  </ThingDef>\n",
                        a = self.content_ids[0].0,
                    ),
                ));
            }
            if i == 6 {
                // `LoadFolders.xml` with `IfModActive` gates and versioned
                // folders -- `1.6/Compat/` must genuinely exist and hold
                // something for the gate to resolve to a real loaded
                // folder, not just be named.
                m.load_folders_xml = Some(format!(
                    "<loadFolders>\n  <v1.6>\n    <li>1.6</li>\n    \
                     <li IfModActive=\"{target_mod}\">1.6/Compat</li>\n  </v1.6>\n</loadFolders>\n"
                ));
                m.extra_files.push((
                    "1.6/Compat/Defs/CompatDefs.xml".to_string(),
                    "<Defs>\n  <ThingDef>\n    <defName>SynthCompatThing</defName>\n  \
                     </ThingDef>\n</Defs>\n"
                        .to_string(),
                ));
            }
            if i == 7 {
                // `PatchInvalidatesPredicate`, scenario 1 (B half): this mod
                // (`synth.patchonly.007`) replaces the predicate's own key
                // child; `synth.patchonly.008` (the `i == 8` arm below) adds
                // under the same predicate step -- the edge must fire between
                // the two.
                m.defs.push((
                    "PredicateDef.xml".to_string(),
                    "  <example.PartAssignmentDef>\n    <defName>SynthPredicateDef1</defName>\n    \
                     <degreeDatas>\n      <li>\n        <label>x</label>\n      </li>\n    \
                     </degreeDatas>\n  </example.PartAssignmentDef>\n"
                        .to_string(),
                ));
                m.patches.push((
                    "PredicateB.xml".to_string(),
                    "  <Operation Class=\"PatchOperationReplace\">\n    \
                     <xpath>Defs/example.PartAssignmentDef[defName=\"SynthPredicateDef1\"]/degreeDatas/li[label=\"x\"]/label</xpath>\n    \
                     <value>\n      <label>y</label>\n    </value>\n  </Operation>\n"
                        .to_string(),
                ));
            }
            if i == 8 {
                // Scenario 1 (A half) -- see the `i == 7` arm above.
                m.patches.push((
                    "PredicateA.xml".to_string(),
                    "  <Operation Class=\"PatchOperationAdd\">\n    \
                     <xpath>Defs/example.PartAssignmentDef[defName=\"SynthPredicateDef1\"]/degreeDatas/li[label=\"x\"]/statOffsets</xpath>\n    \
                     <value>\n      <li>1</li>\n    </value>\n  </Operation>\n"
                        .to_string(),
                ));
            }
            if i == 9 {
                // Scenario 2: same shape, but B for a second def, and A
                // wrapped in a same-xpath `PatchOperationConditional` -- must
                // NOT produce the edge (the predicate-edge rule excludes a
                // conditional-wrapped patcher).
                m.defs.push((
                    "PredicateDef2.xml".to_string(),
                    "  <example.PartAssignmentDef>\n    <defName>SynthPredicateDef2</defName>\n    \
                     <degreeDatas>\n      <li>\n        <label>x</label>\n      </li>\n    \
                     </degreeDatas>\n  </example.PartAssignmentDef>\n"
                        .to_string(),
                ));
                m.patches.push((
                    "PredicateB2.xml".to_string(),
                    "  <Operation Class=\"PatchOperationReplace\">\n    \
                     <xpath>Defs/example.PartAssignmentDef[defName=\"SynthPredicateDef2\"]/degreeDatas/li[label=\"x\"]/label</xpath>\n    \
                     <value>\n      <label>y</label>\n    </value>\n  </Operation>\n"
                        .to_string(),
                ));
            }
            if i == 10 {
                m.patches.push((
                    "PredicateA2.xml".to_string(),
                    "  <Operation Class=\"PatchOperationConditional\">\n    \
                     <xpath>Defs/example.PartAssignmentDef[defName=\"SynthPredicateDef2\"]/degreeDatas/li[label=\"x\"]/statOffsets</xpath>\n    \
                     <match Class=\"PatchOperationAdd\">\n      \
                     <xpath>Defs/example.PartAssignmentDef[defName=\"SynthPredicateDef2\"]/degreeDatas/li[label=\"x\"]/statOffsets</xpath>\n      \
                     <value>\n        <li>1</li>\n      </value>\n    </match>\n  </Operation>\n"
                        .to_string(),
                ));
            }
            if i == 11 {
                // A dangling `PatchOperationAdd` naming a defName nothing
                // ships -- `VerifyOrder` predicts this as a real
                // `PatchWillFail` (RimWorld logs "Patch operation ... failed"
                // for an xpath that matches nothing). Added so
                // `apps/cli/tests/synthetic_install_e2e.rs`'s own "at least
                // one predicted patch failure" anti-vacuous-green floor
                // has something real to find.
                m.patches.push((
                    "Dangling.xml".to_string(),
                    "  <Operation Class=\"PatchOperationAdd\">\n    \
                     <xpath>Defs/ThingDef[defName=\"SynthNoSuchDef\"]/tradeTags</xpath>\n    \
                     <value>\n      <li>SynthDanglingTag</li>\n    </value>\n  </Operation>\n"
                        .to_string(),
                ));
            }
            if i == 12 {
                // `PatchRemovedNodeCosmetic`, toucher half: adds one leaf
                // under `synthCosmeticParent` with no enclosing Sequence and
                // no Conditional -- trivially satisfies every one of the
                // cosmetic rule's four conditions (b/c/d are vacuous, a
                // holds because its own write lands exactly at the path the
                // `i == 13` remover below deletes). The `i == 13` mod
                // (`synth.patchonly.013`) is the remover half.
                m.patches.push((
                    "CosmeticToucher.xml".to_string(),
                    format!(
                        "  <Operation Class=\"PatchOperationAdd\">\n    \
                         <xpath>Defs/{target_def_type}[defName=\"{target_def_name}\"]/synthCosmeticParent</xpath>\n    \
                         <value>\n      <synthCosmeticChild>1</synthCosmeticChild>\n    </value>\n  </Operation>\n"
                    ),
                ));
            }
            if i == 13 {
                // `PatchRemovedNodeCosmetic`, remover half -- see the `i ==
                // 12` arm above. Removes the whole `synthCosmeticParent`
                // node the toucher writes under, so the final document is
                // identical whichever of the two mods loads last:
                // either the toucher's own write never lands because the
                // node is already gone, or it lands and is then deleted
                // along with the rest of the subtree.
                m.patches.push((
                    "CosmeticRemover.xml".to_string(),
                    format!(
                        "  <Operation Class=\"PatchOperationRemove\">\n    \
                         <xpath>Defs/{target_def_type}[defName=\"{target_def_name}\"]/synthCosmeticParent</xpath>\n  </Operation>\n"
                    ),
                ));
            }

            self.write_mod(&m)?;
            self.active_order.push(id);
        }
        Ok(())
    }

    /// Pairs of mods with a direct declared-vs-declared contradiction (A
    /// loadAfter B and B loadAfter A) -- an unresolvable direct cycle,
    /// forcing the sorter to drop one edge.
    pub(super) fn plant_cycles(&mut self) -> Result<()> {
        for i in 1..=self.spec.planted.cycles_forcing_dropped_edges {
            let a = format!("synth.cycle.{i:03}.a");
            let b = format!("synth.cycle.{i:03}.b");
            let mut ma = GenMod::new(
                &a,
                format!("SynthCycle{i:03}A"),
                format!("Synthetic Cycle {i:03}A"),
            );
            ma.load_after.push(b.clone());
            let mut mb = GenMod::new(
                &b,
                format!("SynthCycle{i:03}B"),
                format!("Synthetic Cycle {i:03}B"),
            );
            mb.load_after.push(a.clone());
            self.write_mod(&ma)?;
            self.write_mod(&mb)?;
            self.active_order.push(a);
            self.active_order.push(b);
        }
        Ok(())
    }

    pub(super) fn plant_missing_dependencies(&mut self) -> Result<()> {
        for i in 1..=self.spec.planted.missing_dependencies {
            let id = format!("synth.missingdep.{i:03}");
            let mut m = GenMod::new(
                &id,
                format!("SynthMissingDep{i:03}"),
                format!("Synthetic Missing Dependency {i:03}"),
            );
            m.dependencies.push((
                format!("synth.absent.{i:03}"),
                format!("Absent Dependency {i:03}"),
            ));
            self.write_mod(&m)?;
            self.active_order.push(id);
        }
        Ok(())
    }

    pub(super) fn plant_incompatible_pairs(&mut self) -> Result<()> {
        for i in 1..=self.spec.planted.incompatible_pairs {
            let a = format!("synth.incompatible.{i:03}.a");
            let b = format!("synth.incompatible.{i:03}.b");
            let mut ma = GenMod::new(
                &a,
                format!("SynthIncompatible{i:03}A"),
                format!("Synthetic Incompatible {i:03}A"),
            );
            ma.incompatible_with.push(b.clone());
            self.write_mod(&ma)?;
            let mb = GenMod::new(
                &b,
                format!("SynthIncompatible{i:03}B"),
                format!("Synthetic Incompatible {i:03}B"),
            );
            self.write_mod(&mb)?;
            self.active_order.push(a);
            self.active_order.push(b);
        }
        Ok(())
    }

    pub(super) fn plant_unsupported_versions(&mut self) -> Result<()> {
        for i in 1..=self.spec.planted.unsupported_versions {
            let id = format!("synth.unsupported.{i:03}");
            let mut m = GenMod::new(
                &id,
                format!("SynthUnsupported{i:03}"),
                format!("Synthetic Unsupported {i:03}"),
            );
            m.supported_versions = vec!["1.5".to_string()];
            self.write_mod(&m)?;
            self.active_order.push(id);
        }
        Ok(())
    }

    /// No rule is planted into the report itself (placements are
    /// `RuleSet` data, never part of a scan) -- these are just ordinary
    /// mods `sort_golden.rs`'s own tests pin `Top`/`Bottom` by id, the way
    /// it already did for the real install's own `Top`-pinned mod.
    pub(super) fn plant_top_bottom_pins(&mut self) -> Result<()> {
        for i in 1..=self.spec.planted.top_pinned {
            let id = format!("synth.toppin.{i:03}");
            let m = GenMod::new(
                &id,
                format!("SynthTopPin{i:03}"),
                format!("Synthetic Top Pin {i:03}"),
            );
            self.write_mod(&m)?;
            self.active_order.push(id);
        }
        for i in 1..=self.spec.planted.bottom_pinned {
            let id = format!("synth.bottompin.{i:03}");
            let m = GenMod::new(
                &id,
                format!("SynthBottomPin{i:03}"),
                format!("Synthetic Bottom Pin {i:03}"),
            );
            self.write_mod(&m)?;
            self.active_order.push(id);
        }
        Ok(())
    }

    pub(super) fn plant_inactive_mods(&mut self) -> Result<()> {
        for i in 1..=self.spec.inactive {
            let id = format!("synth.inactive.{i:03}");
            let m = GenMod::new(
                &id,
                format!("SynthInactive{i:03}"),
                format!("Synthetic Inactive {i:03}"),
            );
            self.write_mod(&m)?;
            self.inactive_ids.push(id);
        }
        Ok(())
    }

    /// One shadowed pair: a local copy and a workshop copy of the same
    /// packageId, only the local one active (`_steam` construct).
    pub(super) fn plant_shadowed_pair(&mut self) -> Result<()> {
        let shadowed_id = "synth.shadowed.001".to_string();
        let mut shadowed_local = GenMod::new(
            &shadowed_id,
            "SynthShadowedLocal",
            "Synthetic Content Shadowed",
        );
        shadowed_local.defs.push((
            "ShadowedDefs.xml".to_string(),
            "  <ThingDef>\n    <defName>SynthShadowedThing</defName>\n  </ThingDef>\n".to_string(),
        ));
        self.write_mod(&shadowed_local)?;
        self.active_order.push(shadowed_id.clone());

        let shadowed_workshop = GenMod::new(
            &shadowed_id,
            "SynthShadowedWorkshop",
            "Synthetic Content Shadowed",
        );
        // Offset matches `anonymize_report.rs`'s own synthetic-id
        // convention (a real Steam workshop id is a plain, mid-sized
        // `u64`; this just needs to look like one and stay stable).
        let workshop_id = 2_000_000_000 + self.rng.next_u64() % 1_000_000_000;
        write_workshop_mod(&self.out, &shadowed_workshop, workshop_id)
    }

    /// Writes `ModsConfig.xml` and returns the final summary.
    pub(super) fn finalize(self) -> Result<GenSummary> {
        let mut mods_config = String::from(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<ModsConfigData>\n  \
             <version>1.6.4000 rev0</version>\n  <activeMods>\n",
        );
        for id in &self.active_order {
            let _ = writeln!(mods_config, "    <li>{}</li>", escape_xml(id));
        }
        mods_config.push_str("  </activeMods>\n</ModsConfigData>\n");
        std::fs::write(self.game_dir.join("ModsConfig.xml"), mods_config)?;

        Ok(GenSummary {
            active_mods: self.active_order.len(),
            inactive_mods: self.inactive_ids.len() + 1, // +1: the shadowed workshop copy
            total_mod_folders: self.active_order.len() + self.inactive_ids.len() + 1,
        })
    }
}

/// The four ways one planted patch-collision pair contends for a field.
/// Every shape is a genuine `PatchCollision` under the analyzer's current
/// grouping; the severity each is meant to grade as is noted per variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CollisionShape {
    /// Both owners `Replace` the same scalar field (contested).
    ContestedScalar,
    /// Owner 0 `Add`s a list item to the node owner 1 `Replace`s
    /// (contested; the `Add` is not split because a node-acting op shares
    /// its key-space).
    AddVersusReplace,
    /// Both owners `Add` the identical `<li>` to one list (additive; the
    /// analyzer keys an append by the item it injects, so only an
    /// identical item collides).
    IdenticalAdditive,
    /// Owner 0 `Remove`s the field owner 1 `Replace`s (contested, with the
    /// remover recorded in `removed_by`).
    RemoveVersusReplace,
}

impl CollisionShape {
    const CYCLE: [Self; 4] = [
        Self::ContestedScalar,
        Self::AddVersusReplace,
        Self::IdenticalAdditive,
        Self::RemoveVersusReplace,
    ];

    /// Shape of the 1-based pair `pair`; a pure function of the index so
    /// the generated install stays deterministic.
    fn for_pair(pair: usize) -> Self {
        Self::CYCLE[(pair - 1) % Self::CYCLE.len()]
    }

    /// The patch file body (one `<Operation>`) for `owner` (0 or 1) of
    /// pair `pair`.
    fn operation_xml(self, pair: usize, owner: usize) -> String {
        let field = format!("synthCollision{pair:03}");
        let xpath = format!("Defs/ThingDef[defName=\"SynthFrameworkThing01\"]/{field}");
        let replace = |value: &str| {
            format!(
                "  <Operation Class=\"PatchOperationReplace\">\n    <xpath>{xpath}</xpath>\n    \
                 <value>\n      {value}\n    </value>\n  </Operation>\n"
            )
        };
        let add_item = |item: &str| {
            format!(
                "  <Operation Class=\"PatchOperationAdd\">\n    <xpath>{xpath}</xpath>\n    \
                 <value>\n      <li>{item}</li>\n    </value>\n  </Operation>\n"
            )
        };
        let remove = format!(
            "  <Operation Class=\"PatchOperationRemove\">\n    <xpath>{xpath}</xpath>\n  </Operation>\n"
        );
        match (self, owner) {
            (Self::ContestedScalar, _) => {
                replace(&format!("<{field}>SynthValue{pair:03}_{owner}</{field}>"))
            }
            (Self::AddVersusReplace, 0) => add_item(&format!("SynthTag{pair:03}")),
            (Self::AddVersusReplace, _) => {
                replace(&format!("<{field}><li>SynthKept{pair:03}</li></{field}>"))
            }
            (Self::IdenticalAdditive, _) => add_item(&format!("SynthTag{pair:03}")),
            (Self::RemoveVersusReplace, 0) => remove,
            (Self::RemoveVersusReplace, _) => {
                replace(&format!("<{field}>SynthValue{pair:03}</{field}>"))
            }
        }
    }
}
