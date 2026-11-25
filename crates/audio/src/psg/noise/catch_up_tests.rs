//! Bounded LFSR catch-up against the one-shift-per-owed-period replay.

use super::*;

/// The unbounded catch-up: one `shift_lfsr` per owed period.
fn replay_every_owed_shift(noise: &mut NoiseChannel, volume: u8) {
    while noise.elapsed >= noise.period {
        noise.elapsed -= noise.period;
        noise.shift_lfsr(volume);
    }
}

fn observable(noise: &NoiseChannel) -> (u16, i8, u8, u8, u64) {
    (
        noise.lfsr,
        noise.output,
        noise.level,
        noise.centre,
        noise.elapsed,
    )
}

#[test]
fn slow_to_fast_retune_bounds_the_owed_shifts_in_one_sample() {
    // 2_926 samples at 0xD7 leave nearly one old period
    // accrued, and 0x00 then owes 114_701 transitions in a single sample.
    let mut noise = NoiseChannel::from_control_byte(0xD7);
    for _ in 0..2_926 {
        noise.clock_sample(15);
    }
    assert_eq!(noise.shifts_run, 0);
    noise.retune(0x00);
    let mut reference = noise.clone();
    reference.elapsed += TICKS_PER_SAMPLE;
    replay_every_owed_shift(&mut reference, 15);
    assert_eq!(reference.shifts_run, 114_701);

    noise.clock_sample(15);
    // 114_701 owed = 3 full cycles + 16_400, and never above one cycle.
    assert_eq!(noise.shifts_run, 16_400);
    assert!(noise.shifts_run <= LfsrWidth::WIDE_PERIOD);
    assert_eq!(observable(&noise), observable(&reference));
}

#[test]
fn bounded_catch_up_matches_the_unbounded_replay_bit_for_bit() {
    // Wide: sampled states (every state is on the one 32_767 cycle, bar
    // the fixed point). Narrow: every low-seven-bit state under four
    // upper patterns, through the eight-shift lead.
    let mut seed = 0x1234_5678_u32;
    let mut next = move || {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        u16::try_from((seed >> 8) & 0x7FFF).expect("fifteen bits")
    };
    let wide_owed = [
        1, 2, 32_766, 32_767, 32_768, 65_534, 65_535, 65_536, 114_701,
    ];
    let mut wide_states = vec![0, 0x4000, 0x7FFF, 1, 0x3FFF];
    wide_states.extend((0..24).map(|_| next()));
    let narrow_owed: Vec<u64> = (0..=300).chain([1_000, 4_321]).collect();
    let narrow_states: Vec<u16> = [0, 0x7F80, 0x5500, 0x2A80]
        .into_iter()
        .flat_map(|upper| (0..0x80).map(move |lower| upper | lower))
        .collect();
    let cases = [
        (LfsrWidth::FifteenBit, &wide_owed[..], wide_states),
        (LfsrWidth::SevenBit, &narrow_owed[..], narrow_states),
    ];
    for (width, owed_counts, states) in cases {
        for lfsr in states {
            for &owed in owed_counts {
                let mut bounded = NoiseChannel::from_control_byte(0);
                bounded.width = width;
                bounded.lfsr = lfsr;
                bounded.period = 7;
                bounded.elapsed = owed * 7 + 3;
                bounded.level = 6;
                let mut reference = bounded.clone();
                bounded.settle_owed_clocks(9);
                replay_every_owed_shift(&mut reference, 9);
                assert_eq!(
                    observable(&bounded),
                    observable(&reference),
                    "{width:?} state {lfsr:#06x} owed {owed}"
                );
                let bound = match width {
                    LfsrWidth::FifteenBit => LfsrWidth::WIDE_PERIOD,
                    LfsrWidth::SevenBit => 8 + LfsrWidth::NARROW_PERIOD - 1,
                };
                assert!(bounded.shifts_run <= bound.min(owed));
            }
        }
    }
}

/// Replays `step` until it returns `first`, giving up after `bound` steps.
///
/// `Ok` holds the steps taken to return; `Err` holds the state reached when
/// the bound ran out, so a generator stuck in another cycle or fixed point
/// fails with its state instead of hanging the test.
fn steps_until_return(first: u16, bound: u64, mut step: impl FnMut() -> u16) -> Result<u64, u16> {
    let mut state = first;
    for steps in 1..=bound {
        state = step();
        if state == first {
            return Ok(steps);
        }
    }
    Err(state)
}

#[test]
fn the_lfsr_periods_match_the_catch_up_moduli() {
    for (byte, period) in [
        (0, LfsrWidth::WIDE_PERIOD),
        (NoiseControl::WIDTH_BIT, LfsrWidth::NARROW_PERIOD),
    ] {
        let mut noise = NoiseChannel::from_control_byte(byte);
        noise.period = 1;
        noise.shift_lfsr(1);
        let first = noise.lfsr;
        let returned_after = steps_until_return(first, period, || {
            noise.shift_lfsr(1);
            noise.lfsr
        });
        assert_eq!(
            returned_after,
            Ok(period),
            "{byte:#04x}: first state {first:#06x} did not return within {period} shifts"
        );
    }
}

#[test]
fn a_generator_that_never_returns_fails_with_its_state_instead_of_hanging() {
    // A fixed point away from the first state.
    assert_eq!(steps_until_return(0x1234, 127, || 0x0001), Err(0x0001));
    // A cycle that never revisits the first state.
    let mut toggle = 0u16;
    assert_eq!(
        steps_until_return(0x1234, 127, || {
            toggle ^= 1;
            toggle
        }),
        Err(1)
    );
    let mut count = 0;
    assert_eq!(
        steps_until_return(7, 3, || {
            count += 1;
            if count == 3 {
                7
            } else {
                0
            }
        }),
        Ok(3)
    );
}
