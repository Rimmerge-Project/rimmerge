//! Parses the `rimmerge.json` marker Rimmerge writes into the root of
//! every mod folder it generates (the profile merge mod, or an exported
//! compat patch). Pure bytes-in, `Option` out — no filesystem; `infra::discovery`
//! decides what a missing file, a read failure, or a `None` parse means.

use std::collections::BTreeSet;

use serde::Deserialize;

use crate::domain::{GeneratedKind, GeneratedMarker, ModId};

/// `rimmerge.json`'s wire shape, camelCase like every other field
/// Rimmerge writes into it (`generatedAt`, `decisionsSha256`, ...). Only
/// the fields this parser cares about are modeled; the rest (read by
/// other consumers, or not yet written) are ignored rather than
/// `deny_unknown_fields` — a mod's own marker file is never a strict
/// contract.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawMarker {
    kind: Option<String>,
    /// `patchId` for a compat patch; `assignmentId` for an assignment project
    /// — both land in the same [`GeneratedMarker::patch_id`] slot, so this
    /// field accepts either wire name rather than the marker struct growing a
    /// second, nearly identical one. A mod is never both kinds at once, so
    /// there is no ambiguity in accepting both unconditionally. A hand-edited
    /// file carrying **both** keys is not "last one wins" or "first one
    /// wins": serde's derived deserializer treats a second occurrence of an
    /// aliased field within the same object as a duplicate-field error, so
    /// `serde_json::from_slice` fails and [`parse`]'s `.ok()?` turns the
    /// whole marker into `None`, the same as any other malformed file —
    /// pinned by `both_patch_id_and_assignment_id_present_is_none` below, not
    /// left as an assumption.
    #[serde(alias = "assignmentId")]
    patch_id: Option<String>,
    /// Raw text, not `ModId` — `ModId`'s `Deserialize` is `#[serde(transparent)]`
    /// over the inner `String` and therefore does **not** go through
    /// [`ModId::new`]'s lowercasing, unlike every other `ModId` this crate
    /// builds. Normalizing happens explicitly in [`parse`] instead, so a
    /// mixed-case scope entry in a hand-edited or foreign-tool-written
    /// `rimmerge.json` still compares equal to the same mod's lowercase id
    /// everywhere downstream.
    scope: Option<BTreeSet<String>>,
}

/// Parses one mod folder's `rimmerge.json` marker contents. `None` for
/// anything this version doesn't recognize as a marker: malformed JSON, or
/// a `kind` other than `"merge"`/`"patch"`/`"assignment"` (a future marker
/// kind, or garbage a user put in the folder — never a panic, never an
/// error, a user can put anything in a folder). A missing `kind` reads as
/// [`GeneratedKind::Merge`] — the shape written by a merge mod generated
/// from before this field existed. Every `scope` entry is normalized through
/// [`ModId::new`] (trimmed, lowercased) here at the parse boundary, exactly
/// like every other `ModId` this crate constructs — see [`RawMarker::scope`]'s
/// own doc comment for why that can't just be `serde`'s job.
#[must_use]
pub fn parse(bytes: &[u8]) -> Option<GeneratedMarker> {
    let raw: RawMarker = serde_json::from_slice(bytes).ok()?;
    let kind = match raw.kind.as_deref() {
        None | Some("merge") => GeneratedKind::Merge,
        Some("patch") => GeneratedKind::Patch,
        Some("assignment") => GeneratedKind::Assignment,
        Some(_) => return None,
    };
    let scope = raw
        .scope
        .map(|ids| ids.iter().map(ModId::new).collect::<BTreeSet<ModId>>());
    Some(GeneratedMarker {
        kind,
        patch_id: raw.patch_id,
        scope,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_with_no_kind_field_reads_as_merge() {
        let json = br#"{"generatedAt":"2026-01-01T00:00:00Z","profileHash":"abc","decisionsSha256":"def","rimmergeVersion":"0.1.0"}"#;

        let marker = parse(json).expect("must parse a marker with no kind field");

        assert_eq!(marker.kind, GeneratedKind::Merge);
        assert_eq!(marker.patch_id, None);
        assert_eq!(marker.scope, None);
    }

    #[test]
    fn explicit_merge_kind_reads_as_merge() {
        let json = br#"{"kind":"merge","profileHash":"abc","decisionsSha256":"def","rimmergeVersion":"0.1.0"}"#;

        let marker = parse(json).expect("must parse");

        assert_eq!(marker.kind, GeneratedKind::Merge);
    }

    #[test]
    fn patch_kind_carries_its_patch_id_and_scope() {
        let json = br#"{"kind":"patch","patchId":"3f9a1c02be77","profileHash":"abc",
            "scope":["fixture.moda","fixture.modb"],"decisionsSha256":"def",
            "rimmergeVersion":"0.1.0"}"#;

        let marker = parse(json).expect("must parse a patch marker");

        assert_eq!(marker.kind, GeneratedKind::Patch);
        assert_eq!(marker.patch_id.as_deref(), Some("3f9a1c02be77"));
        assert_eq!(
            marker.scope,
            Some(BTreeSet::from([
                ModId::new("fixture.moda"),
                ModId::new("fixture.modb"),
            ]))
        );
    }

    /// `ModId`'s `Deserialize` is `#[serde(transparent)]` and does not
    /// lowercase on its own (see [`RawMarker::scope`]'s doc comment) — this
    /// is the boundary that must, and does, normalize a mixed-case scope
    /// entry so it compares equal to the same mod's canonical lowercase id.
    #[test]
    fn scope_entries_are_normalized_to_lowercase() {
        let json = br#"{"kind":"patch","patchId":"3f9a1c02be77","profileHash":"abc",
            "scope":["Fixture.ModA","FIXTURE.MODB"],"decisionsSha256":"def",
            "rimmergeVersion":"0.1.0"}"#;

        let marker = parse(json).expect("must parse a patch marker");

        assert_eq!(
            marker.scope,
            Some(BTreeSet::from([
                ModId::new("fixture.moda"),
                ModId::new("fixture.modb"),
            ]))
        );
    }

    #[test]
    fn assignment_kind_carries_its_assignment_id_in_the_patch_id_slot_and_scope() {
        let json =
            br#"{"kind":"assignment","assignmentId":"example-race-groups","profileHash":"abc",
            "scope":["fixture.example","fixture.target"],"decisionsSha256":"def",
            "rimmergeVersion":"0.1.0"}"#;

        let marker = parse(json).expect("must parse an assignment marker");

        assert_eq!(marker.kind, GeneratedKind::Assignment);
        assert_eq!(marker.patch_id.as_deref(), Some("example-race-groups"));
        assert_eq!(
            marker.scope,
            Some(BTreeSet::from([
                ModId::new("fixture.example"),
                ModId::new("fixture.target"),
            ]))
        );
    }

    #[test]
    fn assignment_kind_with_no_scope_reads_as_unscoped() {
        let json =
            br#"{"kind":"assignment","assignmentId":"example-race-groups","rimmergeVersion":"0.1.0"}"#;

        let marker = parse(json).expect("must parse an assignment marker with no scope");

        assert_eq!(marker.kind, GeneratedKind::Assignment);
        assert_eq!(marker.patch_id.as_deref(), Some("example-race-groups"));
        assert_eq!(marker.scope, None);
    }

    /// A hand-edited file carrying **both** `patchId` and `assignmentId`
    /// is not resolved by "last one wins": serde's derived deserializer
    /// rejects a second occurrence of an aliased field as a duplicate, so
    /// the whole object fails to parse and [`parse`] reports it the same
    /// as any other malformed marker — `None`, never a panic. Pinned here
    /// per [`RawMarker::patch_id`]'s own doc comment, rather than left as
    /// an assumption about what serde does.
    #[test]
    fn both_patch_id_and_assignment_id_present_is_none() {
        let json =
            br#"{"kind":"assignment","patchId":"a","assignmentId":"b","rimmergeVersion":"0.1.0"}"#;

        assert!(parse(json).is_none());
    }

    #[test]
    fn unknown_kind_is_none_not_an_error() {
        let json = br#"{"kind":"something_future","rimmergeVersion":"0.1.0"}"#;

        assert!(parse(json).is_none());
    }

    #[test]
    fn malformed_json_is_none_not_an_error() {
        assert!(parse(b"{ this is not json").is_none());
    }

    #[test]
    fn empty_object_reads_as_an_unscoped_merge_marker() {
        let marker = parse(b"{}").expect("must parse");

        assert_eq!(marker.kind, GeneratedKind::Merge);
        assert_eq!(marker.scope, None);
    }
}
