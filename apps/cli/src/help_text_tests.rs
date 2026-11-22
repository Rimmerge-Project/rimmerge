//! `--help` is user-facing text, so the em dash rule applies to it as well as to the
//! messages the commands print. clap turns every `///` doc comment on a command, a
//! subcommand or an argument into help text; this walks the whole command tree, so a
//! new doc comment with an em dash fails here instead of reaching a user's terminal.
//! (`scripts/check-em-dashes.ps1` checks string literals, which doc comments are not.)

use clap::{Command, CommandFactory};

use crate::Cli;

const EM_DASH: char = '\u{2014}';

/// One piece of help text and where in the command tree it lives.
struct HelpText {
    location: String,
    text: String,
}

fn collect(command: &Command, path: &str, found: &mut Vec<HelpText>) {
    let here = if path.is_empty() {
        command.get_name().to_owned()
    } else {
        format!("{path} {}", command.get_name())
    };
    let command_texts = [
        ("about", command.get_about()),
        ("long_about", command.get_long_about()),
        ("before_help", command.get_before_help()),
        ("before_long_help", command.get_before_long_help()),
        ("after_help", command.get_after_help()),
        ("after_long_help", command.get_after_long_help()),
    ];
    for (field, text) in command_texts {
        if let Some(text) = text {
            found.push(HelpText {
                location: format!("{here}: {field}"),
                text: text.to_string(),
            });
        }
    }
    for argument in command.get_arguments() {
        let argument_texts = [
            ("help", argument.get_help()),
            ("long_help", argument.get_long_help()),
        ];
        for (field, text) in argument_texts {
            if let Some(text) = text {
                found.push(HelpText {
                    location: format!("{here}: --{} {field}", argument.get_id()),
                    text: text.to_string(),
                });
            }
        }
        for value in argument.get_possible_values() {
            if let Some(text) = value.get_help() {
                found.push(HelpText {
                    location: format!("{here}: --{} value {}", argument.get_id(), value.get_name()),
                    text: text.to_string(),
                });
            }
        }
    }
    for subcommand in command.get_subcommands() {
        collect(subcommand, &here, found);
    }
}

/// Every rendered `--help` and `-h` page of the tree, which is what a user actually reads.
fn rendered_pages(command: &Command, path: &str, found: &mut Vec<HelpText>) {
    let here = if path.is_empty() {
        command.get_name().to_owned()
    } else {
        format!("{path} {}", command.get_name())
    };
    let mut for_short = command.clone();
    let mut for_long = command.clone();
    found.push(HelpText {
        location: format!("{here}: rendered -h"),
        text: for_short.render_help().to_string(),
    });
    found.push(HelpText {
        location: format!("{here}: rendered --help"),
        text: for_long.render_long_help().to_string(),
    });
    for subcommand in command.get_subcommands() {
        rendered_pages(subcommand, &here, found);
    }
}

/// The count floors alone would pass a walk that stopped one level short.
fn assert_reaches_nested_subcommands(found: &[HelpText]) {
    assert!(
        found
            .iter()
            .any(|help| help.location.starts_with("rimmerge assign set-row:")),
        "the walk must reach nested subcommands such as `rimmerge assign set-row`"
    );
}

fn built_command() -> Command {
    let mut command = Cli::command();
    command.build();
    command
}

fn offenders(found: &[HelpText]) -> Vec<&str> {
    found
        .iter()
        .filter(|help| help.text.contains(EM_DASH))
        .map(|help| help.location.as_str())
        .collect()
}

#[test]
fn no_help_text_in_the_command_tree_contains_an_em_dash() {
    let command = built_command();
    let mut found = Vec::new();
    collect(&command, "", &mut found);

    assert!(
        found.len() > 100,
        "the walk must reach the whole tree, saw only {} texts",
        found.len()
    );
    assert_reaches_nested_subcommands(&found);
    let offending = offenders(&found);
    assert!(
        offending.is_empty(),
        "help text with an em dash; use a colon, comma, parentheses or a full stop:\n{}",
        offending.join("\n")
    );
}

#[test]
fn no_rendered_help_page_contains_an_em_dash() {
    let command = built_command();
    let mut found = Vec::new();
    rendered_pages(&command, "", &mut found);

    assert!(
        found.len() > 50,
        "the walk must reach every subcommand, saw only {} pages",
        found.len()
    );
    assert_reaches_nested_subcommands(&found);
    let offending = offenders(&found);
    assert!(
        offending.is_empty(),
        "rendered help with an em dash; use a colon, comma, parentheses or a full stop:\n{}",
        offending.join("\n")
    );
}

#[test]
fn the_walk_detects_an_em_dash_in_help_text() {
    let planted = Command::new("root").subcommand(
        Command::new("child").arg(clap::Arg::new("flag").long("flag").help("a \u{2014} b")),
    );
    let mut found = Vec::new();
    collect(&planted, "", &mut found);

    assert_eq!(offenders(&found), vec!["root child: --flag help"]);
}
