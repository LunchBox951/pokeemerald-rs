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
        (seed >> 8) as u16 & 0x7FFF
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
        let mut shifts = 1;
        loop {
            noise.shift_lfsr(1);
            shifts += 1;
            if noise.lfsr == first {
                break;
            }
        }
        assert_eq!(shifts - 1, period);
    }
}
