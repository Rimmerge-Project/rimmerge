//! The one place a command turns a path from the webview into a
//! [`UserPath`]: every command that receives a file or folder path calls
//! [`parse_user_path`] before it looks at the file system, so a network share
//! is refused without contacting it (see `rim_session::user_path` for the rule
//! and why). A command never builds a `PathBuf` from request text itself.

use rim_session::user_path::{UserPath, UserPathError};

use super::{CommandError, CommandErrorCode};

/// Parses `raw`, the text of the path named by `what` (a noun phrase for the
/// error message, such as "the profile folder").
///
/// Surrounding whitespace is trimmed here and nowhere else, so every path
/// field answers to one rule: a hand-typed or pasted ` C:\x ` is `C:\x`
/// whichever command receives it.
///
/// # Errors
///
/// [`CommandErrorCode::NetworkPathRefused`] for a network share (UNC or
/// WebDAV), the one refusal a user can cause with a file picker and fix by
/// mapping a drive; [`CommandError::invalid_input`] for an empty, relative,
/// NUL-holding or device path.
pub(crate) fn parse_user_path(raw: &str, what: &str) -> Result<UserPath, CommandError> {
    UserPath::parse(raw.trim()).map_err(|error| match error {
        UserPathError::NetworkPath => CommandError::new(
            CommandErrorCode::NetworkPathRefused,
            format!("{what} is on a network share; use a folder on a drive letter"),
        ),
        UserPathError::Empty
        | UserPathError::ContainsNul
        | UserPathError::NotAbsolute
        | UserPathError::DevicePath => CommandError::invalid_input(format!("{what}: {error}")),
    })
}

/// [`parse_user_path`] for an optional field: `None` stays `None`, and a
/// blank text is refused like any other empty path.
///
/// # Errors
///
/// As [`parse_user_path`].
pub(crate) fn parse_optional_user_path(
    raw: Option<&str>,
    what: &str,
) -> Result<Option<UserPath>, CommandError> {
    raw.map(|text| parse_user_path(text, what)).transpose()
}

#[cfg(test)]
mod tests {
    #[cfg(windows)]
    mod windows_paths {
        use super::super::*;

        #[test]
        fn a_network_share_has_its_own_code() {
            for raw in [
                r"\\server\share\x",
                r"\\?\UNC\server\share\x",
                r"\\server@SSL\x",
            ] {
                let error = parse_user_path(raw, "the folder").expect_err(raw);

                assert_eq!(error.code, CommandErrorCode::NetworkPathRefused, "{raw}");
            }
        }

        #[test]
        fn every_other_refusal_is_invalid_input() {
            for raw in [
                "",
                "x\\y",
                "C:\\x\0",
                r"\\.\pipe\x",
                r"\\?\GLOBALROOT\x",
                "C:drive-relative",
            ] {
                let error = parse_user_path(raw, "the folder").expect_err(raw);

                assert_eq!(error.code, CommandErrorCode::InvalidInput, "{raw}");
            }
        }

        #[test]
        fn the_message_names_what_was_refused() {
            let error = parse_user_path(r"\\server\share", "the profile folder").expect_err("unc");

            assert!(error.message.starts_with("the profile folder"));
        }

        #[test]
        fn a_drive_letter_path_parses_and_an_absent_optional_stays_absent() {
            let parsed = parse_optional_user_path(Some(r"C:\x"), "the file").expect("drive path");
            let absent = parse_optional_user_path(None, "the file").expect("absent");

            assert_eq!(
                parsed.expect("some").as_path(),
                std::path::Path::new(r"C:\x")
            );
            assert_eq!(absent, None);
        }

        #[test]
        fn surrounding_whitespace_is_trimmed_for_every_command() {
            let parsed = parse_user_path("  C:\\x\\y \t", "the file").expect("trimmed");

            assert_eq!(parsed.as_path(), std::path::Path::new(r"C:\x\y"));
        }

        #[test]
        fn a_reserved_device_name_is_invalid_input() {
            let error = parse_user_path(r"C:\x\COM1.rml", "the file").expect_err("device");

            assert_eq!(error.code, CommandErrorCode::InvalidInput);
        }

        #[test]
        fn a_blank_optional_is_refused_not_skipped() {
            let error = parse_optional_user_path(Some("  "), "the file").expect_err("blank");

            assert_eq!(error.code, CommandErrorCode::InvalidInput);
        }
    }
}
