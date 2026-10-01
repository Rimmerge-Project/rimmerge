//! Rendering `About/About.xml`.

use std::collections::{BTreeMap, BTreeSet};

use rim_analyzer::domain::ModId;

use super::input::AboutSpec;
use crate::xml::escape_text;

pub(super) fn join_ids(ids: &BTreeSet<ModId>) -> String {
    ids.iter().map(ModId::as_str).collect::<Vec<_>>().join(",")
}

pub(super) fn render_about_xml(
    about: &AboutSpec<'_>,
    game_version: &str,
    depends_on: &BTreeMap<ModId, String>,
    load_after: &BTreeSet<ModId>,
) -> String {
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n");
    out.push_str("<ModMetaData>\n");
    out.push_str(&format!(
        "  <packageId>{}</packageId>\n",
        escape_text(about.identity.package_id.as_str())
    ));
    out.push_str(&format!(
        "  <name>{}</name>\n",
        escape_text(&about.identity.display_name)
    ));
    out.push_str(&format!(
        "  <author>{}</author>\n",
        escape_text(about.author)
    ));
    out.push_str(&format!(
        "  <description>{}</description>\n",
        escape_text(about.description)
    ));
    out.push_str("  <supportedVersions>\n");
    out.push_str(&format!("    <li>{}</li>\n", escape_text(game_version)));
    out.push_str("  </supportedVersions>\n");
    out.push_str("  <modDependencies>\n");
    for (id, name) in depends_on {
        out.push_str("    <li>\n");
        out.push_str(&format!(
            "      <packageId>{}</packageId>\n",
            escape_text(id.as_str())
        ));
        out.push_str(&format!(
            "      <displayName>{}</displayName>\n",
            escape_text(name)
        ));
        out.push_str("    </li>\n");
    }
    out.push_str("  </modDependencies>\n");
    out.push_str("  <loadAfter>\n");
    for id in load_after {
        out.push_str(&format!("    <li>{}</li>\n", escape_text(id.as_str())));
    }
    out.push_str("  </loadAfter>\n");
    out.push_str("</ModMetaData>\n");
    out
}
