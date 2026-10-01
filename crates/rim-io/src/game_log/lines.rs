//! Splits a byte stream into bounded, lossily decoded lines, one at a time,
//! into a single reused buffer — the streaming half of [`super::parse`].
//!
//! **Line splitting is load-bearing** (see the parent module's header): a
//! line ends at `\n`, one `\r` directly before that `\n` is stripped, and a
//! lone `\r` never splits. A final segment with bytes but no `\n` is a line
//! too; an empty one is not.
//!
//! Two per-line bounds, both reported on the returned [`ReadLine`] instead
//! of refusing anything: a line longer than [`MAX_LINE_BYTES`] keeps its
//! head and drops the rest (the reader still consumes every byte, so memory
//! never depends on line length), and bytes that are not valid UTF-8 become
//! replacement characters. Invalid bytes are counted only within the kept
//! head, since the dropped tail is never decoded.

use std::borrow::Cow;
use std::io::{self, BufRead};

/// How much of one line is kept for classification. Real logs hold lines
/// of up to ~735 KB (an engine dump listing every allowed thing); every
/// shape this parser recognizes is far shorter than 64 KiB, so the head
/// classifies the line.
pub const MAX_LINE_BYTES: usize = 64 * 1024;

/// One decoded line, borrowed from the reader's buffer until the next
/// call.
pub(super) struct ReadLine<'a> {
    /// The kept head of the line, without its line ending.
    pub(super) text: Cow<'a, str>,
    /// The line was longer than [`MAX_LINE_BYTES`] and its tail was
    /// dropped.
    pub(super) was_truncated: bool,
    /// Decoding the kept head replaced at least one invalid byte.
    pub(super) had_invalid_utf8: bool,
}

/// Reads lines from a [`BufRead`] with memory bounded by
/// [`MAX_LINE_BYTES`].
pub(super) struct BoundedLines<Reader> {
    reader: Reader,
    buffer: Vec<u8>,
}

impl<Reader: BufRead> BoundedLines<Reader> {
    /// Wraps `reader`.
    pub(super) fn new(reader: Reader) -> Self {
        Self {
            reader,
            buffer: Vec::new(),
        }
    }

    /// The next line, or `None` at end of input.
    ///
    /// # Errors
    ///
    /// Returns the reader's I/O error.
    pub(super) fn next_line(&mut self) -> io::Result<Option<ReadLine<'_>>> {
        self.buffer.clear();
        let Some(consumed) = self.fill_buffer()? else {
            return Ok(None);
        };
        let (kept_len, was_truncated) = self.settle_line_end(consumed);
        self.buffer.truncate(kept_len);
        let text = String::from_utf8_lossy(&self.buffer);
        let had_invalid_utf8 = matches!(text, Cow::Owned(_));
        Ok(Some(ReadLine {
            text,
            was_truncated,
            had_invalid_utf8,
        }))
    }

    /// Reads up to and including the next `\n`, storing at most
    /// `MAX_LINE_BYTES + 1` bytes (the extra one leaves room for a `\r`
    /// that a CRLF ending adds). `None` when the input is exhausted with
    /// nothing read.
    fn fill_buffer(&mut self) -> io::Result<Option<LineExtent>> {
        let mut total_len = 0usize;
        let mut read_anything = false;
        let mut ended_with_newline = false;
        loop {
            let available = match self.reader.fill_buf() {
                Ok(bytes) => bytes,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            };
            if available.is_empty() {
                break;
            }
            read_anything = true;
            let newline_at = available.iter().position(|&byte| byte == b'\n');
            let content = &available[..newline_at.unwrap_or(available.len())];
            let room = (MAX_LINE_BYTES + 1).saturating_sub(self.buffer.len());
            self.buffer
                .extend_from_slice(&content[..content.len().min(room)]);
            total_len += content.len();
            let consume_len = content.len() + usize::from(newline_at.is_some());
            self.reader.consume(consume_len);
            if newline_at.is_some() {
                ended_with_newline = true;
                break;
            }
        }
        Ok(read_anything.then_some(LineExtent {
            total_len,
            ended_with_newline,
        }))
    }

    /// Strips a CRLF's `\r`, then applies the length bound. Returns the
    /// number of bytes of `self.buffer` to keep and whether the line was
    /// truncated.
    fn settle_line_end(&mut self, extent: LineExtent) -> (usize, bool) {
        let nothing_dropped = extent.total_len == self.buffer.len();
        let mut content_len = extent.total_len;
        let mut kept_len = self.buffer.len();
        if extent.ended_with_newline && nothing_dropped && self.buffer.last() == Some(&b'\r') {
            content_len -= 1;
            kept_len -= 1;
        }
        if content_len <= MAX_LINE_BYTES {
            return (kept_len, false);
        }
        (self.char_boundary_at_or_below(MAX_LINE_BYTES), true)
    }

    /// `limit`, moved back past the start of a multi-byte character the
    /// cut would otherwise split — so the cut itself never produces an
    /// invalid byte sequence. A cut that lands inside genuinely invalid
    /// bytes is left alone.
    fn char_boundary_at_or_below(&self, limit: usize) -> usize {
        let head = &self.buffer[..limit];
        match std::str::from_utf8(head) {
            Err(error) if error.error_len().is_none() => error.valid_up_to(),
            Ok(_) | Err(_) => limit,
        }
    }
}

/// What [`BoundedLines::fill_buffer`] saw of one line.
#[derive(Clone, Copy)]
struct LineExtent {
    /// The line's byte length before its `\n`, kept or not.
    total_len: usize,
    ended_with_newline: bool,
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    /// Every line of `bytes` as `(text, was_truncated, had_invalid_utf8)`.
    fn collect(bytes: &[u8]) -> Vec<(String, bool, bool)> {
        let mut lines = BoundedLines::new(Cursor::new(bytes.to_vec()));
        let mut out = Vec::new();
        while let Some(line) = lines.next_line().expect("in-memory read cannot fail") {
            out.push((
                line.text.into_owned(),
                line.was_truncated,
                line.had_invalid_utf8,
            ));
        }
        out
    }

    fn texts(bytes: &[u8]) -> Vec<String> {
        collect(bytes).into_iter().map(|(text, ..)| text).collect()
    }

    #[test]
    fn it_splits_like_str_lines_on_a_mixed_input() {
        let inputs: [&str; 8] = [
            "",
            "a",
            "a\n",
            "a\n\n",
            "a\r\nb\r\n",
            "one\rstill one\ntwo\n",
            "no newline at end\r",
            "\n\n\r\n",
        ];
        for input in inputs {
            let expected: Vec<String> = input.lines().map(str::to_string).collect();
            assert_eq!(texts(input.as_bytes()), expected, "input {input:?}");
        }
    }

    #[test]
    fn a_lone_carriage_return_does_not_split_a_line() {
        assert_eq!(
            texts(b"line one\rstill line one\nline two\n"),
            vec!["line one\rstill line one", "line two"]
        );
    }

    #[test]
    fn only_one_trailing_carriage_return_is_stripped() {
        assert_eq!(texts(b"a\r\r\n"), vec!["a\r"]);
    }

    #[test]
    fn a_line_over_the_bound_keeps_its_head_and_is_flagged() {
        let mut bytes = vec![b'x'; MAX_LINE_BYTES + 1000];
        bytes.extend_from_slice(b"\nnext\n");
        let lines = collect(&bytes);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].0.len(), MAX_LINE_BYTES);
        assert!(lines[0].1, "must be flagged truncated");
        assert_eq!(lines[1], ("next".to_string(), false, false));
    }

    #[test]
    fn a_line_exactly_at_the_bound_is_not_truncated_with_or_without_crlf() {
        for ending in ["\n", "\r\n"] {
            let mut bytes = vec![b'x'; MAX_LINE_BYTES];
            bytes.extend_from_slice(ending.as_bytes());
            let lines = collect(&bytes);
            assert_eq!(lines.len(), 1, "ending {ending:?}");
            assert_eq!(lines[0].0.len(), MAX_LINE_BYTES, "ending {ending:?}");
            assert!(!lines[0].1, "ending {ending:?} must not count as truncated");
        }
    }

    #[test]
    fn one_byte_over_the_bound_is_truncated() {
        let mut bytes = vec![b'x'; MAX_LINE_BYTES + 1];
        bytes.push(b'\n');
        let lines = collect(&bytes);
        assert!(lines[0].1);
        assert_eq!(lines[0].0.len(), MAX_LINE_BYTES);
    }

    #[test]
    fn a_cut_inside_a_multibyte_character_is_not_reported_as_invalid_utf8() {
        // 'é' is two bytes; put its first byte at the last kept position.
        let mut bytes = vec![b'x'; MAX_LINE_BYTES - 1];
        bytes.extend_from_slice("é".as_bytes());
        bytes.extend_from_slice(b"tail\n");
        let lines = collect(&bytes);
        assert!(lines[0].1, "truncated");
        assert!(!lines[0].2, "the cut itself must not read as invalid UTF-8");
        assert_eq!(lines[0].0.len(), MAX_LINE_BYTES - 1);
    }

    #[test]
    fn an_invalid_byte_is_replaced_and_flagged_on_its_line_only() {
        let lines = collect(b"good\nba\xFFd\nalso good\n");
        assert_eq!(lines[0], ("good".to_string(), false, false));
        assert_eq!(lines[1], ("ba\u{FFFD}d".to_string(), false, true));
        assert_eq!(lines[2], ("also good".to_string(), false, false));
    }

    #[test]
    fn a_reader_that_yields_one_byte_at_a_time_splits_identically() {
        struct OneByte(Vec<u8>, usize);
        impl io::Read for OneByte {
            fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
                let Some(&byte) = self.0.get(self.1) else {
                    return Ok(0);
                };
                out[0] = byte;
                self.1 += 1;
                Ok(1)
            }
        }
        let input = b"ab\r\ncd\n\nlast".to_vec();
        let mut lines = BoundedLines::new(io::BufReader::with_capacity(1, OneByte(input, 0)));
        let mut out = Vec::new();
        while let Some(line) = lines.next_line().expect("read") {
            out.push(line.text.into_owned());
        }
        assert_eq!(out, vec!["ab", "cd", "", "last"]);
    }
}
