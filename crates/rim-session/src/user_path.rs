//! [`UserPath`]: the one parser for a file or folder path an interface
//! receives from outside the process (a webview, a file picker, a form field).
//!
//! The rule is an allowlist on the parsed Windows prefix: only a drive-letter
//! path (`C:\x`, `\\?\C:\x`) is accepted. A mapped network drive (`Z:\x`) is a
//! drive letter the user configured themselves, so it passes. A raw network
//! path (`\\server\share\x`, `\\?\UNC\server\share\x`, a WebDAV
//! `\\server@SSL\x`) is refused, and so is every device path (`\\.\pipe\x`,
//! `\\?\GLOBALROOT\x`, `\\?\CONIN$`, the bare volume `\\?\C:`) and any path
//! with a legacy DOS device name as a component (`C:\x\NUL`, `C:\x\COM1.rml`;
//! see [`is_reserved_device_name`]). The parser reads text only, so a device
//! or share reached through a symbolic link is out of its reach.
//!
//! Why: any file-system call on a network path, even a metadata check, makes
//! Windows contact the host and authenticate with the user's credentials
//! (NTLMv2 over SMB), which is a connection outside the closed network list
//! and leaks a hash to whoever controls the host. An offline or slow share
//! also blocks the call until the SMB timeout. A device path can block on
//! open. The parser works on the text alone: it never touches the file
//! system, so refusing a network path never contacts the host.

use std::path::{Component, Path, PathBuf, Prefix};

/// Why a text is not an acceptable [`UserPath`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum UserPathError {
    /// The text is empty or only whitespace.
    #[error("the path is empty")]
    Empty,
    /// The text holds a NUL character, which no path can.
    #[error("the path holds a character a path cannot")]
    ContainsNul,
    /// The text is relative, rooted without a drive, or drive-relative
    /// (`C:x`).
    #[error("the path must be absolute")]
    NotAbsolute,
    /// The text names a network location (UNC or WebDAV).
    #[error("the path is on a network share")]
    NetworkPath,
    /// The text names a device: a device namespace (`\\.\pipe\x`,
    /// `\\?\GLOBALROOT\x`), a bare volume (`\\?\C:`) or a path with a legacy
    /// DOS device name as a component (`C:\x\NUL`).
    #[error("the path is a device path")]
    DevicePath,
}

/// An absolute path that names a place on a drive letter: validated, never
/// empty, free of NUL, and never a network or device path.
///
/// Holding one proves the text was checked; it does not prove the path
/// exists. A symbolic link inside a local path may still lead to a share (a
/// junction cannot: it only targets local volumes): that is the file system's
/// doing and out of this parser's reach.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UserPath(PathBuf);

impl UserPath {
    /// Parses `raw` as a drive-letter path.
    ///
    /// # Errors
    ///
    /// The [`UserPathError`] naming the first rule the text breaks.
    pub fn parse(raw: &str) -> Result<Self, UserPathError> {
        if raw.trim().is_empty() {
            return Err(UserPathError::Empty);
        }
        if raw.contains('\0') {
            return Err(UserPathError::ContainsNul);
        }
        let path = Path::new(raw);
        let mut components = path.components();
        if let Some(Component::Prefix(prefix)) = components.next() {
            check_prefix(prefix.kind(), components.next())?;
        }
        if !path.is_absolute() {
            return Err(UserPathError::NotAbsolute);
        }
        if path.components().any(is_reserved_component) {
            return Err(UserPathError::DevicePath);
        }
        Ok(Self(path.to_path_buf()))
    }

    /// The path.
    #[must_use]
    pub fn as_path(&self) -> &Path {
        &self.0
    }

    /// The path, owned.
    #[must_use]
    pub fn into_path_buf(self) -> PathBuf {
        self.0
    }
}

/// The allowlist on the parsed prefix. `after_prefix` is the component that
/// follows it: `\\?\C:` with no root after it names the volume device, not a
/// folder.
fn check_prefix(
    kind: Prefix<'_>,
    after_prefix: Option<Component<'_>>,
) -> Result<(), UserPathError> {
    match kind {
        Prefix::Disk(_) => Ok(()),
        Prefix::VerbatimDisk(_) if matches!(after_prefix, Some(Component::RootDir)) => Ok(()),
        Prefix::VerbatimDisk(_) | Prefix::DeviceNS(_) => Err(UserPathError::DevicePath),
        Prefix::UNC(..) | Prefix::VerbatimUNC(..) => Err(UserPathError::NetworkPath),
        // `\\?\unc\s\x`: `std` knows only the exact `UNC` spelling, but the
        // object manager matches the name case-insensitively, so it is a share.
        Prefix::Verbatim(name) if name.eq_ignore_ascii_case("UNC") => {
            Err(UserPathError::NetworkPath)
        }
        Prefix::Verbatim(_) => Err(UserPathError::DevicePath),
    }
}

fn is_reserved_component(component: Component<'_>) -> bool {
    matches!(component, Component::Normal(name) if is_reserved_device_name(&name.to_string_lossy()))
}

/// Whether `name`, one path component, is a legacy DOS device name that
/// Windows resolves to a device inside any folder: `CON`, `PRN`, `AUX`,
/// `NUL`, `COM1`-`COM9`, `LPT1`-`LPT9` (also with the superscript digits
/// `\u{b9}`, `\u{b2}`, `\u{b3}`), `CONIN$` and `CONOUT$`. Case, an extension
/// and spaces before it do not matter: `nul.txt` and `Con .x` are the device.
/// Opening one can block, so it is refused like a device path.
#[must_use]
pub fn is_reserved_device_name(name: &str) -> bool {
    // The device name ends at the first dot or colon (`NUL.txt`,
    // `NUL:stream`), and the spaces before that point are dropped.
    let stem = name
        .split(['.', ':'])
        .next()
        .unwrap_or_default()
        .trim_end_matches(' ');
    let upper = stem.to_uppercase();
    if matches!(
        upper.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) {
        return true;
    }
    upper
        .strip_prefix("COM")
        .or_else(|| upper.strip_prefix("LPT"))
        .is_some_and(is_device_number)
}

fn is_device_number(rest: &str) -> bool {
    let mut chars = rest.chars();
    matches!(
        (chars.next(), chars.next()),
        (Some('1'..='9' | '\u{b9}' | '\u{b2}' | '\u{b3}'), None)
    )
}

impl AsRef<Path> for UserPath {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

// The prefixes are parsed by `std` on Windows only; elsewhere a backslash is a
// file-name character, so these forms are not prefixes there. The product is a
// Windows program and CI runs on Windows.
#[cfg(test)]
mod tests {
    use super::is_reserved_device_name;

    #[test]
    fn legacy_device_names_are_reserved_whatever_their_case_extension_or_padding() {
        for name in [
            "CON",
            "con",
            "Prn",
            "AUX",
            "NUL",
            "nul.txt",
            "COM1.rml",
            "com9",
            "LPT1",
            "lpt9.x.y",
            "NUL.",
            "NUL ",
            "Con .x",
            "NUL:stream",
            "COM\u{b9}",
            "COM\u{b2}",
            "COM\u{b3}",
            "LPT\u{b9}.txt",
            "CONIN$",
            "conout$",
        ] {
            assert!(is_reserved_device_name(name), "{name:?}");
        }
    }

    #[test]
    fn ordinary_names_that_only_resemble_a_device_are_not_reserved() {
        for name in [
            "", "x", "CONSOLE", "COM0", "COM10", "LPT0", "COM", "NULL", "aux1", "xnul", "COM1x",
            ".nul", "My CON",
        ] {
            assert!(!is_reserved_device_name(name), "{name:?}");
        }
    }

    #[cfg(windows)]
    mod windows_paths {
        use super::super::*;

        fn refusal(raw: &str) -> UserPathError {
            UserPath::parse(raw).expect_err(raw)
        }

        #[test]
        fn a_drive_letter_path_is_accepted() {
            for raw in [
                r"C:\x",
                r"c:\data\me\x",
                r"Z:\mapped\x",
                r"C:/forward/slash",
            ] {
                assert_eq!(UserPath::parse(raw).expect(raw).as_path(), Path::new(raw));
            }
        }

        #[test]
        fn an_extended_length_drive_path_is_accepted() {
            let parsed = UserPath::parse(r"\\?\C:\x").expect("verbatim disk");

            assert_eq!(parsed.into_path_buf(), PathBuf::from(r"\\?\C:\x"));
        }

        #[test]
        fn a_unc_path_is_a_network_path() {
            assert_eq!(refusal(r"\\server\share\x"), UserPathError::NetworkPath);
        }

        #[test]
        fn a_forward_slash_unc_path_is_a_network_path() {
            assert_eq!(refusal("//server/share/x"), UserPathError::NetworkPath);
        }

        #[test]
        fn an_extended_length_unc_path_is_a_network_path() {
            assert_eq!(
                refusal(r"\\?\UNC\server\share\x"),
                UserPathError::NetworkPath
            );
        }

        #[test]
        fn a_webdav_path_is_a_network_path() {
            assert_eq!(refusal(r"\\server@SSL\x"), UserPathError::NetworkPath);
            assert_eq!(
                refusal(r"\\server@SSL\DavWWWRoot\x"),
                UserPathError::NetworkPath
            );
        }

        #[test]
        fn a_device_namespace_path_is_a_device_path() {
            assert_eq!(refusal(r"\\.\pipe\x"), UserPathError::DevicePath);
        }

        #[test]
        fn an_extended_length_device_path_is_a_device_path() {
            for raw in [r"\\?\GLOBALROOT\x", r"\\?\pipe\x", r"\\?\CONIN$"] {
                assert_eq!(refusal(raw), UserPathError::DevicePath, "{raw}");
            }
        }

        #[test]
        fn a_bare_volume_is_a_device_path() {
            for raw in [r"\\?\C:", r"\\?\c:"] {
                assert_eq!(refusal(raw), UserPathError::DevicePath, "{raw}");
            }
        }

        #[test]
        fn a_verbatim_unc_spelled_in_any_case_is_a_network_path() {
            for raw in [r"\\?\unc\server\share\x", r"\\?\Unc\server\share\x"] {
                assert_eq!(refusal(raw), UserPathError::NetworkPath, "{raw}");
            }
        }

        #[test]
        fn a_forward_slash_verbatim_marker_is_a_network_path_as_std_reads_it() {
            // `std` takes the `?` for a server name when the marker is not `\\?\` exactly.
            for raw in ["//?/C:/x", r"\\?/C:\x"] {
                assert_eq!(refusal(raw), UserPathError::NetworkPath, "{raw}");
            }
        }

        #[test]
        fn a_verbatim_drive_with_a_forward_slash_after_it_is_a_drive_path_as_std_reads_it() {
            let parsed = UserPath::parse(r"\\?\C:/x").expect("verbatim disk");

            assert_eq!(parsed.into_path_buf(), PathBuf::from(r"\\?\C:/x"));
        }

        #[test]
        fn a_server_without_a_share_or_with_three_slashes_is_not_absolute() {
            for raw in [r"\\server", r"\\\server\share"] {
                assert_eq!(refusal(raw), UserPathError::NotAbsolute, "{raw}");
            }
        }

        #[test]
        fn a_legacy_device_name_under_a_drive_letter_is_a_device_path() {
            for raw in [
                r"C:\x\COM1.rml",
                r"C:\x\NUL",
                r"C:\CON",
                r"C:\NUL\x.rml",
                r"C:/x/aux.txt",
                r"\\?\C:\x\PRN",
                "C:\\x\\COM\u{b9}.rml",
            ] {
                assert_eq!(refusal(raw), UserPathError::DevicePath, "{raw}");
            }
        }

        #[test]
        fn a_name_that_only_resembles_a_device_is_accepted() {
            for raw in [r"C:\x\console.rml", r"C:\x\COM10.rml", r"C:\NULL"] {
                assert!(UserPath::parse(raw).is_ok(), "{raw}");
            }
        }

        #[test]
        fn a_relative_path_is_not_absolute() {
            for raw in [r"x\y", "x", r"..\x", r"\rooted", r"C:drive-relative"] {
                assert_eq!(refusal(raw), UserPathError::NotAbsolute, "{raw}");
            }
        }

        #[test]
        fn an_empty_path_is_empty() {
            assert_eq!(refusal(""), UserPathError::Empty);
            assert_eq!(refusal("  \t"), UserPathError::Empty);
        }

        #[test]
        fn a_nul_character_is_refused_before_anything_else() {
            assert_eq!(refusal("C:\\x\0y"), UserPathError::ContainsNul);
            assert_eq!(refusal("\\\\server\\x\0"), UserPathError::ContainsNul);
        }
    }
}
