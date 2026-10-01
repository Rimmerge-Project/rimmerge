//! Tests for the `assign` command's parsers.

use super::*;

// -- parse_def_key --------------------------------------------------

#[test]
fn parse_def_key_splits_on_the_first_slash() {
    let key = parse_def_key("ThingDef/Human").expect("must parse");
    assert_eq!(key.def_type, "ThingDef");
    assert_eq!(key.def_name, "Human");
}

#[test]
fn parse_def_key_only_splits_once_a_def_name_may_itself_contain_a_slash() {
    // Not a realistic RimWorld defName, but the parser must still do
    // the unsurprising thing rather than silently truncating.
    let key = parse_def_key("example.PartAssignmentDef/Group/Extra").expect("must parse");
    assert_eq!(key.def_type, "example.PartAssignmentDef");
    assert_eq!(key.def_name, "Group/Extra");
}

#[test]
fn parse_def_key_without_a_slash_is_an_error() {
    assert!(parse_def_key("NoSlashHere").is_err());
}

// -- parse_field_path -------------------------------------------------

#[test]
fn parse_field_path_accepts_a_plain_top_level_tag() {
    let path = parse_field_path("speciesNames").expect("must parse");
    assert_eq!(path.to_string(), "speciesNames");
}

#[test]
fn parse_field_path_rejects_a_malformed_li_spec() {
    // `li[#...]` must be a valid position number.
    assert!(parse_field_path("parts/li[#notanumber]").is_err());
}

// -- parse_row_value --------------------------------------------------

#[test]
fn parse_row_value_names_splits_on_commas() {
    assert_eq!(
        parse_row_value("names:a,b,c").expect("must parse"),
        RowValue::Names(vec!["a".to_string(), "b".to_string(), "c".to_string()])
    );
}

#[test]
fn parse_row_value_numbers_splits_and_parses_each_value() {
    assert_eq!(
        parse_row_value("numbers:1.0,2.5").expect("must parse"),
        RowValue::Numbers(vec![1.0, 2.5])
    );
}

#[test]
fn parse_row_value_numbers_lets_nan_and_negative_zero_pass_through() {
    // Neither is rejected *here* — `f64::from_str` accepts both, and
    // this parser's own job is turning CLI text into a `RowValue`,
    // not enforcing the domain's own finiteness rule
    // (`AssignmentRowError::NonFiniteChance`, checked by
    // `AssignmentProject::set_row` once the value reaches the
    // domain — see `an_e2e_nan_chance_is_rejected_by_the_domain_not_the_cli_parser`).
    let RowValue::Numbers(numbers) =
        parse_row_value("numbers:NaN").expect("NaN parses as a plain f64")
    else {
        panic!("expected RowValue::Numbers");
    };
    assert!(numbers[0].is_nan());

    assert_eq!(
        parse_row_value("numbers:-0").expect("must parse"),
        RowValue::Numbers(vec![-0.0])
    );
}

#[test]
fn parse_row_value_numbers_rejects_a_non_numeric_entry() {
    assert!(parse_row_value("numbers:1.0,not-a-number").is_err());
}

#[test]
fn parse_row_value_text_keeps_the_remainder_verbatim() {
    assert_eq!(
        parse_row_value("text:hello world").expect("must parse"),
        RowValue::Text("hello world".to_string())
    );
}

#[test]
fn parse_row_value_omit() {
    assert_eq!(parse_row_value("omit").expect("must parse"), RowValue::Omit);
}

#[test]
fn parse_row_value_rejects_an_unrecognized_spec() {
    assert!(parse_row_value("bogus:whatever").is_err());
}

// -- parse_row_values ---------------------------------------------------

#[test]
fn parse_row_values_parses_several_path_spec_pairs() {
    let values = parse_row_values(&[
        "parts=names:Part1".to_string(),
        "enabled=text:false".to_string(),
    ])
    .expect("must parse");
    assert_eq!(
        values.get(&"parts".parse().unwrap()),
        Some(&RowValue::Names(vec!["Part1".to_string()]))
    );
    assert_eq!(
        values.get(&"enabled".parse().unwrap()),
        Some(&RowValue::Text("false".to_string()))
    );
}

#[test]
fn parse_row_values_rejects_an_entry_missing_the_equals_separator() {
    assert!(parse_row_values(&["parts-names:Part1".to_string()]).is_err());
}
