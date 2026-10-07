//! The one XML text escape every hand-rendered writer in this crate uses
//! (`ModsConfig.xml`, the `.rml` mod list).

/// Escapes the five XML predefined entities. Values written into a
/// document (a version string, a mod id, a mod name) come from files and
/// databases this process does not control, so every one is escaped: a
/// stray `&`/`<`/`>`/`"`/`'` can never corrupt the document.
pub(crate) fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Escapes only `&`, `<` and `>`: the three characters RimWorld's own
/// XML writer escapes in a text node (it writes `'` and `"` raw there).
/// Used where output must match the game byte for byte.
pub(crate) fn xml_escape_text(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_all_five_entities_without_double_escaping() {
        assert_eq!(
            xml_escape(r#"a&b <c> "d" 'e' &amp;"#),
            "a&amp;b &lt;c&gt; &quot;d&quot; &apos;e&apos; &amp;amp;"
        );
    }

    #[test]
    fn text_escape_leaves_quotes_raw_like_the_game() {
        assert_eq!(
            xml_escape_text(r#"Tom & Jerry's <"Best">"#),
            r#"Tom &amp; Jerry's &lt;"Best"&gt;"#
        );
    }

    #[test]
    fn leaves_plain_text_alone() {
        assert_eq!(xml_escape("Example Framework 1.6"), "Example Framework 1.6");
    }
}
