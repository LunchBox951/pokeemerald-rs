//! The play-time counter: the clock behind [`PlayTime`]
//! (`pokeemerald/src/play_time.c:13-72`).
//!
//! [`PlayTime`] is only the four saved bytes. Whether they advance is run
//! state the save never records, so it lives in [`PlayTimeCounter`], which the
//! session owns beside its `SaveBlock2` and which acts on that block's time
//! through `&mut`. A counter is stopped when created -- upstream's static
//! starts zeroed and a soft reset clears it again -- so loading a save does
//! not by itself start the clock; the new-game or continue entry does
//! ([`PlayTimeCounter::reset`], [`PlayTimeCounter::start`]).
//!
//! # Max-out
//!
//! The counter pins [`PlayTime::MAX`] (999:59:59 with 59 vblanks) when the
//! hours exceed 999 and stops counting. A save loaded already past 999 hours
//! is pinned the moment the counter starts.
//!
//! # Malformed loaded bytes
//!
//! The stored fields are single bytes (hours a `u16`), and a loaded save can
//! hold values the counter never writes, such as 255 vblanks. An increment
//! wraps in its own field rather than carrying -- as upstream's `u8` fields
//! do -- so such a time never panics.

use super::block::PlayTime;

/// Vblanks per second, seconds per minute and minutes per hour alike: each
/// stored field carries into the next when it reaches 60.
const CARRY_AT: u8 = 60;

/// The most hours the counter counts; one more pins [`PlayTime::MAX`].
const MAX_HOURS: u16 = 999;

impl PlayTime {
    /// The time the counter pins once the hours exceed 999
    /// (`PlayTimeCounter_SetToMax`, `pokeemerald/src/play_time.c:65-72`).
    pub const MAX: Self = Self {
        hours: MAX_HOURS,
        minutes: CARRY_AT - 1,
        seconds: CARRY_AT - 1,
        vblanks: CARRY_AT - 1,
    };
}

/// Whether, and why not, the counter advances.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum RunState {
    /// Not counting: before a game starts, or after [`PlayTimeCounter::stop`].
    #[default]
    Stopped,
    /// Counting one vblank per [`PlayTimeCounter::update`].
    Running,
    /// Pinned at [`PlayTime::MAX`]; counts no further.
    MaxedOut,
}

/// The play-time clock's run state (module docs).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PlayTimeCounter {
    state: RunState,
}

impl PlayTimeCounter {
    /// A stopped counter.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: RunState::Stopped,
        }
    }

    /// Whether [`Self::update`] currently counts.
    #[must_use]
    pub const fn is_running(&self) -> bool {
        matches!(self.state, RunState::Running)
    }

    /// Whether the time has been pinned at [`PlayTime::MAX`].
    #[must_use]
    pub const fn is_maxed_out(&self) -> bool {
        matches!(self.state, RunState::MaxedOut)
    }

    /// Stop the counter and zero `time` (`PlayTimeCounter_Reset`,
    /// `pokeemerald/src/play_time.c:15-23`), as a new game does.
    pub fn reset(&mut self, time: &mut PlayTime) {
        self.state = RunState::Stopped;
        *time = PlayTime::default();
    }

    /// Start counting from `time` (`PlayTimeCounter_Start`,
    /// `pokeemerald/src/play_time.c:25-30`). A loaded time past 999 hours is
    /// pinned at once; one at exactly 999 hours is not.
    pub fn start(&mut self, time: &mut PlayTime) {
        self.state = RunState::Running;
        if time.hours > MAX_HOURS {
            self.pin_max(time);
        }
    }

    /// Stop counting, leaving the time as it is
    /// (`PlayTimeCounter_Stop`, `pokeemerald/src/play_time.c:32-35`).
    pub fn stop(&mut self) {
        self.state = RunState::Stopped;
    }

    /// Count one vblank into `time` when running, carrying 60 vblanks into a
    /// second, 60 seconds into a minute and 60 minutes into an hour
    /// (`PlayTimeCounter_Update`, `pokeemerald/src/play_time.c:37-63`), and
    /// pinning [`PlayTime::MAX`] when the hours pass 999. Does nothing while
    /// stopped or maxed out.
    pub fn update(&mut self, time: &mut PlayTime) {
        if !self.is_running() {
            return;
        }
        if !bump(&mut time.vblanks) || !bump(&mut time.seconds) || !bump(&mut time.minutes) {
            return;
        }
        time.hours = time.hours.wrapping_add(1);
        if time.hours > MAX_HOURS {
            self.pin_max(time);
        }
    }

    /// `PlayTimeCounter_SetToMax` (`pokeemerald/src/play_time.c:65-72`).
    fn pin_max(&mut self, time: &mut PlayTime) {
        self.state = RunState::MaxedOut;
        *time = PlayTime::MAX;
    }
}

/// Add one to `field`; at [`CARRY_AT`] or more it returns to zero and the
/// result is `true`, the carry into the next field.
fn bump(field: &mut u8) -> bool {
    *field = field.wrapping_add(1);
    if *field < CARRY_AT {
        return false;
    }
    *field = 0;
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn time(hours: u16, minutes: u8, seconds: u8, vblanks: u8) -> PlayTime {
        PlayTime {
            hours,
            minutes,
            seconds,
            vblanks,
        }
    }

    /// A counter started on `loaded`, and the time it then holds.
    fn started(loaded: PlayTime) -> (PlayTimeCounter, PlayTime) {
        let mut counter = PlayTimeCounter::new();
        let mut time = loaded;
        counter.start(&mut time);
        (counter, time)
    }

    fn after_one_update(loaded: PlayTime) -> PlayTime {
        let (mut counter, mut time) = started(loaded);
        counter.update(&mut time);
        time
    }

    #[test]
    fn a_new_counter_is_stopped() {
        assert!(!PlayTimeCounter::new().is_running());
        assert_eq!(PlayTimeCounter::default(), PlayTimeCounter::new());
    }

    #[test]
    fn update_while_stopped_changes_nothing() {
        let mut counter = PlayTimeCounter::new();
        let mut loaded = time(12, 34, 56, 59);
        counter.update(&mut loaded);
        assert_eq!(loaded, time(12, 34, 56, 59));
    }

    #[test]
    fn update_after_stop_changes_nothing_and_start_resumes() {
        let (mut counter, mut now) = started(time(0, 0, 0, 5));
        counter.update(&mut now);
        counter.stop();
        counter.update(&mut now);
        assert_eq!(now, time(0, 0, 0, 6));
        counter.start(&mut now);
        counter.update(&mut now);
        assert_eq!(now, time(0, 0, 0, 7));
    }

    #[test]
    fn sixty_vblanks_carry_one_second() {
        assert_eq!(after_one_update(time(0, 0, 0, 58)), time(0, 0, 0, 59));
        assert_eq!(after_one_update(time(0, 0, 0, 59)), time(0, 0, 1, 0));
    }

    #[test]
    fn fifty_nine_fifty_nine_carries_a_minute() {
        assert_eq!(after_one_update(time(0, 0, 59, 59)), time(0, 1, 0, 0));
    }

    #[test]
    fn fifty_nine_minutes_carry_an_hour() {
        assert_eq!(after_one_update(time(0, 59, 59, 59)), time(1, 0, 0, 0));
        assert_eq!(after_one_update(time(998, 59, 59, 59)), time(999, 0, 0, 0));
    }

    #[test]
    fn maxed_out_time_sticks() {
        let (mut counter, mut now) = started(time(999, 59, 59, 58));
        counter.update(&mut now);
        assert_eq!(now, time(999, 59, 59, 59));
        assert!(counter.is_running());
        counter.update(&mut now);
        assert_eq!(now, PlayTime::MAX);
        assert!(counter.is_maxed_out());
        for _ in 0..200 {
            counter.update(&mut now);
        }
        assert_eq!(now, time(999, 59, 59, 59));
        assert!(counter.is_maxed_out());
    }

    #[test]
    fn start_on_hours_above_999_maxes_at_once() {
        for hours in [1000, u16::MAX] {
            let (counter, now) = started(time(hours, 3, 4, 5));
            assert_eq!(now, time(999, 59, 59, 59));
            assert!(counter.is_maxed_out());
        }
    }

    #[test]
    fn start_at_exactly_999_hours_does_not_max() {
        let (counter, now) = started(time(999, 0, 0, 0));
        assert_eq!(now, time(999, 0, 0, 0));
        assert!(counter.is_running());
    }

    #[test]
    fn reset_zeroes_the_time_and_stops_the_counter() {
        let (mut counter, mut now) = started(time(1000, 1, 2, 3));
        counter.reset(&mut now);
        assert_eq!(now, time(0, 0, 0, 0));
        assert!(!counter.is_running() && !counter.is_maxed_out());
        counter.update(&mut now);
        assert_eq!(now, time(0, 0, 0, 0));
    }

    #[test]
    fn a_malformed_loaded_field_wraps_without_carrying() {
        assert_eq!(after_one_update(time(1, 2, 3, 255)), time(1, 2, 3, 0));
        // A loaded 60 vblanks is out of range but still carries on the update.
        assert_eq!(after_one_update(time(0, 0, 0, 60)), time(0, 0, 1, 0));
        assert_eq!(after_one_update(time(0, 255, 59, 59)), time(0, 0, 0, 0));
    }
}
