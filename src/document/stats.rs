//! Live text statistics for the viewer/editor.
//!
//! Pure computation over a borrowed `&str` — no copies of plaintext are
//! retained. The cache is keyed by the buffer's (pointer, length), which
//! changes on any document switch and on any edit that changes length;
//! a same-length in-place edit refreshes on the next length change.

/// Counts derived from the document text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextStats {
    pub chars: usize,
    pub chars_no_ws: usize,
    pub words: usize,
    pub lines: usize,
    /// Estimated reading time in whole seconds (≈220 wpm).
    pub reading_secs: u64,
}

impl TextStats {
    pub fn compute(text: &str) -> Self {
        let mut chars = 0usize;
        let mut chars_no_ws = 0usize;
        for c in text.chars() {
            chars += 1;
            if !c.is_whitespace() {
                chars_no_ws += 1;
            }
        }
        let words = text.split_whitespace().count();
        let lines = if text.is_empty() {
            0
        } else {
            text.lines().count()
        };
        let reading_secs = ((words as f64 / 220.0) * 60.0).ceil() as u64;
        TextStats {
            chars,
            chars_no_ws,
            words,
            lines,
            reading_secs,
        }
    }

    /// "3 min" / "40 sec" style reading-time label.
    pub fn reading_label(&self) -> String {
        if self.reading_secs < 60 {
            format!("{} sec", self.reading_secs.max(1))
        } else {
            format!("{} min", self.reading_secs.div_ceil(60))
        }
    }
}

/// Compact thousands formatting: 950 → "950", 2140 → "2.1k", 1200000 → "1.2M".
pub fn compact(n: usize) -> String {
    if n < 1000 {
        n.to_string()
    } else if n < 1_000_000 {
        format!("{:.1}k", n as f64 / 1000.0)
    } else {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    }
}

/// Cache of the last computed stats, keyed by buffer identity.
#[derive(Default)]
pub struct StatsCache {
    key: Option<(usize, usize)>,
    value: Option<TextStats>,
}

impl StatsCache {
    /// Stats for `text`, recomputing only when the buffer moved or resized.
    pub fn get(&mut self, text: &str) -> TextStats {
        let key = (text.as_ptr() as usize, text.len());
        if self.key != Some(key) || self.value.is_none() {
            self.value = Some(TextStats::compute(text));
            self.key = Some(key);
        }
        self.value.unwrap_or_else(|| TextStats::compute(text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_basics() {
        let s = TextStats::compute("Hello world\nsecond line here\n");
        assert_eq!(s.words, 5);
        assert_eq!(s.lines, 2);
        assert_eq!(s.chars, 29);
        assert_eq!(s.chars_no_ws, 24);
    }

    #[test]
    fn empty_text() {
        let s = TextStats::compute("");
        assert_eq!(s.words, 0);
        assert_eq!(s.lines, 0);
        assert_eq!(s.chars, 0);
        assert_eq!(s.reading_label(), "1 sec");
    }

    #[test]
    fn unicode_chars_counted_as_chars() {
        let s = TextStats::compute("héllo wörld — ünïcode");
        assert_eq!(s.words, 4);
        assert_eq!(s.chars, "héllo wörld — ünïcode".chars().count());
    }

    #[test]
    fn compact_formatting() {
        assert_eq!(compact(950), "950");
        assert_eq!(compact(2140), "2.1k");
        assert_eq!(compact(1_200_000), "1.2M");
    }

    #[test]
    fn cache_recomputes_on_len_change() {
        let mut cache = StatsCache::default();
        let a = String::from("one two");
        assert_eq!(cache.get(&a).words, 2);
        let b = String::from("one two three");
        assert_eq!(cache.get(&b).words, 3);
    }

    #[test]
    fn reading_time() {
        let text = "word ".repeat(440); // ~2 min at 220wpm
        let s = TextStats::compute(&text);
        assert_eq!(s.reading_label(), "2 min");
    }
}
