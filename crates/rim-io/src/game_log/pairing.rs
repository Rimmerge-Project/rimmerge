//! Pairs each terse `Patch operation ... failed` line with the stack-trace
//! block that describes it.
//!
//! **Why keyed, not positional.** A patch-reporting mod prints its blocks
//! *before* the engine's terse lines (never adjacent to them, never after),
//! and prints a block for every failure it knows of. When one is missing
//! (a logging gap, a block the mod did not print), positional pairing
//! shifts every later block onto the wrong failure without any signal, and
//! the wrong leaf xpath reaches whoever joins the failure to a patch. So a
//! failure claims a block by what identifies it, in file order:
//!
//! 1. **Same startup pass, same mod tag, same file path** (paths compared
//!    lowercased with `\` as `/`), the earliest unclaimed block first.
//! 2. What is still unpaired falls back to **same pass and mod tag**, again
//!    earliest first, but only where a path is missing: a failure and a block
//!    that both name a file must name the same file, so two different paths
//!    never pair. The mod tag alone decides only when either path is
//!    missing, and it is never relaxed: a failure and a block of different
//!    mods stay unpaired.
//!
//! The exact pass runs over the whole log before the fallback, so a
//! fallback pairing never takes a block that a later failure matches
//! exactly. What neither pass claims stays visible: an unclaimed failure
//! has no stack trace, and an unclaimed block is returned for
//! `extra_stack_traces`.

use std::collections::{BTreeMap, VecDeque};

use rim_session::ports::{RawPatchFailure, StackTraceBlock};

use super::extract::TerseFailure;

/// A record with the startup pass it was logged in.
pub(super) struct PassTagged<T> {
    pub(super) pass: u32,
    pub(super) value: T,
}

/// Which block each failure claimed, and which blocks are taken.
struct Claims {
    /// Per failure (in file order): the index of the block it claimed.
    block_of_failure: Vec<Option<usize>>,
    is_block_claimed: Vec<bool>,
}

impl Claims {
    fn new(failure_count: usize, block_count: usize) -> Self {
        Self {
            block_of_failure: vec![None; failure_count],
            is_block_claimed: vec![false; block_count],
        }
    }

    fn claim(&mut self, failure: usize, block: usize) {
        self.block_of_failure[failure] = Some(block);
        self.is_block_claimed[block] = true;
    }
}

/// Lowercased, with `\` as `/`: the same normalization the importer uses
/// to compare log paths with mod folders.
fn normalize_path(path: &str) -> String {
    path.replace('\\', "/").to_lowercase()
}

/// Pairs `failures` with `blocks` (both in file order) as described in the
/// module documentation. Returns the failures with their blocks attached,
/// and the blocks no failure claimed, in file order.
pub(super) fn pair_failures(
    failures: Vec<PassTagged<TerseFailure>>,
    blocks: Vec<PassTagged<StackTraceBlock>>,
) -> (Vec<RawPatchFailure>, Vec<StackTraceBlock>) {
    let mut claims = Claims::new(failures.len(), blocks.len());
    claim_by_path(&failures, &blocks, &mut claims);
    claim_by_tag(&failures, &blocks, &mut claims);

    let mut slots: Vec<Option<StackTraceBlock>> = blocks
        .into_iter()
        .map(|tagged| Some(tagged.value))
        .collect();
    let paired = failures
        .into_iter()
        .zip(claims.block_of_failure)
        .map(|(tagged, block)| RawPatchFailure {
            mod_tag: tagged.value.mod_tag,
            operation: tagged.value.operation,
            source_file: tagged.value.source_file,
            stack_trace: block.and_then(|index| slots[index].take()),
        })
        .collect();
    (paired, slots.into_iter().flatten().collect())
}

/// Phase 1: a failure takes the earliest unclaimed block with its pass,
/// tag and normalized path.
fn claim_by_path(
    failures: &[PassTagged<TerseFailure>],
    blocks: &[PassTagged<StackTraceBlock>],
    claims: &mut Claims,
) {
    let mut lanes: BTreeMap<(u32, &str, String), VecDeque<usize>> = BTreeMap::new();
    for (index, block) in blocks.iter().enumerate() {
        if let Some(path) = &block.value.source_file {
            let key = (
                block.pass,
                block.value.mod_tag.as_str(),
                normalize_path(path),
            );
            lanes.entry(key).or_default().push_back(index);
        }
    }
    for (index, failure) in failures.iter().enumerate() {
        let Some(path) = &failure.value.source_file else {
            continue;
        };
        let key = (
            failure.pass,
            failure.value.mod_tag.as_str(),
            normalize_path(path),
        );
        if let Some(block) = lanes.get_mut(&key).and_then(VecDeque::pop_front) {
            claims.claim(index, block);
        }
    }
}

/// The blocks of one (pass, tag), earliest first.
#[derive(Default)]
struct TagLane {
    /// Every block, whatever it names.
    all: VecDeque<usize>,
    /// The blocks with no trailer source-file line.
    without_path: VecDeque<usize>,
}

/// The front of `queue` that is still unclaimed, taken off it.
fn pop_unclaimed(queue: &mut VecDeque<usize>, claims: &Claims) -> Option<usize> {
    while let Some(block) = queue.pop_front() {
        if !claims.is_block_claimed[block] {
            return Some(block);
        }
    }
    None
}

/// Phase 2: an unpaired failure takes the earliest unclaimed block of its
/// pass and tag, unless both name a file (the files then differ, or phase 1
/// would have paired them: a failure's own file has no unclaimed block
/// left). So a failure that names a file may only take a block without one,
/// and a failure without one may take any.
fn claim_by_tag(
    failures: &[PassTagged<TerseFailure>],
    blocks: &[PassTagged<StackTraceBlock>],
    claims: &mut Claims,
) {
    let mut lanes: BTreeMap<(u32, &str), TagLane> = BTreeMap::new();
    for (index, block) in blocks.iter().enumerate() {
        let lane = lanes
            .entry((block.pass, block.value.mod_tag.as_str()))
            .or_default();
        lane.all.push_back(index);
        if block.value.source_file.is_none() {
            lane.without_path.push_back(index);
        }
    }
    for (index, failure) in failures.iter().enumerate() {
        if claims.block_of_failure[index].is_some() {
            continue;
        }
        let Some(lane) = lanes.get_mut(&(failure.pass, failure.value.mod_tag.as_str())) else {
            continue;
        };
        let queue = if failure.value.source_file.is_some() {
            &mut lane.without_path
        } else {
            &mut lane.all
        };
        if let Some(block) = pop_unclaimed(queue, claims) {
            claims.claim(index, block);
        }
    }
}
