//! The bounded, incremental reader behind every mod-list source.
//!
//! One entry point, [`read_mod_list`], serves a file and the CLI's stdin.
//! It reads in fixed chunks into one buffer that never exceeds
//! `MAX_INPUT_BYTES + 1`, and decides from the first bytes (`sniff.rs`)
//! what it is reading:
//!
//! - a list (`.rml`, `ModsConfig.xml`, text): the whole input up to the
//!   budget, then parsed, exactly as before;
//! - a RimWorld save: only its header. Reading stops at the first
//!   `</meta>`, at most one chunk past it, so a 200 MB save costs a few
//!   hundred kilobytes and nothing of its body is decoded or parsed.
//!
//! Every read fills a whole chunk (a slow pipe is read until the chunk is
//! full or the input ends), and the shape must be settled within the first
//! chunk. Together that bounds the sniffing to two passes over at most
//! 64 KiB, however small the source's own reads are.

use std::io::{self, Read};

use rim_session::mod_list::{ModListLimits, Rejection};
use rim_session::ports::ModListRead;

use super::read::parse_bytes;
use super::save_header::parse_save_header;
use super::sniff::{Sniff, find, sniff};

/// How many bytes one read asks for.
const CHUNK_BYTES: usize = 64 * 1024;
/// A shape still undecided after this many bytes is read as a list.
const SNIFF_WINDOW_BYTES: usize = CHUNK_BYTES;
const META_END: &[u8] = b"</meta>";

/// What the input has shown it is.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Shape {
    Undecided,
    /// Anything but a save: parsed whole.
    List,
    /// A save: only its header is read. `root_at` is where its root
    /// element starts in the buffer.
    Save {
        root_at: usize,
    },
}

/// Reads a mod list from `reader`: the whole input up to
/// `ModListLimits::MAX_INPUT_BYTES` for a list, or a save's header only.
/// Public so an interface that reads a list from somewhere other than a
/// file (the CLI's stdin) is bounded by the same constant.
///
/// # Errors
///
/// An I/O error from `reader`. A document that is not an importable list
/// is an `Ok(ModListRead::Rejected(..))`.
pub fn read_mod_list(mut reader: impl Read) -> io::Result<ModListRead> {
    let budget = ModListLimits::MAX_INPUT_BYTES + 1;
    let mut buffer: Vec<u8> = Vec::new();
    let mut shape = Shape::Undecided;
    let mut searched_to: usize = 0;
    loop {
        let is_budget_spent = buffer.len() >= budget;
        // The end of the input is learned once and never read past: a
        // terminal stdin would otherwise need its end-of-file typed twice.
        let is_input_over = if is_budget_spent {
            false
        } else {
            append_chunk(&mut reader, &mut buffer, budget)?
        };
        if shape == Shape::Undecided {
            let is_sniff_over =
                is_input_over || is_budget_spent || buffer.len() >= SNIFF_WINDOW_BYTES;
            match decide(&buffer, is_sniff_over, is_input_over) {
                Decision::Reject(rejection) => return Ok(ModListRead::Rejected(rejection)),
                Decision::Shape(decided) => shape = decided,
            }
        }
        match shape {
            Shape::Undecided => {}
            Shape::List => {
                if is_budget_spent {
                    return Ok(ModListRead::Rejected(Rejection::TooLarge {
                        limit_bytes: ModListLimits::MAX_INPUT_BYTES,
                    }));
                }
                if is_input_over {
                    return Ok(finish(parse_bytes(&buffer)));
                }
            }
            Shape::Save { root_at } => {
                // A `</meta>` can straddle two chunks; nothing before the
                // root can hold one that counts.
                let from = searched_to.saturating_sub(META_END.len() - 1).max(root_at);
                if let Some(at) = find(&buffer[from..], META_END) {
                    let end = from + at + META_END.len();
                    // The buffer holds one byte more than the limit so an
                    // over-long input is noticed; a header ending in that
                    // byte is over the limit too.
                    if end > ModListLimits::MAX_INPUT_BYTES {
                        return Ok(ModListRead::Rejected(Rejection::SaveHeaderTooLarge {
                            limit_bytes: ModListLimits::MAX_INPUT_BYTES,
                        }));
                    }
                    return Ok(finish(parse_save_header(&buffer[..end], root_at)));
                }
                searched_to = buffer.len();
                if is_budget_spent {
                    return Ok(ModListRead::Rejected(Rejection::SaveHeaderTooLarge {
                        limit_bytes: ModListLimits::MAX_INPUT_BYTES,
                    }));
                }
                if is_input_over {
                    return Ok(ModListRead::Rejected(Rejection::SaveHeaderTruncated));
                }
            }
        }
    }
}

enum Decision {
    Reject(Rejection),
    Shape(Shape),
}

/// What the buffer so far shows. When no more can be waited for (the input
/// ended or the sniff window is spent) an undecided sniff means a list (the
/// text format, or a document too short to tell), and a save root without a
/// first child is a truncated save (input ended) or one with no header.
fn decide(buffer: &[u8], is_sniff_over: bool, is_input_over: bool) -> Decision {
    match sniff(buffer) {
        Sniff::NeedMore if is_sniff_over => Decision::Shape(Shape::List),
        Sniff::NeedMore => Decision::Shape(Shape::Undecided),
        Sniff::SavePending if is_input_over => Decision::Reject(Rejection::SaveHeaderTruncated),
        Sniff::SavePending if is_sniff_over => Decision::Reject(Rejection::SaveHeaderMissing),
        Sniff::SavePending => Decision::Shape(Shape::Undecided),
        Sniff::NotSave => Decision::Shape(Shape::List),
        Sniff::SaveHeader { root_at } => Decision::Shape(Shape::Save { root_at }),
        Sniff::DocType => Decision::Reject(Rejection::DtdNotAllowed),
        Sniff::SaveWithoutHeader => Decision::Reject(Rejection::SaveHeaderMissing),
    }
}

fn finish(parsed: Result<rim_session::mod_list::ParsedModList, Rejection>) -> ModListRead {
    match parsed {
        Ok(parsed) => ModListRead::Parsed(parsed),
        Err(rejection) => ModListRead::Rejected(rejection),
    }
}

/// Appends up to [`CHUNK_BYTES`] bytes, reading until the chunk is full or
/// the input ends, and never growing the buffer past `budget`. Returns
/// whether the input ended (a read answered `0`); it is never read again
/// after that.
fn append_chunk(reader: &mut impl Read, buffer: &mut Vec<u8>, budget: usize) -> io::Result<bool> {
    let start = buffer.len();
    let want = CHUNK_BYTES.min(budget - start);
    buffer.resize(start + want, 0);
    let mut filled = 0;
    let mut is_input_over = false;
    while filled < want {
        match reader.read(&mut buffer[start + filled..]) {
            Ok(0) => {
                is_input_over = true;
                break;
            }
            Ok(read) => filled += read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => {
                buffer.truncate(start);
                return Err(error);
            }
        }
    }
    buffer.truncate(start + filled);
    Ok(is_input_over)
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use super::*;

    /// An endless reader of `b'a'` that counts the bytes handed out.
    struct CountingReader(Rc<Cell<usize>>);

    impl Read for CountingReader {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            buf.fill(b'a');
            self.0.set(self.0.get() + buf.len());
            Ok(buf.len())
        }
    }

    /// Hands out `head`, then endless `b'x'`, counting every byte given.
    struct HeadThenEndless {
        head: Vec<u8>,
        given: Rc<Cell<usize>>,
    }

    impl Read for HeadThenEndless {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let from_head = self.head.len().min(buf.len());
            buf[..from_head].copy_from_slice(&self.head[..from_head]);
            self.head.drain(..from_head);
            buf[from_head..].fill(b'x');
            self.given.set(self.given.get() + buf.len());
            Ok(buf.len())
        }
    }

    /// One byte per read, whatever is asked.
    struct OneByteAtATime<'a>(&'a [u8]);

    impl Read for OneByteAtATime<'_> {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let Some((first, rest)) = self.0.split_first() else {
                return Ok(0);
            };
            buf[0] = *first;
            self.0 = rest;
            Ok(1)
        }
    }

    const HEADER: &str = "<savegame><meta><gameVersion>1.6.1 rev1</gameVersion>\
        <modIds><li>example.alpha</li></modIds></meta>";

    fn parsed_ids(read: ModListRead) -> Vec<String> {
        match read {
            ModListRead::Parsed(parsed) => parsed
                .list
                .entries()
                .iter()
                .map(|entry| entry.id.as_str().to_string())
                .collect(),
            ModListRead::Rejected(rejection) => panic!("rejected: {rejection:?}"),
        }
    }

    #[test]
    fn an_endless_list_is_cut_off_one_byte_past_the_limit() {
        let handed_out = Rc::new(Cell::new(0));

        let result = read_mod_list(CountingReader(Rc::clone(&handed_out))).expect("reads");

        assert_eq!(
            result,
            ModListRead::Rejected(Rejection::TooLarge {
                limit_bytes: ModListLimits::MAX_INPUT_BYTES
            })
        );
        assert_eq!(handed_out.get(), ModListLimits::MAX_INPUT_BYTES + 1);
    }

    #[test]
    fn a_list_exactly_at_the_limit_is_parsed_not_refused() {
        let mut bytes = b"ludeon.rimworld\n".to_vec();
        bytes.resize(ModListLimits::MAX_INPUT_BYTES, b'\n');

        let result = read_mod_list(bytes.as_slice()).expect("reads");

        assert_eq!(parsed_ids(result), ["ludeon.rimworld"]);
    }

    #[test]
    fn a_save_is_read_only_up_to_one_chunk_past_its_header() {
        let given = Rc::new(Cell::new(0));
        let reader = HeadThenEndless {
            head: HEADER.as_bytes().to_vec(),
            given: Rc::clone(&given),
        };

        let result = read_mod_list(reader).expect("reads");

        assert_eq!(parsed_ids(result), ["example.alpha"]);
        assert!(given.get() <= HEADER.len() + CHUNK_BYTES, "{}", given.get());
    }

    #[test]
    fn a_header_end_split_across_reads_is_found() {
        let result = read_mod_list(OneByteAtATime(HEADER.as_bytes())).expect("reads");

        assert_eq!(parsed_ids(result), ["example.alpha"]);
    }

    #[test]
    fn a_header_end_is_found_wherever_the_chunks_split() {
        let mut bytes = HEADER.as_bytes().to_vec();
        bytes.extend_from_slice(b"<game>never read");
        for split in 1..bytes.len() {
            let (first, second) = bytes.split_at(split);
            let result = read_mod_list(first.chain(second)).expect("reads");
            assert_eq!(parsed_ids(result), ["example.alpha"], "split at {split}");
        }
    }

    #[test]
    fn a_header_that_never_ends_is_too_large_after_exactly_the_budget() {
        let given = Rc::new(Cell::new(0));
        let reader = HeadThenEndless {
            head: b"<savegame><meta>".to_vec(),
            given: Rc::clone(&given),
        };

        let result = read_mod_list(reader).expect("reads");

        assert_eq!(
            result,
            ModListRead::Rejected(Rejection::SaveHeaderTooLarge {
                limit_bytes: ModListLimits::MAX_INPUT_BYTES
            })
        );
        assert_eq!(given.get(), ModListLimits::MAX_INPUT_BYTES + 1);
    }

    #[test]
    fn a_save_that_ends_before_its_header_does_is_truncated() {
        let result =
            read_mod_list(b"<savegame><meta><modIds><li>example.a".as_slice()).expect("reads");

        assert_eq!(
            result,
            ModListRead::Rejected(Rejection::SaveHeaderTruncated)
        );
    }

    struct Failing;

    impl Read for Failing {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("disk gone"))
        }
    }

    #[test]
    fn a_read_error_is_an_error_not_a_rejection() {
        assert!(read_mod_list(Failing).is_err());
    }

    #[test]
    fn a_read_error_after_a_save_header_is_under_way_is_an_error() {
        // Padded past the first chunk, so the shape was decided and more
        // of the header was being read when the source failed.
        let mut prefix = b"<savegame><meta><modIds>".to_vec();
        prefix.resize(CHUNK_BYTES + 16, b' ');
        let reader = prefix.as_slice().chain(Failing);

        assert!(read_mod_list(reader).is_err());
    }

    #[test]
    fn a_read_error_after_a_list_prefix_is_an_error() {
        let reader = b"ludeon.rimworld\n".as_slice().chain(Failing);

        assert!(read_mod_list(reader).is_err());
    }

    /// A save whose first `</meta>` ends exactly at byte `end`.
    fn header_ending_at(end: usize) -> Vec<u8> {
        let mut bytes = b"<savegame><meta><modIds><li>example.alpha</li></modIds>".to_vec();
        bytes.resize(end - META_END.len(), b' ');
        bytes.extend_from_slice(META_END);
        bytes
    }

    #[test]
    fn a_header_ending_exactly_at_the_limit_is_read() {
        let bytes = header_ending_at(ModListLimits::MAX_INPUT_BYTES);

        let result = read_mod_list(bytes.as_slice()).expect("reads");

        assert_eq!(parsed_ids(result), ["example.alpha"]);
    }

    #[test]
    fn a_header_ending_one_byte_past_the_limit_is_too_large() {
        let bytes = header_ending_at(ModListLimits::MAX_INPUT_BYTES + 1);

        let result = read_mod_list(bytes.as_slice()).expect("reads");

        assert_eq!(
            result,
            ModListRead::Rejected(Rejection::SaveHeaderTooLarge {
                limit_bytes: ModListLimits::MAX_INPUT_BYTES
            })
        );
    }

    #[test]
    fn a_save_root_that_the_input_ends_after_is_truncated() {
        for bytes in [
            &b"<savegame>"[..],
            b"<savegame>\r\n  ",
            b"<savegame attr=\"x\"",
        ] {
            let result = read_mod_list(bytes).expect("reads");

            assert_eq!(
                result,
                ModListRead::Rejected(Rejection::SaveHeaderTruncated),
                "{bytes:?}"
            );
        }
    }

    #[test]
    fn a_save_whose_root_comes_after_a_leading_comment_naming_it_is_read() {
        let mut bytes = b"<!-- <SAVEGAME -->".to_vec();
        bytes.extend_from_slice(HEADER.as_bytes());

        let result = read_mod_list(bytes.as_slice()).expect("reads");

        assert_eq!(parsed_ids(result), ["example.alpha"]);
    }

    #[test]
    fn a_long_leading_comment_read_a_byte_at_a_time_is_settled_within_the_window() {
        let mut bytes = b"<!--".to_vec();
        bytes.resize(SNIFF_WINDOW_BYTES * 2, b' ');
        bytes.extend_from_slice(b"--><savegame><meta><modIds></modIds></meta></savegame>");

        let result = read_mod_list(OneByteAtATime(&bytes)).expect("reads");

        // Past the sniff window the input is a list, and a list that is
        // XML but not a known root is refused.
        assert_eq!(result, ModListRead::Rejected(Rejection::UnrecognizedFormat));
    }

    #[test]
    fn a_save_root_with_no_first_child_inside_the_window_has_no_header() {
        let mut bytes = b"<savegame>".to_vec();
        bytes.resize(bytes.len() + SNIFF_WINDOW_BYTES, b' ');
        bytes.extend_from_slice(b"<meta><modIds></modIds></meta>");

        let result = read_mod_list(bytes.as_slice()).expect("reads");

        assert_eq!(result, ModListRead::Rejected(Rejection::SaveHeaderMissing));
    }

    /// Data, then one end-of-input answer, then a failure on any later read.
    struct EndsOnce {
        data: &'static [u8],
        has_ended: bool,
    }

    impl Read for EndsOnce {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if !self.data.is_empty() {
                let count = self.data.len().min(buf.len());
                buf[..count].copy_from_slice(&self.data[..count]);
                self.data = &self.data[count..];
                return Ok(count);
            }
            assert!(!self.has_ended, "read again after the end of the input");
            self.has_ended = true;
            Ok(0)
        }
    }

    #[test]
    fn the_end_of_the_input_is_never_read_past() {
        for data in [
            HEADER.as_bytes(),
            b"ludeon.rimworld\n".as_slice(),
            b"<savegame><meta><modIds>".as_slice(),
        ] {
            let reader = EndsOnce {
                data,
                has_ended: false,
            };

            let result = read_mod_list(reader);

            assert!(result.is_ok());
        }
    }
}
