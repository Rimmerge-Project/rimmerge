//! The engine's graphic model: which texture files a `Graphic` class reads
//! for one `texPath`. Pure: no IO, only a [`TextureCatalog`] to ask which
//! keys exist.
//!
//! Every key here is a normalized texture key (lowercased, `/`-separated,
//! no extension), the same convention as
//! [`crate::extract::textures::normalize`]. Each rule below was read off the
//! decompiled `Verse.Graphic_*` classes; `Graphic_Multi.Init` and
//! `Graphic_Collection.Init` are the two that matter most.

use std::collections::{BTreeMap, BTreeSet};

/// `Verse.Graphic_Single.MaskSuffix`: a texture whose name ends with this
/// is a colour mask, filtered out of every `Graphic_Collection` before
/// the collection is even checked for emptiness.
pub const MASK_SUFFIX: &str = "_m";

/// `Verse.Graphic_Multi`'s `NorthSuffix`/`EastSuffix`/`SouthSuffix`/
/// `WestSuffix`, in the engine's slot order (north, east, south, west).
pub const FACE_SUFFIXES: [&str; 4] = ["_north", "_east", "_south", "_west"];

/// Every vanilla `Graphic` class that inherits `Graphic_Collection`'s own
/// `Init` — read off the assembly with `ilspycmd`: none of them overrides
/// `Init`, so all of them resolve a folder `texPath` the way
/// `analysis::conflicts::textures`' resolution rule describes.
/// `Graphic_Collection` itself is abstract but listed for completeness.
///
/// Compared on the *last* `.`-segment, so a fully-qualified
/// `Verse.Graphic_Random` matches too. A class outside this list — a
/// mod's own `Graphic` subclass included — falls back to the lenient
/// folder rule: guessing that an unknown class behaves like
/// `Graphic_Collection` would fabricate exactly the kind of false
/// `MissingTexturePath` the existence gate exists to avoid.
pub const COLLECTION_GRAPHIC_CLASSES: [&str; 14] = [
    "Graphic_ActivityMaskRandom",
    "Graphic_ActivityStaged",
    "Graphic_Cluster",
    "Graphic_ClusterTight",
    "Graphic_Collection",
    "Graphic_Flicker",
    "Graphic_Genepack",
    "Graphic_Indexed",
    "Graphic_Indexed_SquashNStretch",
    "Graphic_MealVariants",
    "Graphic_MoteRandom",
    "Graphic_Random",
    "Graphic_StackCount",
    "Graphic_WithPropertyBlockRandom",
];

/// At most this many members of one collection are expanded; the rest are
/// counted in [`GraphicLayout::Collection`]'s `truncated`, never dropped
/// silently.
pub const MAX_COLLECTION_MEMBERS: usize = 64;

/// `StuffAppearanceDefOf.Smooth` (lowercased): the appearance every other
/// one falls back to in `Graphic_Appearances`.
pub const SMOOTH_APPEARANCE: &str = "smooth";

/// The simple (last `.`-segment, trimmed) name of a possibly
/// namespace-qualified class.
fn simple_class_name(graphic_class: &str) -> &str {
    graphic_class
        .rsplit('.')
        .next()
        .unwrap_or(graphic_class)
        .trim()
}

/// Whether `graphic_class` (possibly namespace-qualified) names one of
/// [`COLLECTION_GRAPHIC_CLASSES`].
#[must_use]
pub fn names_collection_graphic(graphic_class: &str) -> bool {
    COLLECTION_GRAPHIC_CLASSES.contains(&simple_class_name(graphic_class))
}

/// Whether a collection member's file name carries a rotation face
/// (`_north`/`_east`/`_south`/`_west`) anywhere in it —
/// `Graphic_Collection.Init` tests with `string.Contains`, not a suffix.
#[must_use]
pub fn names_rotation_face(file_name: &str) -> bool {
    FACE_SUFFIXES
        .iter()
        .any(|suffix| file_name.contains(suffix))
}

/// Which texture keys exist, in the union of every source the analyzer can
/// see (loose files, asset bundles, Core's built-in resources). A face
/// served by a bundle still exists to the engine, even if it is not
/// viewable.
pub trait TextureCatalog {
    /// Whether `key` exists exactly.
    fn contains(&self, key: &str) -> bool;

    /// Every key strictly under `folder/` (recursively), ascending and
    /// without duplicates.
    fn members_under(&self, folder: &str) -> Vec<&str>;
}

impl TextureCatalog for BTreeSet<String> {
    fn contains(&self, key: &str) -> bool {
        BTreeSet::contains(self, key)
    }

    fn members_under(&self, folder: &str) -> Vec<&str> {
        let prefix = format!("{folder}/");
        self.range(prefix.clone()..)
            .take_while(|key| key.starts_with(&prefix))
            .map(String::as_str)
            .collect()
    }
}

/// How a collection class picks a member; the member list itself is the
/// same for all of them (they share `Graphic_Collection.Init`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollectionKind {
    /// `Graphic_Random`: a thing picks `thingId % count`; with no thing the
    /// preview is member 0.
    Random,
    /// `Graphic_StackCount`: member 0 is the single-item graphic, the last
    /// is the full stack.
    StackCount,
    /// Any other class inheriting `Graphic_Collection.Init`.
    Other,
}

/// What the engine does with a `graphicData/graphicClass` value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphicClass {
    /// `Graphic_Single` (and `Graphic_Terrain`, which adds nothing).
    Single,
    /// `Graphic_Multi`.
    Multi,
    /// A class inheriting `Graphic_Collection.Init`.
    Collection(CollectionKind),
    /// `Graphic_Appearances`: its members depend on the `StuffAppearanceDef`
    /// list, see [`expand_appearances`].
    Appearances,
    /// Any other class (a mod's own `Graphic` subclass included): its
    /// layout is inferred from the files that exist.
    Unmodelled(String),
}

impl GraphicClass {
    /// Classifies a possibly namespace-qualified class name by its last
    /// `.`-segment.
    #[must_use]
    pub fn classify(graphic_class: &str) -> Self {
        match simple_class_name(graphic_class) {
            "Graphic_Single" | "Graphic_Terrain" => Self::Single,
            "Graphic_Multi" => Self::Multi,
            "Graphic_Random" => Self::Collection(CollectionKind::Random),
            "Graphic_StackCount" => Self::Collection(CollectionKind::StackCount),
            "Graphic_Appearances" => Self::Appearances,
            _ if names_collection_graphic(graphic_class) => Self::Collection(CollectionKind::Other),
            _ => Self::Unmodelled(graphic_class.trim().to_owned()),
        }
    }

    /// Whether the layout is a guess from file names rather than the
    /// engine's own rule for this class.
    #[must_use]
    pub fn is_inferred(&self) -> bool {
        matches!(self, Self::Unmodelled(_))
    }
}

/// A direction of a `Graphic_Multi`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Facing {
    North,
    East,
    South,
    West,
}

impl Facing {
    /// The engine's slot order.
    pub const ALL: [Self; 4] = [Self::North, Self::East, Self::South, Self::West];

    /// The file suffix this direction reads (`_north`, ...).
    #[must_use]
    pub fn suffix(self) -> &'static str {
        match self {
            Self::North => FACE_SUFFIXES[0],
            Self::East => FACE_SUFFIXES[1],
            Self::South => FACE_SUFFIXES[2],
            Self::West => FACE_SUFFIXES[3],
        }
    }
}

/// Whether the engine mirrors a face it substitutes from the opposite
/// side: `GraphicData.allowFlip`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlipPolicy {
    Allowed,
    Forbidden,
}

/// One texture the engine would show for one direction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FaceKey {
    /// The texture's key. A `Single`'s key and a collection's single
    /// member are not checked for existence here: that is the
    /// availability layer's job.
    pub key: String,
    /// Whether the engine draws this texture mirrored (west taken from
    /// east, or the reverse).
    pub is_mirrored: bool,
}

impl FaceKey {
    fn direct(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            is_mirrored: false,
        }
    }

    fn substituted(key: impl Into<String>, flip: FlipPolicy) -> Self {
        Self {
            key: key.into(),
            is_mirrored: flip == FlipPolicy::Allowed,
        }
    }
}

/// The four faces of a `Graphic_Multi` after the engine's fallbacks, so a
/// Multi with three faces is unrepresentable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultiFaces {
    pub north: FaceKey,
    pub east: FaceKey,
    pub south: FaceKey,
    pub west: FaceKey,
}

impl MultiFaces {
    /// The face for `facing`.
    #[must_use]
    pub fn face(&self, facing: Facing) -> &FaceKey {
        match facing {
            Facing::North => &self.north,
            Facing::East => &self.east,
            Facing::South => &self.south,
            Facing::West => &self.west,
        }
    }
}

/// One member of a collection, in engine order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CollectionMember {
    Single(FaceKey),
    Multi(MultiFaces),
    /// The engine builds a bad graphic here (a rotation group whose own
    /// `Graphic_Multi` finds no file). Carries the group key it tried.
    Missing(String),
}

/// What one `(class, base key)` expands to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphicLayout {
    Single(FaceKey),
    Multi(MultiFaces),
    /// Non-empty, in engine order; the engine's no-thing preview is
    /// member 0.
    Collection {
        members: Vec<CollectionMember>,
        /// Members beyond [`MAX_COLLECTION_MEMBERS`] that were not expanded.
        truncated: usize,
    },
    /// Nothing the engine could load.
    Missing,
}

/// The inputs of [`expand`].
#[derive(Debug, Clone, Copy)]
pub struct ExpandRequest<'a> {
    pub class: &'a GraphicClass,
    /// The normalized `texPath`.
    pub base_key: &'a str,
    pub flip: FlipPolicy,
}

/// Expands one graphic into the texture keys the engine reads.
///
/// A `Single` returns its key unchecked. [`GraphicClass::Appearances`]
/// needs the `StuffAppearanceDef` list ([`expand_appearances`]); here it
/// falls back to the file-layout probe, like [`GraphicClass::Unmodelled`].
#[must_use]
pub fn expand(request: &ExpandRequest<'_>, catalog: &impl TextureCatalog) -> GraphicLayout {
    let ExpandRequest {
        class,
        base_key,
        flip,
    } = *request;
    match class {
        GraphicClass::Single => GraphicLayout::Single(FaceKey::direct(base_key)),
        GraphicClass::Multi => expand_multi(base_key, flip, catalog)
            .map_or(GraphicLayout::Missing, GraphicLayout::Multi),
        GraphicClass::Collection(_) => expand_collection(base_key, flip, catalog),
        GraphicClass::Appearances | GraphicClass::Unmodelled(_) => {
            probe_layout(base_key, flip, catalog)
        }
    }
}

/// `Graphic_Multi.Init`: the four faces, with the engine's fallbacks.
/// `None` when not even the exact `base_key` exists.
fn expand_multi(
    base_key: &str,
    flip: FlipPolicy,
    catalog: &impl TextureCatalog,
) -> Option<MultiFaces> {
    let existing = |suffix: &str| {
        let key = format!("{base_key}{suffix}");
        catalog.contains(&key).then_some(key)
    };
    let [north, east, south, west] = FACE_SUFFIXES.map(existing);
    // North falls back to south, then east, then west, then the exact key.
    let north = north
        .or_else(|| south.clone())
        .or_else(|| east.clone())
        .or_else(|| west.clone())
        .or_else(|| catalog.contains(base_key).then(|| base_key.to_owned()))?;
    let south = south.unwrap_or_else(|| north.clone());
    // East falls back to west (mirrored), else north. West then falls back
    // to the resolved east, mirrored.
    let east = match (east, &west) {
        (Some(key), _) => FaceKey::direct(key),
        (None, Some(key)) => FaceKey::substituted(key.clone(), flip),
        (None, None) => FaceKey::direct(north.clone()),
    };
    let west = match west {
        Some(key) => FaceKey::direct(key),
        None => FaceKey::substituted(east.key.clone(), flip),
    };
    Some(MultiFaces {
        north: FaceKey::direct(north),
        east,
        south: FaceKey::direct(south),
        west,
    })
}

/// `Graphic_Collection.Init`: every non-mask file under the folder, ordered
/// by file name, grouped by the text before the first `_`; direction-named
/// files of a group fold into one `Graphic_Multi` appended after that
/// group's single members.
fn expand_collection(
    folder: &str,
    flip: FlipPolicy,
    catalog: &impl TextureCatalog,
) -> GraphicLayout {
    let mut names: Vec<&str> = catalog
        .members_under(folder)
        .into_iter()
        .filter_map(|key| key.rsplit('/').next())
        .filter(|name| !name.ends_with(MASK_SUFFIX))
        .collect();
    if names.is_empty() {
        return GraphicLayout::Missing;
    }
    // Stable, like LINQ's `orderby`: equal names keep catalog order. The
    // engine's comparer is culture-aware over the original case; catalog
    // keys are lowercased, so which name sorts first (the member 0 a
    // preview shows) can differ for names that differ only in case.
    names.sort();
    let mut members = Vec::new();
    for (group, files) in group_by_first_token(&names) {
        let (faces, singles): (Vec<&str>, Vec<&str>) = files
            .into_iter()
            .partition(|name| names_rotation_face(name));
        members.extend(
            singles
                .into_iter()
                .map(|name| CollectionMember::Single(FaceKey::direct(format!("{folder}/{name}")))),
        );
        if !faces.is_empty() {
            let group_key = format!("{folder}/{group}");
            members.push(match expand_multi(&group_key, flip, catalog) {
                Some(faces) => CollectionMember::Multi(faces),
                None => CollectionMember::Missing(group_key),
            });
        }
    }
    let truncated = members.len().saturating_sub(MAX_COLLECTION_MEMBERS);
    members.truncate(MAX_COLLECTION_MEMBERS);
    GraphicLayout::Collection { members, truncated }
}

/// Groups file names by the text before the first `_`, groups in order of
/// first appearance (LINQ `group by`).
fn group_by_first_token<'a>(names: &[&'a str]) -> Vec<(&'a str, Vec<&'a str>)> {
    let mut position_of: BTreeMap<&str, usize> = BTreeMap::new();
    let mut groups: Vec<(&str, Vec<&str>)> = Vec::new();
    for &name in names {
        let token = name.split('_').next().unwrap_or(name);
        let position = *position_of.entry(token).or_insert_with(|| {
            groups.push((token, Vec::new()));
            groups.len() - 1
        });
        if let Some((_, files)) = groups.get_mut(position) {
            files.push(name);
        }
    }
    groups
}

/// The layout of a class the analyzer has no engine rule for: any face
/// file present -> Multi; the exact key -> Single; non-mask files under
/// `key/` -> a collection; else nothing.
fn probe_layout(base_key: &str, flip: FlipPolicy, catalog: &impl TextureCatalog) -> GraphicLayout {
    let has_face_file = FACE_SUFFIXES
        .iter()
        .any(|suffix| catalog.contains(&format!("{base_key}{suffix}")));
    if has_face_file && let Some(faces) = expand_multi(base_key, flip, catalog) {
        return GraphicLayout::Multi(faces);
    }
    if catalog.contains(base_key) {
        return GraphicLayout::Single(FaceKey::direct(base_key));
    }
    expand_collection(base_key, flip, catalog)
}

/// One `StuffAppearanceDef` as `Graphic_Appearances` reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StuffAppearance {
    pub def_name: String,
    /// `pathPrefix`; empty or absent means the graphic's own folder.
    pub path_prefix: Option<String>,
}

/// One appearance's sub-graphic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppearanceMember {
    /// The appearance's `defName`, lowercased.
    pub def_name: String,
    pub member: CollectionMember,
}

/// `Graphic_Appearances.Init`: per appearance, the first file in its
/// folder whose name ends with the appearance's `defName`, else the
/// `Smooth` appearance's file. Members follow `appearances` order (the
/// `DefDatabase` order the caller supplies).
///
/// Approximation: "first" is the first by name in the catalog's
/// deduplicated union. The engine walks mods in load order and each mod's
/// files in directory order, so the two differ only when several files
/// (in one mod or across mods) end with the same `defName`.
#[must_use]
pub fn expand_appearances(
    base_key: &str,
    appearances: &[StuffAppearance],
    catalog: &impl TextureCatalog,
) -> Vec<AppearanceMember> {
    let own_file = |appearance: &StuffAppearance| {
        let def_name = appearance.def_name.to_lowercase();
        let folder = appearance_folder(base_key, appearance);
        catalog
            .members_under(&folder)
            .into_iter()
            .filter_map(|key| key.rsplit('/').next())
            .find(|name| name.ends_with(&def_name))
            .map(|name| FaceKey::direct(format!("{folder}/{name}")))
    };
    let smooth = appearances
        .iter()
        .find(|appearance| appearance.def_name.to_lowercase() == SMOOTH_APPEARANCE)
        .and_then(own_file);
    appearances
        .iter()
        .map(|appearance| AppearanceMember {
            def_name: appearance.def_name.to_lowercase(),
            member: match own_file(appearance).or_else(|| smooth.clone()) {
                Some(face) => CollectionMember::Single(face),
                None => CollectionMember::Missing(appearance_folder(base_key, appearance)),
            },
        })
        .collect()
}

/// `pathPrefix/lastSegment(base)` when a prefix is set, else `base`.
fn appearance_folder(base_key: &str, appearance: &StuffAppearance) -> String {
    match appearance
        .path_prefix
        .as_deref()
        .filter(|prefix| !prefix.is_empty())
    {
        Some(prefix) => {
            let last = base_key.rsplit('/').next().unwrap_or(base_key);
            format!("{}/{last}", prefix.to_lowercase())
        }
        None => base_key.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog(keys: &[&str]) -> BTreeSet<String> {
        keys.iter().map(|key| (*key).to_owned()).collect()
    }

    fn layout_of(class: &str, base_key: &str, keys: &[&str], flip: FlipPolicy) -> GraphicLayout {
        let class = GraphicClass::classify(class);
        expand(
            &ExpandRequest {
                class: &class,
                base_key,
                flip,
            },
            &catalog(keys),
        )
    }

    fn multi(class: &str, base_key: &str, keys: &[&str]) -> MultiFaces {
        match layout_of(class, base_key, keys, FlipPolicy::Allowed) {
            GraphicLayout::Multi(faces) => faces,
            other => panic!("expected Multi, got {other:?}"),
        }
    }

    fn face(key: &str, is_mirrored: bool) -> FaceKey {
        FaceKey {
            key: key.to_owned(),
            is_mirrored,
        }
    }

    fn members_of(layout: GraphicLayout) -> (Vec<CollectionMember>, usize) {
        match layout {
            GraphicLayout::Collection { members, truncated } => (members, truncated),
            other => panic!("expected Collection, got {other:?}"),
        }
    }

    fn single(key: &str) -> CollectionMember {
        CollectionMember::Single(face(key, false))
    }

    #[test]
    fn classify_matches_a_qualified_name_on_its_last_segment() {
        assert_eq!(
            GraphicClass::classify("Verse.Graphic_Random"),
            GraphicClass::Collection(CollectionKind::Random)
        );
        assert_eq!(
            GraphicClass::classify(" Graphic_StackCount "),
            GraphicClass::Collection(CollectionKind::StackCount)
        );
        assert_eq!(
            GraphicClass::classify("Graphic_Terrain"),
            GraphicClass::Single
        );
        assert_eq!(
            GraphicClass::classify("Graphic_Flicker"),
            GraphicClass::Collection(CollectionKind::Other)
        );
    }

    #[test]
    fn classify_sends_an_unknown_class_to_unmodelled() {
        let class = GraphicClass::classify("Example.Graphic_Custom");
        assert_eq!(
            class,
            GraphicClass::Unmodelled("Example.Graphic_Custom".into())
        );
        assert!(class.is_inferred());
        assert!(!GraphicClass::Multi.is_inferred());
    }

    #[test]
    fn single_returns_its_key_even_when_the_file_is_absent() {
        let layout = layout_of("Graphic_Single", "things/x", &[], FlipPolicy::Allowed);
        assert_eq!(layout, GraphicLayout::Single(face("things/x", false)));
    }

    #[test]
    fn multi_with_all_four_faces_uses_each_unmirrored() {
        let faces = multi(
            "Graphic_Multi",
            "things/x",
            &[
                "things/x_north",
                "things/x_east",
                "things/x_south",
                "things/x_west",
            ],
        );
        for facing in Facing::ALL {
            let expected = format!("things/x{}", facing.suffix());
            assert_eq!(faces.face(facing), &face(&expected, false));
        }
    }

    #[test]
    fn multi_without_west_mirrors_east() {
        let faces = multi(
            "Graphic_Multi",
            "things/x",
            &["things/x_north", "things/x_east", "things/x_south"],
        );
        assert_eq!(faces.west, face("things/x_east", true));
        assert_eq!(faces.east, face("things/x_east", false));
    }

    #[test]
    fn multi_without_east_mirrors_west() {
        let faces = multi(
            "Graphic_Multi",
            "things/x",
            &["things/x_north", "things/x_south", "things/x_west"],
        );
        assert_eq!(faces.east, face("things/x_west", true));
        assert_eq!(faces.west, face("things/x_west", false));
    }

    #[test]
    fn multi_without_north_takes_south() {
        let faces = multi(
            "Graphic_Multi",
            "things/x",
            &["things/x_south", "things/x_east"],
        );
        assert_eq!(faces.north, face("things/x_south", false));
    }

    #[test]
    fn multi_north_fallback_prefers_east_over_west() {
        let faces = multi(
            "Graphic_Multi",
            "things/x",
            &["things/x_east", "things/x_west"],
        );
        assert_eq!(faces.north, face("things/x_east", false));
        assert_eq!(faces.south, face("things/x_east", false));
    }

    #[test]
    fn multi_with_only_the_exact_key_fills_every_face_from_it() {
        let faces = multi("Graphic_Multi", "things/x", &["things/x"]);
        assert_eq!(faces.north, face("things/x", false));
        assert_eq!(faces.south, face("things/x", false));
        assert_eq!(faces.east, face("things/x", false));
        assert_eq!(faces.west, face("things/x", true));
    }

    #[test]
    fn multi_does_not_mirror_when_the_data_forbids_flipping() {
        let layout = layout_of(
            "Graphic_Multi",
            "things/x",
            &["things/x_north", "things/x_east", "things/x_south"],
            FlipPolicy::Forbidden,
        );
        let GraphicLayout::Multi(faces) = layout else {
            panic!("expected Multi");
        };
        assert_eq!(faces.west, face("things/x_east", false));
    }

    #[test]
    fn multi_with_no_file_at_all_is_missing() {
        let layout = layout_of(
            "Graphic_Multi",
            "things/x",
            &["things/y_south"],
            FlipPolicy::Allowed,
        );
        assert_eq!(layout, GraphicLayout::Missing);
    }

    #[test]
    fn collection_excludes_masks_and_flattens_subfolders_by_name() {
        let layout = layout_of(
            "Graphic_Random",
            "things/p",
            &["things/p/b", "things/p/a", "things/p/a_m", "things/p/sub/c"],
            FlipPolicy::Allowed,
        );
        let (members, truncated) = members_of(layout);
        assert_eq!(
            members,
            vec![
                single("things/p/a"),
                single("things/p/b"),
                single("things/p/c")
            ]
        );
        assert_eq!(truncated, 0);
    }

    #[test]
    fn collection_groups_rotation_files_into_one_multi_after_the_groups_singles() {
        let layout = layout_of(
            "Graphic_Random",
            "things/p",
            &[
                "things/p/fox_east",
                "things/p/fox_south",
                "things/p/fox_a",
                "things/p/owl",
            ],
            FlipPolicy::Allowed,
        );
        let (members, _) = members_of(layout);
        let CollectionMember::Multi(faces) = &members[1] else {
            panic!("expected the fox group's Multi second, got {members:?}");
        };
        assert_eq!(members[0], single("things/p/fox_a"));
        assert_eq!(faces.south, face("things/p/fox_south", false));
        assert_eq!(faces.west, face("things/p/fox_east", true));
        assert_eq!(members[2], single("things/p/owl"));
    }

    #[test]
    fn collection_rotation_group_with_a_longer_stem_is_a_missing_member() {
        let layout = layout_of(
            "Graphic_Random",
            "things/p",
            &["things/p/fox_pup_south"],
            FlipPolicy::Allowed,
        );
        let (members, _) = members_of(layout);
        assert_eq!(
            members,
            vec![CollectionMember::Missing("things/p/fox".into())]
        );
    }

    #[test]
    fn collection_with_nothing_under_the_folder_is_missing() {
        let layout = layout_of(
            "Graphic_StackCount",
            "things/p",
            &["things/p"],
            FlipPolicy::Allowed,
        );
        assert_eq!(layout, GraphicLayout::Missing);
    }

    #[test]
    fn stack_count_orders_members_by_name_so_the_single_item_graphic_is_first() {
        let layout = layout_of(
            "Graphic_StackCount",
            "things/p",
            &[
                "things/p/silver_c",
                "things/p/silver_a",
                "things/p/silver_b",
            ],
            FlipPolicy::Allowed,
        );
        let (members, _) = members_of(layout);
        assert_eq!(members.first(), Some(&single("things/p/silver_a")));
        assert_eq!(members.last(), Some(&single("things/p/silver_c")));
    }

    #[test]
    fn collection_beyond_the_bound_counts_what_it_drops() {
        let keys: Vec<String> = (0..70)
            .map(|index| format!("things/p/m{index:03}"))
            .collect();
        let refs: Vec<&str> = keys.iter().map(String::as_str).collect();
        let layout = layout_of("Graphic_Random", "things/p", &refs, FlipPolicy::Allowed);
        let (members, truncated) = members_of(layout);
        assert_eq!(members.len(), MAX_COLLECTION_MEMBERS);
        assert_eq!(truncated, 70 - MAX_COLLECTION_MEMBERS);
    }

    #[test]
    fn unmodelled_class_with_a_face_file_is_read_as_multi() {
        let layout = layout_of(
            "Example.Custom",
            "things/x",
            &["things/x_south"],
            FlipPolicy::Allowed,
        );
        assert!(matches!(layout, GraphicLayout::Multi(_)));
    }

    #[test]
    fn unmodelled_class_with_an_exact_key_is_read_as_single() {
        let layout = layout_of(
            "Example.Custom",
            "things/x",
            &["things/x"],
            FlipPolicy::Allowed,
        );
        assert_eq!(layout, GraphicLayout::Single(face("things/x", false)));
    }

    #[test]
    fn unmodelled_class_with_a_folder_is_read_as_a_collection() {
        let layout = layout_of(
            "Example.Custom",
            "things/x",
            &["things/x/a"],
            FlipPolicy::Allowed,
        );
        assert!(matches!(layout, GraphicLayout::Collection { .. }));
    }

    #[test]
    fn unmodelled_class_with_nothing_is_missing() {
        let layout = layout_of(
            "Example.Custom",
            "things/x",
            &["things/z"],
            FlipPolicy::Allowed,
        );
        assert_eq!(layout, GraphicLayout::Missing);
    }

    fn appearance(def_name: &str, path_prefix: Option<&str>) -> StuffAppearance {
        StuffAppearance {
            def_name: def_name.to_owned(),
            path_prefix: path_prefix.map(str::to_owned),
        }
    }

    #[test]
    fn appearances_pick_a_file_ending_in_the_def_name_and_fall_back_to_smooth() {
        let appearances = [appearance("Smooth", None), appearance("Bricks", None)];
        let members = expand_appearances(
            "things/wall",
            &appearances,
            &catalog(&["things/wall/wall_smooth", "things/wall/wall_bricks"]),
        );
        assert_eq!(members[0].member, single("things/wall/wall_smooth"));
        assert_eq!(members[1].member, single("things/wall/wall_bricks"));

        let only_smooth = expand_appearances(
            "things/wall",
            &appearances,
            &catalog(&["things/wall/wall_smooth"]),
        );
        assert_eq!(only_smooth[1].member, single("things/wall/wall_smooth"));
    }

    #[test]
    fn appearances_use_the_prefix_with_the_last_path_segment() {
        let appearances = [
            appearance("Smooth", None),
            appearance("Planks", Some("Things/Alt")),
        ];
        let members = expand_appearances(
            "things/wall",
            &appearances,
            &catalog(&["things/wall/a_smooth", "things/alt/wall/a_planks"]),
        );
        assert_eq!(members[1].member, single("things/alt/wall/a_planks"));
    }

    #[test]
    fn appearances_without_smooth_or_a_file_are_missing() {
        let members =
            expand_appearances("things/wall", &[appearance("Bricks", None)], &catalog(&[]));
        assert_eq!(
            members[0].member,
            CollectionMember::Missing("things/wall".into())
        );
    }
}
