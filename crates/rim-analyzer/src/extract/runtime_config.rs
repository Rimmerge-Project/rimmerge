//! The Waylith engine's `RimWorld.runtimeconfig.json`, read for one fact:
//! the assembly names the engine itself provides.
//!
//! `configProperties."WaylithClr.AssemblyNameRedirects"` is a
//! semicolon-separated list of `pattern=target` pairs, e.g.
//! `UnityEngine*=UnityStubs;Unity.Newtonsoft.Json=;0PrepatcherAPI=RimWorldWaylith.PrepatcherCompat`.
//! A pattern is either an exact assembly name or a prefix ending in `*`.
//! A mod referencing a name matched by either needs no other mod to ship
//! it, exactly like `UnityEngine.CoreModule` on a Unity install.
//!
//! An entry with an **empty target** is an exclusion, not a redirect: the
//! engine leaves that name alone and binds it normally, so a mod must ship
//! it. The real list carves `Unity.Newtonsoft.Json` out of the `Unity.*`
//! entry that follows it this way. Exclusions are applied to the prefixes
//! ([`EngineAssemblyPrefixes::excluding`]) and never reach `exact`.

use std::collections::BTreeSet;

use thiserror::Error;

use crate::domain::EngineAssemblyPrefixes;

/// The `configProperties` key holding the redirect list.
const REDIRECTS_PROPERTY: &str = "WaylithClr.AssemblyNameRedirects";

/// Why the redirect list could not be read.
#[derive(Debug, Error)]
pub enum RuntimeConfigError {
    /// The file is not valid JSON.
    #[error("not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    /// The file has no `runtimeOptions.configProperties` redirect string.
    #[error("no `{REDIRECTS_PROPERTY}` string under runtimeOptions.configProperties")]
    NoRedirects,
}

/// Engine-provided assembly names, split by how they match.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AssemblyRedirects {
    /// Lowercased exact assembly names the engine redirects (non-empty
    /// target).
    pub exact: BTreeSet<String>,
    /// Lowercased `*`-terminated patterns, with the `*` removed, minus the
    /// exact names an empty-target entry excludes.
    pub prefixes: EngineAssemblyPrefixes,
    /// Patterns this reader does not model (a `*` anywhere but the end, a
    /// bare `*`, an empty name, no `=`), kept verbatim so the caller can say
    /// so rather than silently dropping them.
    pub unsupported: Vec<String>,
}

/// Reads the redirect list out of `RimWorld.runtimeconfig.json`'s bytes.
///
/// # Errors
///
/// [`RuntimeConfigError::Json`] for invalid JSON and
/// [`RuntimeConfigError::NoRedirects`] when the property is absent or not a
/// string.
pub fn parse_assembly_redirects(bytes: &[u8]) -> Result<AssemblyRedirects, RuntimeConfigError> {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let document: serde_json::Value = serde_json::from_slice(bytes)?;
    let list = document
        .pointer("/runtimeOptions/configProperties")
        .and_then(|properties| properties.get(REDIRECTS_PROPERTY))
        .and_then(serde_json::Value::as_str)
        .ok_or(RuntimeConfigError::NoRedirects)?;
    Ok(parse_redirect_list(list))
}

fn parse_redirect_list(list: &str) -> AssemblyRedirects {
    let mut redirects = AssemblyRedirects::default();
    let mut prefixes = Vec::new();
    let mut excluded = Vec::new();
    for entry in list.split(';').map(str::trim).filter(|e| !e.is_empty()) {
        let Some((pattern, target)) = entry.split_once('=') else {
            redirects.unsupported.push(entry.to_string());
            continue;
        };
        let pattern = pattern.trim();
        let is_exclusion = target.trim().is_empty();
        match pattern.strip_suffix('*') {
            Some(prefix) if !prefix.is_empty() && !prefix.contains('*') && !is_exclusion => {
                prefixes.push(prefix.to_string());
            }
            None if !pattern.is_empty() && !pattern.contains('*') && is_exclusion => {
                excluded.push(pattern.to_string());
            }
            None if !pattern.is_empty() && !pattern.contains('*') => {
                redirects.exact.insert(pattern.to_lowercase());
            }
            // Includes an empty-target `Prefix*=`: excluding a whole prefix
            // is not modeled, so it is reported rather than guessed.
            _ => redirects.unsupported.push(entry.to_string()),
        }
    }
    redirects.prefixes = EngineAssemblyPrefixes::new(prefixes).excluding(excluded);
    redirects
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_with(redirects: &str) -> String {
        format!(
            r#"{{"runtimeOptions":{{"configProperties":{{"WaylithClr.AssemblyNameRedirects":"{redirects}","Other":true}}}}}}"#
        )
    }

    #[test]
    fn exact_names_and_star_prefixes_are_split_and_lowercased() {
        let json = config_with("UnityEngine*=UnityStubs;Unity.*=UnityStubs;0PrepatcherAPI=Compat");

        let redirects = parse_assembly_redirects(json.as_bytes()).unwrap();

        assert_eq!(
            redirects.exact,
            BTreeSet::from(["0prepatcherapi".to_string()])
        );
        assert!(redirects.prefixes.matches("unityengine.coremodule"));
        assert!(redirects.prefixes.matches("unity.burst"));
        assert!(!redirects.prefixes.matches("unityx"));
        assert!(redirects.unsupported.is_empty());
    }

    #[test]
    fn the_real_redirect_string_excludes_newtonsoft_from_the_unity_prefix() {
        let json = config_with(
            "UnityEngine*=UnityStubs;Unity.Newtonsoft.Json=;Unity.*=UnityStubs;Assembly-CSharp-firstpass=UnityStubs;0PrepatcherAPI=RimWorldWaylith.PrepatcherCompat;PrepatcherImpl=RimWorldWaylith.PrepatcherCompat;Prestarter=RimWorldWaylith.PrepatcherCompat;0PrepatcherDataAssembly=RimWorldWaylith.PrepatcherCompat",
        );

        let redirects = parse_assembly_redirects(json.as_bytes()).unwrap();

        assert!(!redirects.prefixes.matches("unity.newtonsoft.json"));
        assert!(!redirects.exact.contains("unity.newtonsoft.json"));
        assert!(redirects.prefixes.matches("unity.burst"));
        assert!(redirects.prefixes.matches("unityengine.coremodule"));
        assert!(redirects.exact.contains("assembly-csharp-firstpass"));
        assert!(redirects.unsupported.is_empty());
    }

    #[test]
    fn an_exclusion_listed_after_its_prefix_still_wins() {
        let json = config_with("Unity.*=UnityStubs;Unity.Newtonsoft.Json=");

        let redirects = parse_assembly_redirects(json.as_bytes()).unwrap();

        assert!(!redirects.prefixes.matches("unity.newtonsoft.json"));
        assert!(redirects.prefixes.matches("unity.burst"));
    }

    #[test]
    fn a_leading_byte_order_mark_is_tolerated() {
        let mut bytes = b"\xEF\xBB\xBF".to_vec();
        bytes.extend_from_slice(config_with("A=B").as_bytes());

        let redirects = parse_assembly_redirects(&bytes).unwrap();

        assert_eq!(redirects.exact, BTreeSet::from(["a".to_string()]));
    }

    #[test]
    fn patterns_this_reader_cannot_model_are_reported_not_applied() {
        let json = config_with("*=X;Mid*dle=X;NoEquals;=X; Good =X;Pre*=");

        let redirects = parse_assembly_redirects(json.as_bytes()).unwrap();

        assert_eq!(redirects.exact, BTreeSet::from(["good".to_string()]));
        assert!(!redirects.prefixes.matches("anything"));
        assert_eq!(
            redirects.unsupported,
            vec!["*=X", "Mid*dle=X", "NoEquals", "=X", "Pre*="]
        );
    }

    #[test]
    fn invalid_json_and_a_missing_property_are_typed_errors() {
        assert!(matches!(
            parse_assembly_redirects(b"{not json"),
            Err(RuntimeConfigError::Json(_))
        ));
        assert!(matches!(
            parse_assembly_redirects(br#"{"runtimeOptions":{"configProperties":{}}}"#),
            Err(RuntimeConfigError::NoRedirects)
        ));
        assert!(matches!(
            parse_assembly_redirects(
                br#"{"runtimeOptions":{"configProperties":{"WaylithClr.AssemblyNameRedirects":3}}}"#
            ),
            Err(RuntimeConfigError::NoRedirects)
        ));
    }
}
