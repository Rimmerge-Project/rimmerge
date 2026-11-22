//! The one text fold every search box in this crate compares with.

/// One character's search fold: its Unicode lowercase, with the final sigma `ς` written as `σ`.
///
/// Per character on purpose: `str::to_lowercase` applies the context-sensitive final-sigma rule
/// (a word-final `Σ` becomes `ς`, any other `Σ` becomes `σ`), so the same letter would fold
/// differently at the end of a search text and inside a name, and `ΑΣ` would not find `ΑΣΑ`.
/// Mapping `ς` to `σ` as well (Unicode case folding does the same) lets a word-final `ς`, which
/// is how Greek is typed, find a capital `Σ` anywhere, and a capital `Σ` find a typed `ς`.
pub(crate) fn fold_char(character: char) -> impl Iterator<Item = char> {
    character
        .to_lowercase()
        .map(|lower| if lower == 'ς' { 'σ' } else { lower })
}

/// `text` as the searches compare it: every character through [`fold_char`].
///
/// The folded text is exactly the concatenation of its characters' folds, which the Mods list's
/// highlight mapping relies on. Both the search text and the searched text go through this
/// function. Sort keys keep `str::to_lowercase` (they are not searches).
#[must_use]
pub(crate) fn search_fold(text: &str) -> String {
    text.chars().flat_map(fold_char).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_capital_sigma_and_both_lowercase_sigmas_fold_alike() {
        assert_eq!(search_fold("ΑΣ"), "ασ");
        assert_eq!(search_fold("ας"), "ασ");
        assert_eq!(search_fold("ΚΟΣΜΟΣ"), search_fold("κοσμος"));
    }

    #[test]
    fn ascii_and_length_changing_characters_fold_as_lowercase() {
        assert_eq!(search_fold("Combat"), "combat");
        assert_eq!(search_fold("İ"), "i\u{307}");
    }
}
