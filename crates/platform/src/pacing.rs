//! Frame pacing (S-1): a fixed-cadence wait analogous to upstream's
//! `WaitForVBlank` (`pokeemerald/src/main.c`).
//!
//! The GBA's LCD refreshes every 280896 CPU cycles at ~16.78 MHz — **not** a
//! rounded 60 Hz (`pokeemerald/src/m4a.c` spells this out: "LCD refresh rate
//! 59.7275Hz", "cycles per LCD fresh 280896"). [`FramePacer`] reproduces that
//! cadence and the hardware's catch-up behaviour: `WaitForVBlank` always
//! blocks for the *next* vblank IRQ, it never queues up extra frames to make
//! up for time already lost, so neither does this pacer.
//!
//! [`FramePacer::tick`] takes the current time as a parameter rather than
//! reading a clock itself, which is the injectable seam that lets the unit
//! tests below exercise cadence and catch-up math with synthetic [`Instant`]s
//! and without ever calling [`std::thread::sleep`].

use std::time::{Duration, Instant};

/// The GBA CPU clock, in Hz (`pokeemerald/src/m4a.c`).
const GBA_CPU_HZ: u64 = 16_777_216;

/// CPU cycles per LCD frame / vblank (`pokeemerald/src/m4a.c`).
const GBA_CYCLES_PER_FRAME: u64 = 280_896;

/// The GBA's real per-frame period (~59.7275 Hz), truncated to whole
/// nanoseconds.
///
/// The exact value is `280_896 * 1e9 / 16_777_216 = 16_742_706.298828125`
/// ns; [`Duration`] only stores whole nanoseconds, so this is short by
/// under a third of a nanosecond per frame — accumulating to roughly 1.5ms
/// of drift per 24 hours of continuous play, well below anything
/// perceptible.
pub const GBA_FRAME_PERIOD: Duration =
    Duration::from_nanos(GBA_CYCLES_PER_FRAME * 1_000_000_000 / GBA_CPU_HZ);

/// A fixed-cadence frame pacer.
///
/// Owns the schedule of upcoming frame deadlines; [`tick`](Self::tick) is a
/// pure function of "now" plus internal state — it performs no I/O and never
/// sleeps itself. The caller (see [`crate::window::Platform`]) is
/// responsible for actually blocking for the returned [`Duration`].
#[derive(Debug, Clone, Copy)]
pub struct FramePacer {
    period: Duration,
    next_deadline: Option<Instant>,
}

impl FramePacer {
    /// A pacer targeting the real GBA refresh cadence
    /// ([`GBA_FRAME_PERIOD`]).
    #[must_use]
    pub fn new() -> Self {
        Self::with_period(GBA_FRAME_PERIOD)
    }

    /// A pacer targeting an arbitrary fixed period (mainly for tests; real
    /// callers should use [`FramePacer::new`]).
    #[must_use]
    pub const fn with_period(period: Duration) -> Self {
        Self {
            period,
            next_deadline: None,
        }
    }

    /// Given the current time, return how long the caller should wait before
    /// starting the next frame, and advance the internal schedule.
    ///
    /// The first call always returns [`Duration::ZERO`] (there is no prior
    /// deadline to wait for yet). Subsequent calls return the time remaining
    /// until `period` has elapsed since the previous *scheduled* deadline
    /// (not since the previous call), so a run of on-time frames never
    /// accumulates scheduling drift from this method's own rounding.
    ///
    /// If the caller falls behind by more than one full period (e.g. a
    /// debugger pause, or a very slow frame), this skips forward to the next
    /// upcoming boundary on the fixed cadence rather than returning a zero
    /// wait for every missed frame in between — mirroring hardware, which
    /// only ever waits for the *next* vblank IRQ and never queues up
    /// "catch-up" frames for time that has already passed.
    pub fn tick(&mut self, now: Instant) -> Duration {
        let mut deadline = self.next_deadline.unwrap_or(now);
        if deadline < now {
            let overdue = now.duration_since(deadline);
            let period_nanos = self.period.as_nanos().max(1);
            // Round up: land on the first boundary *at or after* `now`, not
            // the last one strictly before it.
            let mut periods_missed = overdue.as_nanos() / period_nanos;
            if !overdue.as_nanos().is_multiple_of(period_nanos) {
                periods_missed += 1;
            }
            let skip = u32::try_from(periods_missed).unwrap_or(u32::MAX).max(1);
            deadline += self.period * skip;
        }
        let wait = deadline - now;
        self.next_deadline = Some(deadline + self.period);
        wait
    }
}

impl Default for FramePacer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
impl FramePacer {
    /// Whether [`FramePacer::tick`] has ever been called (i.e. a deadline
    /// has been scheduled). Test-only introspection used to assert,
    /// deterministically, that a caller never consults the pacer at all —
    /// see `window::tests::headless_wait_for_next_frame_never_consults_pacer`,
    /// the replacement for a wall-clock timing assertion that could flake
    /// under scheduler pressure.
    pub(crate) fn has_ticked(&self) -> bool {
        self.next_deadline.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_period_matches_gba_hardware_cadence() {
        // 280896 cycles at 16.777216 MHz, expressed precisely in
        // nanoseconds per the issue's formula: 280896 * 1e9 / 16_777_216.
        assert_eq!(GBA_FRAME_PERIOD, Duration::from_nanos(16_742_706));
        // And, in Hz, ~59.7275 - not a rounded 60.
        let hz = 1.0 / GBA_FRAME_PERIOD.as_secs_f64();
        assert!((hz - 59.7275).abs() < 0.001, "got {hz} Hz");
    }

    #[test]
    fn first_tick_never_waits() {
        let mut pacer = FramePacer::new();
        let now = Instant::now();
        assert_eq!(pacer.tick(now), Duration::ZERO);
    }

    #[test]
    fn on_time_tick_waits_the_full_period() {
        let mut pacer = FramePacer::new();
        let t0 = Instant::now();
        assert_eq!(pacer.tick(t0), Duration::ZERO);
        // Frame work took no time at all; called again immediately.
        assert_eq!(pacer.tick(t0), GBA_FRAME_PERIOD);
    }

    #[test]
    fn fast_frame_waits_the_remainder() {
        let mut pacer = FramePacer::new();
        let t0 = Instant::now();
        pacer.tick(t0); // deadline is now t0 + period
        let work = Duration::from_millis(2);
        let wait = pacer.tick(t0 + work);
        assert_eq!(wait, GBA_FRAME_PERIOD.checked_sub(work).unwrap());
    }

    #[test]
    fn scheduling_does_not_drift_across_on_time_frames() {
        // Simulate N frames, each serviced exactly on its scheduled
        // deadline (i.e. the caller slept for exactly the returned
        // duration). The Nth deadline should land exactly at N * period
        // from the start, with zero accumulated drift.
        // The very first tick is a free bootstrap (returns a zero wait), so
        // 121 ticks produce 120 waited periods.
        let mut pacer = FramePacer::new();
        let t0 = Instant::now();
        let mut now = t0;
        for _ in 0..121 {
            let wait = pacer.tick(now);
            now += wait;
        }
        assert_eq!(now, t0 + GBA_FRAME_PERIOD * 120);
    }

    #[test]
    fn one_slow_frame_is_not_repaid_with_a_zero_wait_burst() {
        let mut pacer = FramePacer::new();
        let t0 = Instant::now();
        pacer.tick(t0); // next_deadline = t0 + period
                        // Way overdue: this "frame" took 10 whole periods.
        let very_late = t0 + GBA_FRAME_PERIOD * 10;
        let wait = pacer.tick(very_late);
        // Skips straight to the next upcoming boundary...
        assert_eq!(wait, Duration::ZERO);
        // ...and the frame *after* that waits a full period again, rather
        // than firing nine more zero-wait "catch-up" frames.
        let next_wait = pacer.tick(very_late);
        assert_eq!(next_wait, GBA_FRAME_PERIOD);
    }

    #[test]
    fn slightly_late_frame_catches_up_by_less_than_a_full_period() {
        let mut pacer = FramePacer::new();
        let t0 = Instant::now();
        pacer.tick(t0); // next_deadline = t0 + period
        let overshoot = Duration::from_millis(5);
        let wait = pacer.tick(t0 + GBA_FRAME_PERIOD + overshoot);
        // Still behind the *following* boundary by (period - overshoot).
        assert_eq!(wait, GBA_FRAME_PERIOD.checked_sub(overshoot).unwrap());
    }

    #[test]
    fn custom_period_is_honoured() {
        let period = Duration::from_millis(10);
        let mut pacer = FramePacer::with_period(period);
        let t0 = Instant::now();
        pacer.tick(t0);
        assert_eq!(pacer.tick(t0), period);
    }
}
