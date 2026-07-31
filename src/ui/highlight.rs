//! Search-match highlighting shared by the plaintext view, the markdown
//! renderer, and the editor.
//!
//! Matching is ASCII-case-insensitive and works directly on the borrowed
//! `&str` — the document is never lowercased into a scratch `String`,
//! which would put plaintext in ordinary unlocked memory.
//!
//! Note: building a `LayoutJob` copies the highlighted text (egui owns
//! the job's string). Renderers therefore only take this path while a
//! search is actually active, so the extra transient copy exists only
//! during a find — not for every frame of normal reading.

use std::cell::Cell;

use egui::text::{LayoutJob, TextFormat};

use super::theme;

/// The active search, passed down to the text renderers.
#[derive(Clone, Copy)]
pub struct Highlight<'a> {
    pub query: &'a str,
    /// Index (document order) of the match the user has jumped to, which
    /// is drawn more prominently than the rest.
    pub active: usize,
}

/// Running match index across a whole document render, so the active
/// match can be told apart from the others.
#[derive(Default)]
pub struct Counter(Cell<usize>);

impl Counter {
    pub fn new() -> Self {
        Self(Cell::new(0))
    }
    /// Restart numbering — the editor's layouter can run more than once
    /// per frame, and each run must number matches from zero.
    pub fn reset(&self) {
        self.0.set(0);
    }
    fn next(&self) -> usize {
        let n = self.0.get();
        self.0.set(n + 1);
        n
    }
}

/// Append `text` to `job` in `fmt`, painting any search matches.
pub fn append(
    job: &mut LayoutJob,
    text: &str,
    fmt: &TextFormat,
    hl: Option<Highlight<'_>>,
    counter: &Counter,
) {
    let Some(hl) = hl.filter(|h| !h.query.is_empty()) else {
        job.append(text, 0.0, fmt.clone());
        return;
    };

    let hay = text.as_bytes();
    let needle = hl.query.as_bytes();
    if needle.len() > hay.len() {
        job.append(text, 0.0, fmt.clone());
        return;
    }

    let mut i = 0;
    let mut last = 0;
    while i + needle.len() <= hay.len() {
        let hit = hay[i..i + needle.len()].eq_ignore_ascii_case(needle)
            && text.is_char_boundary(i)
            && text.is_char_boundary(i + needle.len());
        if !hit {
            i += 1;
            continue;
        }
        if last < i {
            job.append(&text[last..i], 0.0, fmt.clone());
        }
        job.append(
            &text[i..i + needle.len()],
            0.0,
            match_format(fmt, counter.next() == hl.active),
        );
        i += needle.len();
        last = i;
    }
    if last < text.len() {
        job.append(&text[last..], 0.0, fmt.clone());
    }
}

/// Styling for a matched run: every match gets a tinted background; the
/// active one gets the full accent plus an underline so it stands out
/// among its neighbours.
fn match_format(base: &TextFormat, is_active: bool) -> TextFormat {
    let mut fmt = base.clone();
    if is_active {
        let bg = theme::accent();
        fmt.background = bg;
        fmt.color = theme::contrast_text(bg);
        fmt.underline = egui::Stroke::new(1.5, theme::contrast_text(bg));
    } else {
        fmt.background = theme::accent_yellow().gamma_multiply(0.55);
        fmt.color = theme::contrast_text(theme::accent_yellow());
    }
    fmt
}

/// Lay out a single run of text with highlighting (plaintext rows).
pub fn job_for(
    text: &str,
    fmt: TextFormat,
    hl: Option<Highlight<'_>>,
    counter: &Counter,
) -> LayoutJob {
    let mut job = LayoutJob::default();
    append(&mut job, text, &fmt, hl, counter);
    job
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fmt() -> TextFormat {
        TextFormat::default()
    }

    #[test]
    fn splits_around_matches_case_insensitively() {
        let counter = Counter::new();
        let job = job_for(
            "Alpha beta ALPHA",
            fmt(),
            Some(Highlight {
                query: "alpha",
                active: 0,
            }),
            &counter,
        );
        // "Alpha" | " beta " | "ALPHA"
        assert_eq!(job.sections.len(), 3);
        assert_eq!(job.text, "Alpha beta ALPHA");
        // Both matches are styled, the first (active) distinctly.
        assert!(job.sections[0].format.background != egui::Color32::TRANSPARENT);
        assert!(job.sections[2].format.background != egui::Color32::TRANSPARENT);
        assert_ne!(
            job.sections[0].format.background,
            job.sections[2].format.background
        );
        assert_eq!(
            job.sections[1].format.background,
            egui::Color32::TRANSPARENT
        );
    }

    #[test]
    fn no_query_leaves_text_untouched() {
        let counter = Counter::new();
        let job = job_for("plain text", fmt(), None, &counter);
        assert_eq!(job.sections.len(), 1);
        assert_eq!(job.text, "plain text");
    }

    #[test]
    fn counter_runs_across_calls() {
        let counter = Counter::new();
        let hl = Some(Highlight {
            query: "x",
            active: 1,
        });
        let a = job_for("x", fmt(), hl, &counter); // match #0
        let b = job_for("x", fmt(), hl, &counter); // match #1 — active
        assert_ne!(
            a.sections[0].format.background,
            b.sections[0].format.background
        );
    }
}
