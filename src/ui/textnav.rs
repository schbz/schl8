//! Text navigation primitives for the viewer: line counting, in-document
//! search, and byte-offset → line mapping.
//!
//! Split out of `app.rs` so the logic is testable without a running egui
//! context. These run over borrowed plaintext (`&str` into a
//! `SecureBuffer`) and must never copy it into an unlocked allocation —
//! see the note on [`find_matches`].

use crate::crypto::secure_buf::SecureBuffer;

/// Most matches [`find_matches`] will report for one query.
pub const MAX_MATCHES: usize = 5_000;

/// Lines in a decrypted buffer, or 0 if it isn't valid text.
pub fn count_lines(content: &SecureBuffer) -> usize {
    content.as_str().map(|s| s.lines().count()).unwrap_or(0)
}

/// ASCII-case-insensitive substring search returning byte offsets.
///
/// Deliberately does not lowercase the haystack: `to_lowercase` would
/// allocate a plain `String` holding a full copy of the plaintext outside
/// the mlock'd buffer, which the security model forbids. Comparing in
/// place costs a little speed and keeps the copy from existing.
///
/// Capped at [`MAX_MATCHES`] so a one-character query in a large document
/// stays cheap.
pub fn find_matches(haystack: &str, needle: &str) -> Vec<usize> {
    let mut out = Vec::new();
    if needle.is_empty() || needle.len() > haystack.len() {
        return out;
    }
    let h = haystack.as_bytes();
    let n = needle.as_bytes();
    let mut i = 0;
    while i + n.len() <= h.len() && out.len() < MAX_MATCHES {
        if h[i..i + n.len()].eq_ignore_ascii_case(n) {
            out.push(i);
        }
        i += 1;
    }
    out
}

/// 0-based line index of a byte offset. Offsets past the end clamp to the
/// last line rather than panicking.
pub fn byte_to_line(text: &str, offset: usize) -> usize {
    text.as_bytes()[..offset.min(text.len())]
        .iter()
        .filter(|b| **b == b'\n')
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_lines_and_handles_non_text() {
        let buf = SecureBuffer::from_bytes(b"one\ntwo\nthree".to_vec());
        assert_eq!(count_lines(&buf), 3);
        // A trailing newline does not invent an extra line.
        assert_eq!(
            count_lines(&SecureBuffer::from_bytes(b"a\nb\n".to_vec())),
            2
        );
        assert_eq!(count_lines(&SecureBuffer::from_bytes(Vec::new())), 0);
        // Non-UTF-8 is 0, not a panic — binary vault entries reach here.
        assert_eq!(
            count_lines(&SecureBuffer::from_bytes(vec![0xff, 0xfe, 0x00])),
            0
        );
    }

    #[test]
    fn finds_overlapping_matches_case_insensitively() {
        assert_eq!(find_matches("Hello hello HELLO", "hello"), vec![0, 6, 12]);
        // Overlapping occurrences are all reported (the search advances
        // one byte at a time, not by the needle's length).
        assert_eq!(find_matches("aaaa", "aa"), vec![0, 1, 2]);
        assert_eq!(find_matches("abc", "z"), Vec::<usize>::new());
    }

    #[test]
    fn find_rejects_degenerate_queries() {
        assert!(find_matches("anything", "").is_empty());
        // A needle longer than the haystack can't match, and must not
        // index out of bounds while proving it.
        assert!(find_matches("ab", "abcdef").is_empty());
        assert!(find_matches("", "a").is_empty());
    }

    #[test]
    fn find_is_capped_so_a_broad_query_stays_cheap() {
        let haystack = "a".repeat(MAX_MATCHES + 500);
        assert_eq!(find_matches(&haystack, "a").len(), MAX_MATCHES);
    }

    #[test]
    fn byte_offsets_map_to_lines_and_clamp() {
        let text = "one\ntwo\nthree";
        assert_eq!(byte_to_line(text, 0), 0);
        assert_eq!(byte_to_line(text, 3), 0, "the newline itself ends line 0");
        assert_eq!(byte_to_line(text, 4), 1);
        assert_eq!(byte_to_line(text, 8), 2);
        // Past the end clamps rather than panicking — a stale match
        // offset after an edit must not take the app down.
        assert_eq!(byte_to_line(text, 9_999), 2);
        assert_eq!(byte_to_line("", 5), 0);
    }
}
