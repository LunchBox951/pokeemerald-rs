//! Luck class of each Game Corner slot machine.
//!
//! Re-expresses upstream's `GetSlotMachineId` (`src/field_specials.c:1289-1326`);
//! the luck classes are the ids of `include/constants/slot_machine.h:4-12`.
//!
//! The Dewford trend, the news evaluation and the script host are not modelled
//! here: the caller supplies the trend fields and the service-day flag. No RNG
//! is drawn. Playing the machine (reels, payouts) is separate.

use crate::event_data::{EventData, EventDataError, SPECIAL_VARS_START};

/// Number of slot machines in the Game Corner (`SLOT_MACHINE_COUNT`).
pub const SLOT_MACHINE_COUNT: u16 = 12;

/// Script variable holding the machine index (`VAR_0x8004`), the one upstream
/// reads in `GetSlotMachineId`.
pub const SLOT_MACHINE_INDEX_VAR: u16 = SPECIAL_VARS_START + 4;

/// Largest value of a Dewford trend's 7-bit `trendiness` field
/// (`include/global.h`, `struct DewfordTrend`).
pub const MAX_TRENDINESS: u8 = 0x7F;

/// A slot machine's luck class, with the ids of `SLOT_MACHINE_*` in
/// `include/constants/slot_machine.h:4-12`, which scripts and the slot-machine
/// game compare against, so they must not be renumbered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum SlotMachineLuck {
    /// `SLOT_MACHINE_UNLUCKIEST`.
    Unluckiest = 0,
    /// `SLOT_MACHINE_UNLUCKIER`.
    Unluckier = 1,
    /// `SLOT_MACHINE_UNLUCKY`.
    Unlucky = 2,
    /// `SLOT_MACHINE_LUCKY`.
    Lucky = 3,
    /// `SLOT_MACHINE_LUCKIER`.
    Luckier = 4,
    /// `SLOT_MACHINE_LUCKIEST`.
    Luckiest = 5,
}

/// Why a luck class could not be determined.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotMachineLuckError {
    /// The machine index is not below [`SLOT_MACHINE_COUNT`]. Upstream would
    /// read its seed table out of bounds.
    InvalidMachine(u16),
    /// The trendiness exceeds the 7-bit field's [`MAX_TRENDINESS`].
    InvalidTrendiness(u8),
    /// The machine-index variable could not be read.
    Variable(EventDataError),
}

impl std::fmt::Display for SlotMachineLuckError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidMachine(index) => write!(f, "slot machine index {index} is not below 12"),
            Self::InvalidTrendiness(value) => {
                write!(f, "trendiness {value} does not fit the 7-bit field")
            }
            Self::Variable(error) => write!(f, "machine index variable: {error}"),
        }
    }
}

impl std::error::Error for SlotMachineLuckError {}

/// `sSlotMachineRandomSeeds`: the per-machine offset added to the trend sum.
const RANDOM_SEEDS: [u8; SLOT_MACHINE_COUNT as usize] = [12, 2, 4, 5, 1, 8, 7, 11, 3, 10, 9, 6];

/// `sSlotMachineIds`: the class at each residue on an ordinary day.
const ORDINARY_LUCK: [SlotMachineLuck; SLOT_MACHINE_COUNT as usize] = {
    use SlotMachineLuck::{Luckier, Luckiest, Lucky, Unluckier, Unluckiest, Unlucky};
    [
        Unluckiest, Unluckier, Unluckier, Unlucky, Unlucky, Unlucky, Lucky, Lucky, Lucky, Luckier,
        Luckier, Luckiest,
    ]
};

/// `sSlotMachineServiceDayIds`: the class at each residue on the service day.
const SERVICE_DAY_LUCK: [SlotMachineLuck; SLOT_MACHINE_COUNT as usize] = {
    use SlotMachineLuck::{Luckier, Luckiest, Lucky};
    [
        Lucky, Lucky, Lucky, Lucky, Lucky, Lucky, Luckier, Luckier, Luckier, Luckier, Luckiest,
        Luckiest,
    ]
};

/// Determines the luck class of `machine` from the first Dewford trend's
/// `trendiness` and `rand` fields and whether the Game Corner service day is
/// active.
///
/// The sum of the two fields and the machine's seed is taken in 32 bits, so
/// the largest inputs do not wrap a 16-bit add.
///
/// # Errors
///
/// Returns [`SlotMachineLuckError::InvalidMachine`] when `machine` is not
/// below [`SLOT_MACHINE_COUNT`], and [`SlotMachineLuckError::InvalidTrendiness`]
/// when `trendiness` exceeds [`MAX_TRENDINESS`].
pub fn slot_machine_luck(
    machine: u16,
    trendiness: u8,
    rand: u16,
    service_day: bool,
) -> Result<SlotMachineLuck, SlotMachineLuckError> {
    let seed = RANDOM_SEEDS
        .get(usize::from(machine))
        .ok_or(SlotMachineLuckError::InvalidMachine(machine))?;
    if trendiness > MAX_TRENDINESS {
        return Err(SlotMachineLuckError::InvalidTrendiness(trendiness));
    }
    let sum = u32::from(trendiness) + u32::from(rand) + u32::from(*seed);
    let residue = (sum % u32::from(SLOT_MACHINE_COUNT)) as usize;
    Ok(if service_day {
        SERVICE_DAY_LUCK[residue]
    } else {
        ORDINARY_LUCK[residue]
    })
}

/// [`slot_machine_luck`] for the machine whose index script variable
/// ([`SLOT_MACHINE_INDEX_VAR`]) holds, as `specialvar VAR_RESULT,
/// GetSlotMachineId` does. The variable is only read.
///
/// # Errors
///
/// Returns the errors of [`slot_machine_luck`], and
/// [`SlotMachineLuckError::Variable`] if the variable cannot be read.
pub fn slot_machine_luck_from_var(
    data: &EventData,
    trendiness: u8,
    rand: u16,
    service_day: bool,
) -> Result<SlotMachineLuck, SlotMachineLuckError> {
    let machine = data
        .var_get(SLOT_MACHINE_INDEX_VAR)
        .map_err(SlotMachineLuckError::Variable)?;
    slot_machine_luck(machine, trendiness, rand, service_day)
}

#[cfg(test)]
mod tests {
    use super::SlotMachineLuck::{Luckier, Luckiest, Lucky, Unluckier, Unluckiest, Unlucky};
    use super::*;
    use crate::event_data::SPECIAL_VARS_END;

    #[test]
    fn tables_and_ids_match_upstream() {
        assert_eq!(RANDOM_SEEDS, [12, 2, 4, 5, 1, 8, 7, 11, 3, 10, 9, 6]);
        assert_eq!(
            ORDINARY_LUCK,
            [
                Unluckiest, Unluckier, Unluckier, Unlucky, Unlucky, Unlucky, Lucky, Lucky, Lucky,
                Luckier, Luckier, Luckiest
            ]
        );
        assert_eq!(
            SERVICE_DAY_LUCK,
            [
                Lucky, Lucky, Lucky, Lucky, Lucky, Lucky, Luckier, Luckier, Luckier, Luckier,
                Luckiest, Luckiest
            ]
        );
        assert_eq!(SLOT_MACHINE_COUNT, 12);
        assert_eq!(SLOT_MACHINE_INDEX_VAR, 0x8004);
        assert_eq!(Unluckiest as u8, 0);
        assert_eq!(Unluckier as u8, 1);
        assert_eq!(Unlucky as u8, 2);
        assert_eq!(Lucky as u8, 3);
        assert_eq!(Luckier as u8, 4);
        assert_eq!(Luckiest as u8, 5);
    }

    #[test]
    fn every_machine_against_every_residue() {
        let seeds = [12u32, 2, 4, 5, 1, 8, 7, 11, 3, 10, 9, 6];
        let ordinary = [0u8, 1, 1, 2, 2, 2, 3, 3, 3, 4, 4, 5];
        let service = [3u8, 3, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5];
        for machine in 0..12u16 {
            for residue in 0..12u32 {
                // Choose `rand` so the sum with the seed lands on `residue`
                // modulo twelve, past the first multiple so it is non-trivial.
                let rand = (24 + residue + 12 - seeds[usize::from(machine)] % 12) % 12 + 24;
                let rand = u16::try_from(rand).unwrap();
                let sum = u32::from(rand) + seeds[usize::from(machine)];
                assert_eq!(sum % 12, residue, "machine {machine}");
                let got = slot_machine_luck(machine, 0, rand, false).unwrap();
                assert_eq!(got as u8, ordinary[residue as usize], "{machine}/{residue}");
                let got = slot_machine_luck(machine, 0, rand, true).unwrap();
                assert_eq!(
                    got as u8, service[residue as usize],
                    "{machine}/{residue} sd"
                );
            }
        }
    }

    #[test]
    fn trendiness_and_rand_both_enter_the_sum() {
        // Machine 0 (seed 12): 0 + 0 + 12 is residue 0.
        assert_eq!(slot_machine_luck(0, 0, 0, false), Ok(Unluckiest));
        assert_eq!(slot_machine_luck(0, 0, 0, true), Ok(Lucky));
        // Trendiness 3 -> residue 3; rand 3 -> residue 3.
        assert_eq!(slot_machine_luck(0, 3, 0, false), Ok(Unlucky));
        assert_eq!(slot_machine_luck(0, 0, 3, false), Ok(Unlucky));
        // Machine 5 (seed 8), trendiness 1 + rand 1: residue 10.
        assert_eq!(slot_machine_luck(5, 1, 1, false), Ok(Luckier));
        assert_eq!(slot_machine_luck(5, 1, 1, true), Ok(Luckiest));
    }

    #[test]
    fn largest_trend_inputs_do_not_wrap_a_16_bit_add() {
        // 127 + 65535 + 12 = 65674, residue 10; a 16-bit add would give
        // 138, residue 6.
        assert_eq!(slot_machine_luck(0, 127, 0xFFFF, false), Ok(Luckier));
        assert_eq!(slot_machine_luck(0, 127, 0xFFFF, true), Ok(Luckiest));
        // Machine 4 (seed 1): 65663, residue 11; wrapped 127, residue 7.
        assert_eq!(slot_machine_luck(4, 127, 0xFFFF, false), Ok(Luckiest));
        assert_eq!(slot_machine_luck(4, 127, 0xFFFF, true), Ok(Luckiest));
    }

    #[test]
    fn invalid_inputs_are_refused() {
        assert_eq!(
            slot_machine_luck(12, 0, 0, false),
            Err(SlotMachineLuckError::InvalidMachine(12))
        );
        assert_eq!(
            slot_machine_luck(u16::MAX, 127, 0xFFFF, true),
            Err(SlotMachineLuckError::InvalidMachine(u16::MAX))
        );
        assert_eq!(
            slot_machine_luck(0, 128, 0, false),
            Err(SlotMachineLuckError::InvalidTrendiness(128))
        );
    }

    #[test]
    fn wrapper_reads_the_variable_without_changing_state() {
        let mut data = EventData::new();
        data.var_set(SLOT_MACHINE_INDEX_VAR, 6).unwrap();
        let before = data.clone();
        // Machine 6 (seed 7), sum 7: residue 7.
        assert_eq!(slot_machine_luck_from_var(&data, 0, 0, false), Ok(Lucky));
        assert_eq!(slot_machine_luck_from_var(&data, 0, 0, true), Ok(Luckier));
        assert_eq!(data.flag_bytes(), before.flag_bytes());
        assert_eq!(data.vars_raw(), before.vars_raw());
        for id in SPECIAL_VARS_START..=SPECIAL_VARS_END {
            assert_eq!(data.var_get(id), before.var_get(id), "special var {id:#x}");
        }
    }

    #[test]
    fn wrapper_refuses_the_first_invalid_index_and_the_largest() {
        let mut data = EventData::new();
        data.var_set(SLOT_MACHINE_INDEX_VAR, 12).unwrap();
        assert_eq!(
            slot_machine_luck_from_var(&data, 0, 0, false),
            Err(SlotMachineLuckError::InvalidMachine(12))
        );
        data.var_set(SLOT_MACHINE_INDEX_VAR, u16::MAX).unwrap();
        assert_eq!(
            slot_machine_luck_from_var(&data, 127, 0xFFFF, true),
            Err(SlotMachineLuckError::InvalidMachine(u16::MAX))
        );
    }
}
