//! Momentum: a writing mode that locks the document if you stop typing.
//!
//! The problem it addresses is not security, it is the blank page. A
//! first draft stalls when you pause to judge the sentence you just
//! wrote, and the cure that writers actually use — freewriting — is to
//! make stopping cost something. So while this mode is on, a pause
//! longer than `pause_seconds` locks the session.
//!
//! **Nothing is destroyed by it.** Locking runs the ordinary lock path,
//! which encrypts unsaved text into the stash before dropping the
//! plaintext, so the penalty is having to unlock and pick the thread
//! back up — never lost words. That is also why the mode refuses to
//! arm on a document with no key to stash to: there the lock would be
//! deferred, the mode would appear broken, and on a file that *could*
//! not defer it would be the one thing this must never do.
//!
//! The timing lives here as plain arithmetic over a clock, with no egui
//! in sight, so the awkward parts — the grace period, what counts as
//! typing, what happens the instant the mode is switched on — are
//! testable without a window.

use crate::config::MomentumSection;

/// What the caller should do after a tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Off, or armed with time still on the clock.
    Running,
    /// The pause ran out. Lock the session.
    Expired,
}

/// Live state for the mode. `enabled` is a session toggle, like focus
/// mode — deliberately not persisted, because a mode that locks your
/// document on a pause should never be a surprise inherited from the
/// last time the app ran.
#[derive(Default)]
pub struct Momentum {
    pub enabled: bool,
    /// Clock reading of the last keystroke, or of the moment the mode
    /// was armed. `None` while disarmed.
    last_input: Option<f64>,
    /// While the clock is before this, a pause does not count.
    armed_until: f64,
    /// Whether the editor was open on the previous tick, so returning to
    /// it can be told apart from having been in it all along.
    was_editing: bool,
}

/// How much longer the grace period is when coming back to the editor
/// after a lock, rather than simply opening it.
///
/// Restarting is harder than starting. After a lock you have to unlock,
/// let the stash decrypt, find where the sentence broke off and pick the
/// thought back up — the ordinary grace is sized for none of that.
const RESUME_GRACE_MULTIPLIER: f64 = 3.0;

impl Momentum {
    /// Turn the mode on and start the grace period.
    ///
    /// `editing` is the editor's current state, recorded so that
    /// switching the mode on while already writing is not mistaken for
    /// *returning* to the editor on the next tick — which would hand
    /// out the longer resume grace and quietly contradict the number in
    /// Settings.
    pub fn arm(&mut self, now: f64, editing: bool, cfg: &MomentumSection) {
        self.enabled = true;
        self.last_input = Some(now);
        self.armed_until = now + cfg.grace_seconds as f64;
        self.was_editing = editing;
    }

    pub fn disarm(&mut self) {
        self.enabled = false;
        self.last_input = None;
    }

    /// Come back to the editor: a longer grace than a plain re-arm.
    ///
    /// This is the path out of a Momentum lock, and it is the one that
    /// has to be generous. Granting the ordinary grace *at the moment
    /// the lock fired* — which is what this used to do — spends the
    /// whole window while the user is still typing a passphrase, so the
    /// mode re-locked the instant their text came back. The clock has
    /// to start when they can actually type again, not when they
    /// stopped being able to.
    pub fn resume(&mut self, now: f64, cfg: &MomentumSection) {
        if self.enabled {
            self.last_input = Some(now);
            self.armed_until = now + cfg.grace_seconds as f64 * RESUME_GRACE_MULTIPLIER;
        }
    }

    /// Note that the user typed.
    pub fn saw_input(&mut self, now: f64) {
        if self.enabled {
            self.last_input = Some(now);
        }
    }

    /// Seconds left before the lock. `None` only when the mode is not
    /// running at all — off, or the editor closed.
    ///
    /// The pause is measured from the later of "your last keystroke" and
    /// "the end of the grace period", which is what makes the grace
    /// actually grant time rather than merely postpone the reckoning.
    /// Measuring from the keystroke alone meant that when a long grace
    /// ended, the seconds spent inside it had already been counted
    /// against the pause: the countdown appeared at zero, or never
    /// appeared, and the document locked on the same frame. Coming back
    /// from a lock is exactly where the grace is longest, so that is
    /// exactly where it bit.
    ///
    /// During the grace this reads full, so the indicator is on screen
    /// the whole time the mode is armed and writing — a mode that can
    /// lock your document should be visible before it does.
    pub fn remaining(&self, now: f64, editing: bool, cfg: &MomentumSection) -> Option<f32> {
        if !self.enabled || !editing {
            return None;
        }
        let start = self.last_input?.max(self.armed_until);
        let pause = cfg.pause_seconds.max(0.0) as f64;
        let left = pause - (now - start).max(0.0);
        Some(left.clamp(0.0, pause) as f32)
    }

    /// Advance the mode. `editing` gates the whole thing: these are the
    /// same letters you would be typing, and a reader who has stopped
    /// scrolling has not stopped writing — they were never writing.
    pub fn step(&mut self, now: f64, editing: bool, cfg: &MomentumSection) -> Step {
        // The editor opening is the moment the user is able to write
        // again — after an unlock, after a dialog, or just from pressing
        // the edit shortcut. None of those are a pause they chose, and
        // however long they took getting here should not count against
        // them, so the clock starts here rather than wherever it left
        // off.
        let resumed = editing && !self.was_editing;
        self.was_editing = editing;
        if resumed {
            self.resume(now, cfg);
            return Step::Running;
        }
        match self.remaining(now, editing, cfg) {
            Some(left) if left <= 0.0 => Step::Expired,
            _ => Step::Running,
        }
    }

    /// How close to the deadline, 0.0 → 1.0, for drawing urgency.
    /// `None` whenever `remaining` is.
    pub fn urgency(&self, now: f64, editing: bool, cfg: &MomentumSection) -> Option<f32> {
        let left = self.remaining(now, editing, cfg)?;
        let span = cfg.pause_seconds.max(f32::EPSILON);
        Some((1.0 - left / span).clamp(0.0, 1.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> MomentumSection {
        MomentumSection {
            pause_seconds: 3.0,
            grace_seconds: 5.0,
            show_countdown: true,
        }
    }

    /// Open the editor and settle past the resume grace, so a test can
    /// start from "already writing" rather than from the doorway.
    fn writing_since(m: &mut Momentum, t: f64, c: &MomentumSection) {
        m.step(t, true, c); // rising edge: grants the resume grace
        m.saw_input(t + resume_grace(c));
    }

    fn resume_grace(c: &MomentumSection) -> f64 {
        c.grace_seconds as f64 * RESUME_GRACE_MULTIPLIER
    }

    /// Off means off: no amount of silence locks anything.
    #[test]
    fn a_disabled_mode_never_expires() {
        let mut m = Momentum::default();
        let c = cfg();
        assert_eq!(m.step(1_000.0, true, &c), Step::Running);
        assert_eq!(m.remaining(1_000.0, true, &c), None);
    }

    /// The grace period is what stops the mode firing while the user is
    /// still reaching for the keyboard.
    #[test]
    fn the_grace_period_holds_the_timer_off() {
        let mut m = Momentum::default();
        let c = cfg();
        m.arm(0.0, false, &c);
        m.step(0.0, true, &c); // enter the editor
        let g = resume_grace(&c);
        // Well past the 3s pause, but inside the grace.
        assert_eq!(m.step(g - 1.0, true, &c), Step::Running);
        assert_eq!(
            m.remaining(g - 1.0, true, &c),
            Some(c.pause_seconds),
            "the indicator reads full while the grace runs, not blank"
        );
        // Past the grace, and past the pause: now it fires.
        assert_eq!(m.step(g + 4.0, true, &c), Step::Expired);
    }

    #[test]
    fn typing_keeps_it_alive_and_silence_ends_it() {
        let mut m = Momentum::default();
        let c = cfg();
        m.arm(0.0, false, &c);
        writing_since(&mut m, 0.0, &c);
        let base = resume_grace(&c);
        // Type steadily every second, well past the grace period.
        for i in 1..20 {
            let t = base + i as f64;
            m.saw_input(t);
            assert_eq!(m.step(t, true, &c), Step::Running, "at t={t}");
        }
        // Then stop. Just under the pause is still fine…
        let last = base + 19.0;
        assert_eq!(m.step(last + 2.9, true, &c), Step::Running);
        // …and just over is not.
        assert_eq!(m.step(last + 3.1, true, &c), Step::Expired);
    }

    /// Reading is not writing. The mode must not lock someone who is
    /// looking at their document with the editor closed — the motion
    /// keys are not typing, and there is nothing to keep momentum in.
    #[test]
    fn it_only_counts_while_the_editor_is_open() {
        let mut m = Momentum::default();
        let c = cfg();
        m.arm(0.0, false, &c);
        assert_eq!(m.step(100.0, false, &c), Step::Running);
        assert_eq!(m.remaining(100.0, false, &c), None);
        // Opening the editor grants grace rather than locking instantly,
        // however long the document sat there being read.
        assert_eq!(m.step(100.0, true, &c), Step::Running);
        let g = resume_grace(&c);
        assert_eq!(m.step(100.0 + g + 4.0, true, &c), Step::Expired);
    }

    /// The countdown has to reach zero before the lock, not after, or
    /// the number on screen disagrees with what just happened.
    #[test]
    fn the_countdown_hits_zero_exactly_when_it_expires() {
        let mut m = Momentum::default();
        let c = cfg();
        m.arm(0.0, false, &c);
        writing_since(&mut m, 0.0, &c);
        let t = resume_grace(&c);
        m.saw_input(t);
        assert_eq!(m.remaining(t, true, &c), Some(3.0));
        assert_eq!(m.remaining(t + 1.5, true, &c), Some(1.5));
        assert_eq!(m.remaining(t + 3.0, true, &c), Some(0.0));
        assert_eq!(m.step(t + 3.0, true, &c), Step::Expired);
        // Never negative — the display formats this straight.
        assert_eq!(m.remaining(t + 99.0, true, &c), Some(0.0));
    }

    #[test]
    fn urgency_runs_from_calm_to_out_of_time() {
        let mut m = Momentum::default();
        let c = cfg();
        m.arm(0.0, false, &c);
        writing_since(&mut m, 0.0, &c);
        let t = resume_grace(&c);
        m.saw_input(t);
        assert_eq!(m.urgency(t, true, &c), Some(0.0));
        assert_eq!(m.urgency(t + 1.5, true, &c), Some(0.5));
        assert_eq!(m.urgency(t + 3.0, true, &c), Some(1.0));
        assert_eq!(m.urgency(t + 99.0, true, &c), Some(1.0), "never past 1");
    }

    /// The bug this mode shipped with: it locked, and the moment the
    /// user finished unlocking and their text came back, it locked
    /// again — because the grace was granted when the lock *fired*,
    /// and spent itself while they were typing a passphrase.
    ///
    /// The clock has to start when the editor reopens.
    #[test]
    fn unlocking_after_a_lock_does_not_immediately_relock() {
        let mut m = Momentum::default();
        let c = cfg();
        m.arm(0.0, false, &c);
        writing_since(&mut m, 0.0, &c);
        let stopped = resume_grace(&c);
        m.saw_input(stopped);
        assert_eq!(m.step(stopped + 4.0, true, &c), Step::Expired, "locks");

        // The lock closes the document, so the editor is shut while the
        // user unlocks — which takes a passphrase and a while.
        let mut t = stopped + 4.0;
        for _ in 0..30 {
            t += 1.0;
            assert_eq!(m.step(t, false, &c), Step::Running);
        }

        // Their text comes back. This must not lock on the next frame.
        assert_eq!(m.step(t, true, &c), Step::Running, "relocked instantly");
        assert_eq!(
            m.remaining(t, true, &c),
            Some(c.pause_seconds),
            "the countdown must be on screen and full, not missing"
        );
        // And they get the fuller resume grace, not the plain one.
        assert_eq!(
            m.step(t + c.grace_seconds as f64 + 1.0, true, &c),
            Step::Running
        );
        // It does eventually resume enforcing.
        assert_eq!(m.step(t + resume_grace(&c) + 4.0, true, &c), Step::Expired);
    }

    /// `resume` is the single lever for "the user can type again". It
    /// restarts the clock without switching the mode off, so the code
    /// path out of a lock cannot accidentally disarm the thing.
    #[test]
    fn resuming_restarts_the_clock_without_toggling_the_mode() {
        let mut m = Momentum::default();
        let c = cfg();
        m.arm(0.0, false, &c);
        writing_since(&mut m, 0.0, &c);
        let t = resume_grace(&c);
        m.saw_input(t);
        assert_eq!(m.step(t + 10.0, true, &c), Step::Expired);

        m.resume(t + 10.0, &c);
        assert!(m.enabled, "resume must not switch the mode off");
        assert_eq!(m.step(t + 11.0, true, &c), Step::Running);
        // And it does start enforcing again once the grace is spent.
        assert_eq!(
            m.step(t + 10.0 + resume_grace(&c) + 4.0, true, &c),
            Step::Expired,
        );
    }

    /// The second bug this mode shipped with, once the first was fixed:
    /// after an unlock the countdown never appeared, and the document
    /// locked a few seconds later anyway.
    ///
    /// The grace held the display off while the pause was measured from
    /// the last keystroke — so by the time the grace ended, fifteen
    /// seconds had already been counted against a three-second pause.
    /// The countdown went straight to zero. A grace period has to grant
    /// time, not merely postpone the reckoning.
    #[test]
    fn the_full_countdown_is_visible_after_a_resume() {
        let mut m = Momentum::default();
        let c = cfg();
        m.arm(0.0, false, &c);

        // Locked, then unlocked twenty seconds later.
        m.step(0.0, false, &c);
        let back = 20.0;
        assert_eq!(m.step(back, true, &c), Step::Running, "reopened");

        // A couple of keystrokes, then they stop to think.
        m.saw_input(back + 1.0);
        m.saw_input(back + 2.0);
        let stopped = back + 2.0;

        // Through the whole grace the indicator is on screen and full.
        let grace_ends = back + resume_grace(&c);
        for t in [back, stopped, grace_ends - 0.1] {
            assert_eq!(
                m.remaining(t, true, &c),
                Some(c.pause_seconds),
                "countdown missing or partial at t={t}"
            );
            assert_eq!(m.step(t, true, &c), Step::Running);
        }

        // The instant the grace ends they get the *whole* pause, not a
        // countdown that already ran out while they were reading.
        assert_eq!(m.remaining(grace_ends, true, &c), Some(c.pause_seconds));
        assert_eq!(m.step(grace_ends, true, &c), Step::Running);

        // And it then counts down visibly, in order, before locking.
        assert_eq!(m.remaining(grace_ends + 1.0, true, &c), Some(2.0));
        assert_eq!(m.remaining(grace_ends + 2.0, true, &c), Some(1.0));
        assert_eq!(m.step(grace_ends + 2.0, true, &c), Step::Running);
        assert_eq!(m.remaining(grace_ends + 3.0, true, &c), Some(0.0));
        assert_eq!(m.step(grace_ends + 3.0, true, &c), Step::Expired);
    }

    /// Disarming forgets the clock, so switching back on later does not
    /// inherit a pause from minutes ago and lock immediately.
    #[test]
    fn re_arming_starts_fresh() {
        let mut m = Momentum::default();
        let c = cfg();
        m.arm(0.0, false, &c);
        m.saw_input(1.0);
        m.disarm();
        assert_eq!(m.step(500.0, true, &c), Step::Running);
        m.arm(500.0, false, &c);
        assert_eq!(m.step(501.0, true, &c), Step::Running, "grace applies");
    }

    /// A pause of zero is not reachable through the settings, but a
    /// hand-edited config is clamped rather than trusted — this pins the
    /// arithmetic against a pathological value regardless.
    #[test]
    fn a_tiny_pause_does_not_produce_nonsense() {
        let mut m = Momentum::default();
        let c = MomentumSection {
            pause_seconds: 0.0,
            grace_seconds: 0.0,
            show_countdown: true,
        };
        m.arm(0.0, false, &c);
        m.step(0.0, true, &c);
        assert_eq!(m.remaining(0.0, true, &c), Some(0.0));
        let u = m.urgency(0.0, true, &c).unwrap();
        assert!(u.is_finite() && (0.0..=1.0).contains(&u), "urgency was {u}");
    }
}
