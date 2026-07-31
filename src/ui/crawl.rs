//! Crawl mode: the document scrolls by itself.
//!
//! A reading mode rather than a viewing mode — the text drifts past at a
//! chosen rate so a long note can be read without touching anything, the
//! way opening titles roll. Everything about it is adjustable while it
//! runs, because the right speed depends on the document and the reader,
//! not on a setting chosen in advance.
//!
//! The motion itself lives here as plain arithmetic over a scroll offset,
//! separate from any egui call, so the awkward parts — reaching the end,
//! looping, reversing, a document shorter than the window — are testable
//! without a window.

use crate::config::{
    CrawlSection, MAX_CRAWL_SCALE, MAX_CRAWL_SPEED, MIN_CRAWL_SCALE, MIN_CRAWL_SPEED,
};

/// What the crawl did on a step, so the caller can react.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Moved (or was paused) with nothing else to report.
    Running,
    /// Reached the end and stopped there.
    Finished,
    /// Reached the end and wrapped around to the other end.
    Looped,
    /// Reached the end and turned around, still moving.
    Reversed,
}

/// Live crawl state. Speed and scale start from the config and are then
/// the session's own — changing them here never writes to disk.
pub struct Crawl {
    pub active: bool,
    pub paused: bool,
    pub speed: f32,
    pub text_scale: f32,
    pub direction_up: bool,
    /// Current scroll offset in points, kept as f64 so a slow crawl
    /// accumulates smoothly instead of stalling on sub-pixel steps.
    pub offset: f64,
    /// Time (egui seconds) to resume after a manual scroll, if set.
    pub resume_at: Option<f64>,
    /// Time until which the control hint overlay stays visible.
    pub hud_until: f64,
    /// The last thing the user changed, shown in the hint overlay.
    pub hud_msg: String,
    /// Interface zoom to restore on exit.
    pub restore_zoom: f32,
    /// Parked at an end with nowhere further to go. Distinguishes "you
    /// paused it" from "it ran out of document", which need different
    /// treatment when the reader asks it to continue.
    pub at_end: bool,
    /// While now is before this, the reader owns the scroll and the
    /// crawl keeps its hands off it.
    pub hold_until: f64,
}

impl Default for Crawl {
    fn default() -> Self {
        Self {
            active: false,
            paused: false,
            speed: 40.0,
            text_scale: 1.3,
            direction_up: true,
            offset: 0.0,
            resume_at: None,
            hud_until: 0.0,
            hud_msg: String::new(),
            restore_zoom: 1.0,
            at_end: false,
            hold_until: 0.0,
        }
    }
}

impl Crawl {
    /// Begin a crawl from `offset`, seeded from the saved defaults.
    pub fn start(&mut self, cfg: &CrawlSection, offset: f32, now: f64, restore_zoom: f32) {
        let cfg = cfg.clamped();
        self.active = true;
        self.paused = false;
        self.speed = cfg.speed;
        self.text_scale = cfg.text_scale;
        self.direction_up = cfg.direction_up;
        // Starts where the reader already was, so turning it on mid-page
        // continues rather than jumping back to the top.
        self.offset = offset.max(0.0) as f64;
        self.resume_at = None;
        self.at_end = false;
        self.hold_until = 0.0;
        self.restore_zoom = restore_zoom;
        self.hud_until = now + 4.0;
        // ASCII words, not arrows: U+2191/U+2193 are absent from the
        // bundled proportional font and would render as tofu boxes.
        self.hud_msg = "Space pause  \u{B7}  Up/Down speed  \u{B7}  +/- text size  \u{B7}  \
             R reverse  \u{B7}  Esc exit"
            .to_string();
    }

    pub fn stop(&mut self) {
        self.active = false;
        self.paused = false;
        self.resume_at = None;
        self.at_end = false;
        self.hold_until = 0.0;
    }

    /// Whether the crawl should be driving the scroll position this
    /// frame, as opposed to leaving it to the reader.
    ///
    /// This is the whole answer to "my scroll wheel did nothing": while
    /// the crawl forces an absolute offset every frame, a wheel event is
    /// overwritten before it can take effect. It only drives while it is
    /// actually moving, and steps aside for a moment after any manual
    /// scroll so the wheel behaves exactly as it does everywhere else.
    pub fn drives_scroll(&self, now: f64) -> bool {
        self.active && !self.paused && now >= self.hold_until
    }

    /// Advance by `dt` seconds. `max_scroll` is the furthest the view can
    /// scroll (content height minus viewport height).
    ///
    /// Returns what happened so the caller can stop, loop, or keep going.
    pub fn step(&mut self, dt: f64, max_scroll: f32, cfg: &CrawlSection) -> Step {
        if !self.active || self.paused {
            return Step::Running;
        }
        let max = max_scroll.max(0.0) as f64;
        // A document shorter than the window has nowhere to go. Treat it
        // as finished rather than spinning at offset 0 forever.
        if max <= 0.0 {
            return Step::Finished;
        }

        let delta = self.speed as f64 * dt.clamp(0.0, 0.25);
        self.offset += if self.direction_up { delta } else { -delta };

        // Only the end you are travelling toward counts as the end.
        // Testing both bounds regardless meant starting at the very top
        // and moving forward reported "end of document" immediately —
        // the offset was still 0, which looked like running off the top.
        if self.direction_up {
            if self.offset >= max {
                return self.hit_end(max, 0.0, cfg);
            }
            // Never above the start, whatever the arithmetic did.
            self.offset = self.offset.max(0.0);
        } else {
            if self.offset <= 0.0 {
                return self.hit_end(0.0, max, cfg);
            }
            self.offset = self.offset.min(max);
        }
        Step::Running
    }

    /// Arrived at `here`; `other` is the far end. Applies the configured
    /// end behavior.
    fn hit_end(&mut self, here: f64, other: f64, cfg: &CrawlSection) -> Step {
        if cfg.loops_at_end() {
            self.offset = other;
            return Step::Looped;
        }
        self.offset = here;
        if cfg.reverses_at_end() {
            // Turn around and keep going: the reader asked for motion,
            // and there is still document in the other direction.
            self.direction_up = !self.direction_up;
            return Step::Reversed;
        }
        self.at_end = true;
        Step::Finished
    }

    /// Adopt a scroll position the user produced by hand, and pause if
    /// configured to. Without the resync the next frame would yank the
    /// view straight back to where the crawl thought it was.
    pub fn user_scrolled(&mut self, offset: f32, now: f64, cfg: &CrawlSection) {
        self.offset = offset.max(0.0) as f64;
        // Scrolling always reaches the document, whether or not it also
        // pauses: for this long a moment the crawl does not touch the
        // offset, so wheel and trackpad work at their normal speed.
        self.hold_until = now + 0.35;
        self.at_end = false;
        if cfg.pause_on_scroll {
            self.paused = true;
            self.resume_at =
                (cfg.resume_after_seconds > 0.0).then_some(now + cfg.resume_after_seconds as f64);
            self.note(
                if cfg.resume_after_seconds > 0.0 {
                    format!("Scrolling — resumes in {:.0}s", cfg.resume_after_seconds)
                } else {
                    "Paused — Space to resume".to_string()
                },
                now,
            );
        }
    }

    /// Resume if a timed pause has elapsed.
    pub fn tick_resume(&mut self, now: f64) {
        if let Some(at) = self.resume_at {
            if now >= at {
                self.resume_at = None;
                self.paused = false;
            }
        }
    }

    pub fn toggle_pause(&mut self, now: f64) {
        // Resuming while parked at an end can only mean going back the
        // other way; continuing forward is not a thing that exists.
        if self.paused && self.at_end {
            self.at_end = false;
            self.paused = false;
            self.direction_up = !self.direction_up;
            self.note("Playing (turned around)".to_string(), now);
            return;
        }
        self.paused = !self.paused;
        self.resume_at = None;
        self.note(
            if self.paused { "Paused" } else { "Playing" }.to_string(),
            now,
        );
    }

    /// Change speed by a multiplicative step, so each press feels the
    /// same at 10 pt/s and at 200.
    pub fn nudge_speed(&mut self, faster: bool, now: f64) {
        let factor = if faster { 1.25 } else { 1.0 / 1.25 };
        self.speed = (self.speed * factor).clamp(MIN_CRAWL_SPEED, MAX_CRAWL_SPEED);
        self.note(format!("Speed {:.0} pt/s", self.speed), now);
    }

    pub fn nudge_scale(&mut self, bigger: bool, now: f64) {
        let step = if bigger { 0.1 } else { -0.1 };
        self.text_scale = (self.text_scale + step).clamp(MIN_CRAWL_SCALE, MAX_CRAWL_SCALE);
        self.note(format!("Text {:.0}%", self.text_scale * 100.0), now);
    }

    pub fn reverse(&mut self, now: f64) {
        self.direction_up = !self.direction_up;
        // Parked at an end, the new direction has somewhere to go — so
        // reversing is also a resume. Not doing this is what left the
        // crawl stuck at the top with no way to restart it.
        if self.at_end {
            self.at_end = false;
            self.paused = false;
            self.resume_at = None;
        }
        self.note(
            if self.direction_up {
                "Forward".to_string()
            } else {
                "Reverse".to_string()
            },
            now,
        );
    }

    /// Show a transient message in the control overlay.
    pub fn note(&mut self, msg: String, now: f64) {
        self.hud_msg = msg;
        self.hud_until = now + 2.0;
    }

    /// Whether the overlay should be drawn right now.
    pub fn hud_visible(&self, now: f64) -> bool {
        self.active && now < self.hud_until
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> CrawlSection {
        CrawlSection::default()
    }

    #[test]
    fn a_step_moves_by_speed_times_time() {
        let mut c = Crawl::default();
        c.start(&cfg(), 0.0, 0.0, 1.0);
        c.speed = 100.0;
        // Ordinary frame times are far below the 0.25s clamp, so motion
        // is exactly speed x time.
        assert_eq!(c.step(0.1, 10_000.0, &cfg()), Step::Running);
        assert!((c.offset - 10.0).abs() < 0.001, "got {}", c.offset);
        c.step(0.2, 10_000.0, &cfg());
        assert!((c.offset - 30.0).abs() < 0.001, "got {}", c.offset);
    }

    #[test]
    fn a_huge_frame_gap_cannot_teleport_the_view() {
        // Waking from sleep, or a stalled frame, hands over a dt of many
        // seconds. Without a clamp the document would jump hundreds of
        // lines and lose the reader's place entirely.
        let mut c = Crawl::default();
        c.start(&cfg(), 0.0, 0.0, 1.0);
        c.speed = 100.0;
        c.step(45.0, 100_000.0, &cfg());
        assert!(
            c.offset <= 25.0,
            "clamped to a quarter second, got {}",
            c.offset
        );
    }

    fn ending(action: &str) -> CrawlSection {
        CrawlSection {
            end_action: action.to_string(),
            ..cfg()
        }
    }

    #[test]
    fn each_end_behaviour_does_what_it_says() {
        let mut c = Crawl::default();
        c.start(&cfg(), 0.0, 0.0, 1.0);
        c.speed = 4000.0; // 0.25s of this clears a 500pt document

        // "stop": park at the end, pinned exactly to the limit rather
        // than drifting past it.
        assert_eq!(c.step(0.25, 500.0, &ending("stop")), Step::Finished);
        assert_eq!(c.offset, 500.0);
        assert!(c.at_end, "and it knows it is stuck there");

        // "reverse" (the default): turn around and keep moving.
        c.start(&cfg(), 0.0, 0.0, 1.0);
        c.speed = 4000.0;
        assert!(c.direction_up);
        assert_eq!(c.step(0.25, 500.0, &ending("reverse")), Step::Reversed);
        assert!(!c.direction_up, "now heading back");
        assert!(!c.at_end, "not stuck — it is still moving");
        assert_eq!(c.offset, 500.0);
        // And it actually moves away from the end on the next step.
        c.speed = 100.0;
        assert_eq!(c.step(0.1, 500.0, &ending("reverse")), Step::Running);
        assert!(c.offset < 500.0);

        // "loop": jump to the other end and continue.
        c.start(&cfg(), 0.0, 0.0, 1.0);
        c.speed = 4000.0;
        assert_eq!(c.step(0.25, 500.0, &ending("loop")), Step::Looped);
        assert_eq!(c.offset, 0.0);
        assert!(c.direction_up, "looping does not change direction");
    }

    /// Regression: reaching the top and reversing left the crawl frozen.
    /// Hitting an end paused it, and neither R nor Space cleared that,
    /// so there was no way to get it moving again short of leaving the
    /// mode entirely.
    #[test]
    fn a_crawl_parked_at_an_end_can_always_be_restarted() {
        let stop = ending("stop");
        let mut c = Crawl::default();
        c.start(&stop, 500.0, 0.0, 1.0);
        c.speed = 4000.0;
        c.reverse(0.0); // heading back toward the top
        assert_eq!(c.step(0.25, 500.0, &stop), Step::Finished);
        assert_eq!(c.offset, 0.0);
        c.paused = true; // what the app does on Finished
        assert!(c.at_end);

        // R turns around and resumes in one action.
        c.reverse(1.0);
        assert!(!c.paused, "reversing at an end also resumes");
        assert!(!c.at_end);
        assert!(c.direction_up);
        assert_eq!(c.step(0.1, 500.0, &stop), Step::Running);
        assert!(c.offset > 0.0, "and it genuinely moves");

        // Space does the same, since forward is the only way left to go.
        c.offset = 0.0;
        c.direction_up = false;
        c.paused = true;
        c.at_end = true;
        c.toggle_pause(2.0);
        assert!(!c.paused);
        assert!(c.direction_up, "turned around, because back is a wall");
        assert!(c.hud_msg.contains("turned around"), "got {}", c.hud_msg);
    }

    /// Regression: the scroll wheel barely moved the document. The crawl
    /// forced an absolute offset every frame, overwriting wheel input
    /// before it could take effect.
    #[test]
    fn the_reader_owns_the_scroll_after_a_wheel_event() {
        let mut c = Crawl::default();
        c.start(&cfg(), 0.0, 100.0, 1.0);
        assert!(c.drives_scroll(100.0), "driving while it runs");

        // A wheel event hands the view over for a moment, so egui's own
        // scrolling applies at its normal speed.
        c.user_scrolled(4000.0, 100.0, &cfg());
        assert!(!c.drives_scroll(100.0));
        assert!(!c.drives_scroll(100.3), "still the reader's");
        assert_eq!(c.offset, 4000.0, "and it adopts where they scrolled to");

        // Pausing keeps the view under manual control indefinitely.
        assert!(!c.drives_scroll(101.0), "still paused from the scroll");
        c.toggle_pause(101.0);
        assert!(c.drives_scroll(101.0), "resumed, and driving again");

        // With pause-on-scroll off it keeps moving, but still yields the
        // view briefly so the wheel is not fought.
        let no_pause = CrawlSection {
            pause_on_scroll: false,
            ..cfg()
        };
        c.user_scrolled(10.0, 200.0, &no_pause);
        assert!(!c.paused);
        assert!(!c.drives_scroll(200.1), "yields for a moment");
        assert!(c.drives_scroll(200.5), "then takes over again");
    }

    #[test]
    fn reverse_walks_back_and_stops_at_the_top() {
        let mut c = Crawl::default();
        c.start(&cfg(), 400.0, 0.0, 1.0);
        c.speed = 4000.0;
        c.reverse(0.0);
        assert!(!c.direction_up);

        assert_eq!(c.step(0.25, 500.0, &ending("stop")), Step::Finished);
        assert_eq!(c.offset, 0.0, "never scrolls above the start");

        // With looping on, running off the top wraps to the end.
        let looping = ending("loop");
        c.offset = 100.0;
        assert_eq!(c.step(0.25, 500.0, &looping), Step::Looped);
        assert_eq!(c.offset, 500.0);
    }

    /// Regression: the crawl reported "end of document" the instant it
    /// started. Beginning at the top means offset 0, and a frame with
    /// dt = 0 (the first one, before any elapsed time is known) leaves it
    /// at 0 — which the old code read as having run off the top.
    #[test]
    fn starting_at_the_top_moving_forward_is_not_the_end() {
        let mut c = Crawl::default();
        c.start(&cfg(), 0.0, 0.0, 1.0);
        assert_eq!(c.offset, 0.0);

        // The first frame has no elapsed time to work from.
        assert_eq!(c.step(0.0, 15_000.0, &cfg()), Step::Running);
        assert!(!c.paused);
        // And it keeps going from there.
        assert_eq!(c.step(0.1, 15_000.0, &cfg()), Step::Running);
        assert!(c.offset > 0.0);

        // Moving backwards from the top IS the end, as before.
        c.offset = 0.0;
        c.reverse(0.0);
        assert_eq!(c.step(0.1, 15_000.0, &ending("stop")), Step::Finished);
    }

    #[test]
    fn a_document_shorter_than_the_window_finishes_immediately() {
        // max_scroll of 0 means there is nothing to scroll. Stepping must
        // report that rather than pretending to animate.
        let mut c = Crawl::default();
        c.start(&cfg(), 0.0, 0.0, 1.0);
        assert_eq!(c.step(1.0, 0.0, &cfg()), Step::Finished);
        assert_eq!(c.offset, 0.0);
    }

    #[test]
    fn pausing_freezes_the_offset() {
        let mut c = Crawl::default();
        c.start(&cfg(), 0.0, 0.0, 1.0);
        c.speed = 100.0;
        c.toggle_pause(0.0);
        assert!(c.paused);
        c.step(10.0, 10_000.0, &cfg());
        assert_eq!(c.offset, 0.0, "a paused crawl does not move");
        c.toggle_pause(0.0);
        c.step(0.1, 10_000.0, &cfg());
        assert!(c.offset > 0.0);
    }

    #[test]
    fn a_manual_scroll_adopts_the_new_position() {
        // The crawl must take the user's position as its own, or the next
        // frame would snap the view back and fight the scroll wheel.
        let mut c = Crawl::default();
        c.start(&cfg(), 0.0, 0.0, 1.0);
        c.user_scrolled(1234.0, 5.0, &cfg());
        assert_eq!(c.offset, 1234.0);
        assert!(c.paused, "scrolling pauses");
        // By default it comes back on its own shortly after the reader
        // stops, rather than needing Space — scrolling to check something
        // should not be the end of the crawl.
        assert_eq!(c.resume_at, Some(5.0 + cfg().resume_after_seconds as f64));
        assert!(
            cfg().resume_after_seconds > 0.0,
            "auto-resume is the default"
        );

        // Set to 0, it waits for the reader instead.
        let manual = CrawlSection {
            resume_after_seconds: 0.0,
            ..cfg()
        };
        c.user_scrolled(1.0, 5.0, &manual);
        assert!(c.paused);
        assert!(c.resume_at.is_none(), "stays paused until Space");

        // With an explicit delay, it comes back on time.
        let timed = CrawlSection {
            resume_after_seconds: 3.0,
            ..cfg()
        };
        c.user_scrolled(10.0, 5.0, &timed);
        assert_eq!(c.resume_at, Some(8.0));
        c.tick_resume(7.0);
        assert!(c.paused, "not yet");
        c.tick_resume(8.0);
        assert!(!c.paused, "resumed on time");

        // Pause-on-scroll off: position still tracks, motion continues.
        let no_pause = CrawlSection {
            pause_on_scroll: false,
            ..cfg()
        };
        c.paused = false;
        c.user_scrolled(50.0, 9.0, &no_pause);
        assert_eq!(c.offset, 50.0);
        assert!(!c.paused);
    }

    #[test]
    fn live_controls_stay_within_their_limits() {
        let mut c = Crawl::default();
        c.start(&cfg(), 0.0, 0.0, 1.0);

        for _ in 0..100 {
            c.nudge_speed(true, 0.0);
        }
        assert_eq!(c.speed, MAX_CRAWL_SPEED);
        for _ in 0..100 {
            c.nudge_speed(false, 0.0);
        }
        assert_eq!(c.speed, MIN_CRAWL_SPEED);

        for _ in 0..100 {
            c.nudge_scale(true, 0.0);
        }
        assert!((c.text_scale - MAX_CRAWL_SCALE).abs() < 0.001);
        for _ in 0..100 {
            c.nudge_scale(false, 0.0);
        }
        assert!((c.text_scale - MIN_CRAWL_SCALE).abs() < 0.001);
    }

    #[test]
    fn speed_steps_are_proportional_not_fixed() {
        // A fixed +10 would be a huge jump at 12 pt/s and imperceptible
        // at 300, so the step is multiplicative.
        let mut c = Crawl {
            speed: 40.0,
            ..Crawl::default()
        };
        c.nudge_speed(true, 0.0);
        assert!((c.speed - 50.0).abs() < 0.001);
        c.speed = 200.0;
        c.nudge_speed(true, 0.0);
        assert!((c.speed - 250.0).abs() < 0.001);
    }

    #[test]
    fn starting_keeps_the_readers_place_and_clamps_bad_config() {
        let mut c = Crawl::default();
        c.start(&cfg(), 900.0, 0.0, 1.0);
        assert_eq!(c.offset, 900.0, "continues from where you were");
        assert!(c.active && !c.paused);

        // A hand-edited config must not produce an unusable crawl: the
        // chrome is hidden, so escaping a 5000 pt/s scroll is hard.
        let wild = CrawlSection {
            speed: 99_999.0,
            text_scale: 40.0,
            ..cfg()
        };
        c.start(&wild, 0.0, 0.0, 1.0);
        assert_eq!(c.speed, MAX_CRAWL_SPEED);
        assert_eq!(c.text_scale, MAX_CRAWL_SCALE);

        let broken = CrawlSection {
            speed: f32::NAN,
            text_scale: f32::NAN,
            ..cfg()
        };
        c.start(&broken, 0.0, 0.0, 1.0);
        assert!(c.speed.is_finite() && c.text_scale.is_finite());
    }

    #[test]
    fn the_hud_appears_on_change_and_fades() {
        let mut c = Crawl::default();
        c.start(&cfg(), 0.0, 100.0, 1.0);
        assert!(c.hud_visible(101.0), "shown on entry");
        assert!(!c.hud_visible(200.0), "and fades away");

        c.nudge_speed(true, 300.0);
        assert!(c.hud_visible(301.0), "any change brings it back");
        assert!(c.hud_msg.contains("pt/s"));

        // Never visible when the crawl isn't running.
        c.stop();
        assert!(!c.hud_visible(300.5));
    }
}
