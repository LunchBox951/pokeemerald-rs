//! Script-visible boolean flags and 16-bit variables.
//!
//! Ordinary values persist in saves; special values last only for the running
//! session. Flag id zero always reads false and ignores writes. Variable ids
//! below [`VARS_START`] are immediate values: reads return the id and writes do
//! nothing.
//!
//! Invalid ids return [`EventDataError::OutOfRange`]. The original game used
//! unchecked fixed-array offsets for these ids; this safe implementation fails
//! closed instead of reproducing an out-of-bounds access.

/// First temporary flag id.
pub const TEMP_FLAGS_START: u16 = 0x0;
/// Last temporary flag id.
pub const TEMP_FLAGS_END: u16 = 0x1F;
/// Number of temporary flag ids.
pub const NUM_TEMP_FLAGS: u16 = TEMP_FLAGS_END - TEMP_FLAGS_START + 1;

const FLAGS_PER_BYTE: usize = u8::BITS as usize;
const DAILY_FLAGS_START: u16 = 0x920;
const DAILY_FLAGS_END: u16 = 0x95F;

/// Number of ids in ordinary flag storage.
pub const FLAGS_COUNT: u16 = DAILY_FLAGS_END + 1;

const _: () = assert!(DAILY_FLAGS_END - DAILY_FLAGS_START + 1 == 64);

/// Byte length of packed ordinary flag storage.
pub const NUM_FLAG_BYTES: usize = (FLAGS_COUNT as usize).div_ceil(FLAGS_PER_BYTE);

/// First session-only flag id.
pub const SPECIAL_FLAGS_START: u16 = 0x4000;
/// Last session-only flag id.
pub const SPECIAL_FLAGS_END: u16 = SPECIAL_FLAGS_START + 0x7F;
/// Number of session-only flag ids.
pub const NUM_SPECIAL_FLAGS: u16 = SPECIAL_FLAGS_END - SPECIAL_FLAGS_START + 1;

const SPECIAL_FLAGS_BYTES: usize = (NUM_SPECIAL_FLAGS as usize).div_ceil(FLAGS_PER_BYTE);

const SYSTEM_FLAGS: u16 = 0x860;
const FLAG_NURSE_UNION_ROOM_REMINDER: u16 = SYSTEM_FLAGS + 0x20;
const FLAG_SYS_USE_FLASH: u16 = SYSTEM_FLAGS + 0x28;
const FLAG_SYS_USE_STRENGTH: u16 = SYSTEM_FLAGS + 0x29;
const FLAG_SYS_CYCLING_ROAD: u16 = SYSTEM_FLAGS + 0x2B;
const FLAG_SYS_SAFARI_MODE: u16 = SYSTEM_FLAGS + 0x2C;
const FLAG_SYS_CRUISE_MODE: u16 = SYSTEM_FLAGS + 0x2D;
/// Set by the White Flute (`ItemUseOutOfBattle_BlackWhiteFlute`, `src/item_use.c:877-890`) while
/// its wild encounter rate increase is active.
pub const FLAG_SYS_ENC_UP_ITEM: u16 = SYSTEM_FLAGS + 0x4D;
/// Set by the Black Flute (`src/item_use.c:877-890`) while its wild encounter rate decrease is
/// active.
pub const FLAG_SYS_ENC_DOWN_ITEM: u16 = SYSTEM_FLAGS + 0x4E;
const FLAG_SYS_CTRL_OBJ_DELETE: u16 = SYSTEM_FLAGS + 0x61;

/// First ordinary variable id.
pub const VARS_START: u16 = 0x4000;
/// Last ordinary variable id.
pub const VARS_END: u16 = 0x40FF;
/// Number of ordinary variable ids.
pub const VARS_COUNT: usize = (VARS_END - VARS_START + 1) as usize;

/// First temporary variable id.
pub const TEMP_VARS_START: u16 = VARS_START;
/// Last temporary variable id.
pub const TEMP_VARS_END: u16 = TEMP_VARS_START + 0xF;
/// Number of temporary variable ids.
pub const NUM_TEMP_VARS: u16 = TEMP_VARS_END - TEMP_VARS_START + 1;

/// First session-only variable id.
pub const SPECIAL_VARS_START: u16 = 0x8000;
/// Last session-only variable id.
pub const SPECIAL_VARS_END: u16 = 0x8015;
/// Number of session-only variable ids.
pub const NUM_SPECIAL_VARS: u16 = SPECIAL_VARS_END - SPECIAL_VARS_START + 1;

/// Remaining Repel steps (`VAR_REPEL_STEP_COUNT`, `include/constants/vars.h:51`). The
/// ordinary variable is saved with the rest of `SaveBlock1`; nonzero means a Repel is active.
pub const VAR_REPEL_STEP_COUNT: u16 = 0x4021;
/// Local day recorded when the Pacifidlog house gave its weekly TM
/// (`VAR_PACIFIDLOG_TM_RECEIVED_DAY`, `include/constants/vars.h:214`).
const VAR_PACIFIDLOG_TM_RECEIVED_DAY: u16 = 0x40C2;
/// Days that must pass before the Pacifidlog house gives another TM
/// (the literal `7` in `GetDaysUntilPacifidlogTMAvailable`, `src/field_specials.c:1558-1563`).
const PACIFIDLOG_TM_COOLDOWN_DAYS: u16 = 7;
/// Wait reported when the local day count is negative (`src/field_specials.c:1560-1561`).
const PACIFIDLOG_TM_NEGATIVE_DAY_WAIT: u16 = 8;
const VAR_ABNORMAL_WEATHER_LOCATION: u16 = 0x4037;
const VAR_SHOULD_END_ABNORMAL_WEATHER: u16 = 0x4039;
const ABNORMAL_WEATHER_NONE: u16 = 0;

/// Errors from flag and variable access.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventDataError {
    /// The id is outside the ordinary and session-only ranges.
    OutOfRange(u16),
}

impl std::fmt::Display for EventDataError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OutOfRange(id) => write!(f, "id {id:#06x} is out of range"),
        }
    }
}

impl std::error::Error for EventDataError {}

enum FlagLocation {
    Null,
    Ordinary { byte: usize, bit: usize },
    Special { byte: usize, bit: usize },
}

fn classify_flag(id: u16) -> Result<FlagLocation, EventDataError> {
    if id == 0 {
        Ok(FlagLocation::Null)
    } else if id < FLAGS_COUNT {
        let idx = usize::from(id);
        Ok(FlagLocation::Ordinary {
            byte: idx / FLAGS_PER_BYTE,
            bit: idx % FLAGS_PER_BYTE,
        })
    } else if (SPECIAL_FLAGS_START..=SPECIAL_FLAGS_END).contains(&id) {
        let idx = usize::from(id - SPECIAL_FLAGS_START);
        Ok(FlagLocation::Special {
            byte: idx / FLAGS_PER_BYTE,
            bit: idx % FLAGS_PER_BYTE,
        })
    } else {
        Err(EventDataError::OutOfRange(id))
    }
}

/// Owned ordinary and session-only event flags and variables.
#[derive(Debug, Clone)]
pub struct EventData {
    flags: [u8; NUM_FLAG_BYTES],
    special_flags: [u8; SPECIAL_FLAGS_BYTES],
    vars: [u16; VARS_COUNT],
    special_vars: [u16; NUM_SPECIAL_VARS as usize],
}

impl Default for EventData {
    fn default() -> Self {
        Self::new()
    }
}

impl EventData {
    /// Creates event data with every flag clear and every variable zero.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            flags: [0; NUM_FLAG_BYTES],
            special_flags: [0; SPECIAL_FLAGS_BYTES],
            vars: [0; VARS_COUNT],
            special_vars: [0; NUM_SPECIAL_VARS as usize],
        }
    }

    /// Returns ordinary flags in their save-file byte layout.
    #[must_use]
    pub const fn flag_bytes(&self) -> &[u8; NUM_FLAG_BYTES] {
        &self.flags
    }

    /// Returns ordinary variables in their save-file word layout.
    #[must_use]
    pub const fn vars_raw(&self) -> &[u16; VARS_COUNT] {
        &self.vars
    }

    /// Restores saved ordinary values with session-only values cleared.
    #[must_use]
    pub const fn from_saved_state(flags: [u8; NUM_FLAG_BYTES], vars: [u16; VARS_COUNT]) -> Self {
        Self {
            flags,
            special_flags: [0; SPECIAL_FLAGS_BYTES],
            vars,
            special_vars: [0; NUM_SPECIAL_VARS as usize],
        }
    }

    /// Reads a flag. Id zero always returns `false`.
    ///
    /// # Errors
    ///
    /// Returns [`EventDataError::OutOfRange`] for an unsupported id.
    pub fn flag_get(&self, id: u16) -> Result<bool, EventDataError> {
        Ok(match classify_flag(id)? {
            FlagLocation::Null => false,
            FlagLocation::Ordinary { byte, bit } => (self.flags[byte] >> bit) & 1 != 0,
            FlagLocation::Special { byte, bit } => (self.special_flags[byte] >> bit) & 1 != 0,
        })
    }

    /// Sets a flag. Id zero is ignored.
    ///
    /// # Errors
    ///
    /// Returns [`EventDataError::OutOfRange`] for an unsupported id.
    pub fn flag_set(&mut self, id: u16) -> Result<(), EventDataError> {
        match classify_flag(id)? {
            FlagLocation::Null => {}
            FlagLocation::Ordinary { byte, bit } => self.flags[byte] |= 1 << bit,
            FlagLocation::Special { byte, bit } => self.special_flags[byte] |= 1 << bit,
        }
        Ok(())
    }

    /// Clears a flag. Id zero is ignored.
    ///
    /// # Errors
    ///
    /// Returns [`EventDataError::OutOfRange`] for an unsupported id.
    pub fn flag_clear(&mut self, id: u16) -> Result<(), EventDataError> {
        match classify_flag(id)? {
            FlagLocation::Null => {}
            FlagLocation::Ordinary { byte, bit } => self.flags[byte] &= !(1 << bit),
            FlagLocation::Special { byte, bit } => self.special_flags[byte] &= !(1 << bit),
        }
        Ok(())
    }

    fn clear_known_ordinary_flag(&mut self, id: u16) {
        debug_assert!(
            id != 0 && id < FLAGS_COUNT,
            "id {id} must be an ordinary flag id"
        );
        let idx = usize::from(id);
        self.flags[idx / FLAGS_PER_BYTE] &= !(1 << (idx % FLAGS_PER_BYTE));
    }

    /// Reads a variable, or returns an immediate id below [`VARS_START`].
    ///
    /// # Errors
    ///
    /// Returns [`EventDataError::OutOfRange`] for an unsupported id.
    pub fn var_get(&self, id: u16) -> Result<u16, EventDataError> {
        if id < VARS_START {
            Ok(id)
        } else if id <= VARS_END {
            Ok(self.vars[usize::from(id - VARS_START)])
        } else if (SPECIAL_VARS_START..=SPECIAL_VARS_END).contains(&id) {
            Ok(self.special_vars[usize::from(id - SPECIAL_VARS_START)])
        } else {
            Err(EventDataError::OutOfRange(id))
        }
    }

    /// Writes a variable. Immediate ids below [`VARS_START`] are ignored.
    ///
    /// # Errors
    ///
    /// Returns [`EventDataError::OutOfRange`] for an unsupported id.
    pub fn var_set(&mut self, id: u16, value: u16) -> Result<(), EventDataError> {
        if id < VARS_START {
            Ok(())
        } else if id <= VARS_END {
            self.vars[usize::from(id - VARS_START)] = value;
            Ok(())
        } else if (SPECIAL_VARS_START..=SPECIAL_VARS_END).contains(&id) {
            self.special_vars[usize::from(id - SPECIAL_VARS_START)] = value;
            Ok(())
        } else {
            Err(EventDataError::OutOfRange(id))
        }
    }

    /// Days until the Pacifidlog house gives another TM, given the local day
    /// count `local_days` (`GetDaysUntilPacifidlogTMAvailable`,
    /// `src/field_specials.c:1555-1564`).
    ///
    /// Upstream's branch order is kept: zero once seven days have passed, then
    /// eight for a negative `local_days`, otherwise the remaining days. The
    /// difference is signed, as in C's promoted `int` arithmetic, so a recorded
    /// day ahead of `local_days` is not clamped: the result exceeds seven, and
    /// past `u16::MAX` it wraps as the `u16` return does upstream.
    #[must_use]
    pub fn days_until_pacifidlog_tm_available(&self, local_days: i16) -> u16 {
        let received_day = self.vars[usize::from(VAR_PACIFIDLOG_TM_RECEIVED_DAY - VARS_START)];
        let elapsed = i32::from(local_days) - i32::from(received_day);
        if elapsed >= i32::from(PACIFIDLOG_TM_COOLDOWN_DAYS) {
            0
        } else if local_days < 0 {
            PACIFIDLOG_TM_NEGATIVE_DAY_WAIT
        } else {
            PACIFIDLOG_TM_COOLDOWN_DAYS
                .wrapping_sub(local_days.cast_unsigned().wrapping_sub(received_day))
        }
    }

    /// Records `local_days` as the day the Pacifidlog TM was received and
    /// returns the stored value (`SetPacifidlogTMReceivedDay`,
    /// `src/field_specials.c:1566-1570`).
    ///
    /// A negative `local_days` is stored as its two's-complement `u16`, as the
    /// `s16` to `u16` assignment does upstream, and is not normalised.
    pub fn set_pacifidlog_tm_received_day(&mut self, local_days: i16) -> u16 {
        let stored = local_days.cast_unsigned();
        self.vars[usize::from(VAR_PACIFIDLOG_TM_RECEIVED_DAY - VARS_START)] = stored;
        stored
    }

    /// Clears per-map temporary values and related system flags.
    ///
    /// All other values remain unchanged.
    pub fn clear_temp_field_event_data(&mut self) {
        let first_temp_flag_byte = usize::from(TEMP_FLAGS_START) / FLAGS_PER_BYTE;
        let last_temp_flag_byte = usize::from(TEMP_FLAGS_END) / FLAGS_PER_BYTE;
        self.flags[first_temp_flag_byte..=last_temp_flag_byte].fill(0);

        let first_temp_var = usize::from(TEMP_VARS_START - VARS_START);
        let last_temp_var = usize::from(TEMP_VARS_END - VARS_START);
        self.vars[first_temp_var..=last_temp_var].fill(0);

        self.clear_known_ordinary_flag(FLAG_SYS_ENC_UP_ITEM);
        self.clear_known_ordinary_flag(FLAG_SYS_ENC_DOWN_ITEM);
        self.clear_known_ordinary_flag(FLAG_SYS_USE_STRENGTH);
        self.clear_known_ordinary_flag(FLAG_SYS_CTRL_OBJ_DELETE);
        self.clear_known_ordinary_flag(FLAG_NURSE_UNION_ROOM_REMINDER);
    }

    /// Clears the persistent field-mode flags and, if pending, ends the
    /// abnormal weather a white-out must not let survive into the next save
    /// (`Overworld_ResetStateAfterWhiteOut`, `src/overworld.c:399-413`).
    ///
    /// All other values remain unchanged.
    pub fn clear_white_out_state(&mut self) {
        for flag in [
            FLAG_SYS_CYCLING_ROAD,
            FLAG_SYS_CRUISE_MODE,
            FLAG_SYS_SAFARI_MODE,
            FLAG_SYS_USE_STRENGTH,
            FLAG_SYS_USE_FLASH,
        ] {
            self.clear_known_ordinary_flag(flag);
        }

        // A Kyogre/Groudon defeat that already maxed the step counter ends
        // the abnormal weather rather than leaving it pending forever.
        let should_end = usize::from(VAR_SHOULD_END_ABNORMAL_WEATHER - VARS_START);
        if self.vars[should_end] == 1 {
            self.vars[should_end] = 0;
            self.vars[usize::from(VAR_ABNORMAL_WEATHER_LOCATION - VARS_START)] =
                ABNORMAL_WEATHER_NONE;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_space_constants_match_upstream_headers() {
        assert_eq!(FLAGS_COUNT, 0x0960);
        assert_eq!(NUM_FLAG_BYTES, 300);
        assert_eq!(DAILY_FLAGS_END - DAILY_FLAGS_START + 1, 64);
        assert_eq!(SPECIAL_FLAGS_START, 0x4000);
        assert_eq!(SPECIAL_FLAGS_END, 0x407F);
        assert_eq!(NUM_SPECIAL_FLAGS, 128);
        assert_eq!(NUM_TEMP_FLAGS, 32);

        assert_eq!(VARS_START, 0x4000);
        assert_eq!(VARS_END, 0x40FF);
        assert_eq!(VARS_COUNT, 256);
        assert_eq!(TEMP_VARS_START, 0x4000);
        assert_eq!(TEMP_VARS_END, 0x400F);
        assert_eq!(NUM_TEMP_VARS, 16);
        assert_eq!(SPECIAL_VARS_START, 0x8000);
        assert_eq!(SPECIAL_VARS_END, 0x8015);
        assert_eq!(NUM_SPECIAL_VARS, 22);
    }

    #[test]
    fn repel_step_count_is_an_ordinary_variable_that_survives_temporary_clearing() {
        assert_eq!(VAR_REPEL_STEP_COUNT, 0x4021);
        assert!((VARS_START..=VARS_END).contains(&VAR_REPEL_STEP_COUNT));
        const { assert!(VAR_REPEL_STEP_COUNT > TEMP_VARS_END) };
        let mut data = EventData::default();
        data.var_set(VAR_REPEL_STEP_COUNT, 37).unwrap();
        data.clear_temp_field_event_data();
        assert_eq!(data.var_get(VAR_REPEL_STEP_COUNT), Ok(37));
    }

    #[test]
    fn fresh_store_has_every_flag_clear_and_every_var_zero() {
        let data = EventData::new();
        assert_eq!(data.flag_get(1), Ok(false));
        assert_eq!(data.flag_get(FLAGS_COUNT - 1), Ok(false));
        assert_eq!(data.flag_get(SPECIAL_FLAGS_START), Ok(false));
        assert_eq!(data.flag_get(SPECIAL_FLAGS_END), Ok(false));
        assert_eq!(data.var_get(VARS_START), Ok(0));
        assert_eq!(data.var_get(VARS_END), Ok(0));
    }

    #[test]
    fn default_matches_new() {
        let data = EventData::default();
        assert_eq!(data.flag_get(1), Ok(false));
        assert_eq!(data.var_get(VARS_START), Ok(0));
    }

    #[test]
    fn flag_round_trips_through_set_and_clear() {
        let mut data = EventData::new();
        let id = 100;
        assert_eq!(data.flag_get(id), Ok(false));
        data.flag_set(id).unwrap();
        assert_eq!(data.flag_get(id), Ok(true));
        data.flag_clear(id).unwrap();
        assert_eq!(data.flag_get(id), Ok(false));
    }

    #[test]
    fn flag_bit_packing_does_not_disturb_neighboring_bits() {
        let first_flag_in_second_byte = 8;
        let last_flag_in_first_byte = first_flag_in_second_byte - 1;
        let preceding_flag = last_flag_in_first_byte - 1;
        let mut data = EventData::new();
        data.flag_set(last_flag_in_first_byte).unwrap();
        assert_eq!(data.flag_get(last_flag_in_first_byte), Ok(true));
        assert_eq!(data.flag_get(first_flag_in_second_byte), Ok(false));
        assert_eq!(data.flag_get(preceding_flag), Ok(false));

        data.flag_set(first_flag_in_second_byte).unwrap();
        assert_eq!(
            data.flag_get(last_flag_in_first_byte),
            Ok(true),
            "setting the next byte must not clear the previous byte"
        );
        assert_eq!(data.flag_get(first_flag_in_second_byte), Ok(true));

        data.flag_clear(last_flag_in_first_byte).unwrap();
        assert_eq!(data.flag_get(last_flag_in_first_byte), Ok(false));
        assert_eq!(
            data.flag_get(first_flag_in_second_byte),
            Ok(true),
            "clearing the previous byte must not clear the next byte"
        );
    }

    #[test]
    fn flag_first_and_last_ordinary_ids_round_trip() {
        let mut data = EventData::new();
        data.flag_set(1).unwrap();
        data.flag_set(FLAGS_COUNT - 1).unwrap();
        assert_eq!(data.flag_get(1), Ok(true));
        assert_eq!(data.flag_get(FLAGS_COUNT - 1), Ok(true));
    }

    #[test]
    fn flag_id_zero_is_tolerated_as_a_permanent_no_op() {
        let mut data = EventData::new();
        assert_eq!(data.flag_get(0), Ok(false));
        assert_eq!(data.flag_set(0), Ok(()));
        assert_eq!(data.flag_get(0), Ok(false));
        assert_eq!(data.flag_clear(0), Ok(()));
    }

    #[test]
    fn flag_special_range_first_and_last_ids_round_trip() {
        let mut data = EventData::new();
        data.flag_set(SPECIAL_FLAGS_START).unwrap();
        data.flag_set(SPECIAL_FLAGS_END).unwrap();
        assert_eq!(data.flag_get(SPECIAL_FLAGS_START), Ok(true));
        assert_eq!(data.flag_get(SPECIAL_FLAGS_END), Ok(true));
        assert_eq!(data.flag_get(1), Ok(false));
    }

    #[test]
    fn flag_gap_between_ordinary_and_special_is_out_of_range() {
        let data = EventData::new();
        assert_eq!(
            data.flag_get(FLAGS_COUNT),
            Err(EventDataError::OutOfRange(FLAGS_COUNT))
        );
        assert_eq!(
            data.flag_get(SPECIAL_FLAGS_START - 1),
            Err(EventDataError::OutOfRange(SPECIAL_FLAGS_START - 1))
        );
    }

    #[test]
    fn flag_past_special_range_is_out_of_range() {
        let mut data = EventData::new();
        assert_eq!(
            data.flag_get(SPECIAL_FLAGS_END + 1),
            Err(EventDataError::OutOfRange(SPECIAL_FLAGS_END + 1))
        );
        assert_eq!(
            data.flag_set(u16::MAX),
            Err(EventDataError::OutOfRange(u16::MAX))
        );
    }

    #[test]
    fn var_round_trips_through_set() {
        let mut data = EventData::new();
        data.var_set(VARS_START, 42).unwrap();
        assert_eq!(data.var_get(VARS_START), Ok(42));
        data.var_set(VARS_END, 0xBEEF).unwrap();
        assert_eq!(data.var_get(VARS_END), Ok(0xBEEF));
        assert_eq!(data.var_get(VARS_START), Ok(42));
    }

    #[test]
    fn var_id_below_vars_start_is_an_immediate_passthrough() {
        let mut data = EventData::new();
        assert_eq!(data.var_get(0), Ok(0));
        assert_eq!(data.var_get(0x1234), Ok(0x1234));
        assert_eq!(data.var_set(0x1234, 0xFFFF), Ok(()));
        assert_eq!(data.var_get(0x1234), Ok(0x1234));
    }

    #[test]
    fn var_special_range_round_trips_through_set() {
        let mut data = EventData::new();
        assert_eq!(data.var_get(SPECIAL_VARS_START), Ok(0));
        assert_eq!(data.var_get(SPECIAL_VARS_END), Ok(0));
        data.var_set(SPECIAL_VARS_START, 42).unwrap();
        data.var_set(SPECIAL_VARS_END, 0xBEEF).unwrap();
        assert_eq!(data.var_get(SPECIAL_VARS_START), Ok(42));
        assert_eq!(data.var_get(SPECIAL_VARS_END), Ok(0xBEEF));
        assert_eq!(data.var_get(VARS_START), Ok(0));
    }

    #[test]
    fn var_gap_between_ordinary_and_special_is_out_of_range() {
        let data = EventData::new();
        assert_eq!(
            data.var_get(VARS_END + 1),
            Err(EventDataError::OutOfRange(VARS_END + 1))
        );
        assert_eq!(
            data.var_get(SPECIAL_VARS_START - 1),
            Err(EventDataError::OutOfRange(SPECIAL_VARS_START - 1))
        );
    }

    #[test]
    fn var_past_special_range_is_out_of_range() {
        let data = EventData::new();
        assert_eq!(
            data.var_get(SPECIAL_VARS_END + 1),
            Err(EventDataError::OutOfRange(SPECIAL_VARS_END + 1))
        );
        assert_eq!(
            data.var_get(u16::MAX),
            Err(EventDataError::OutOfRange(u16::MAX))
        );
    }

    #[test]
    fn clear_temp_field_event_data_clears_only_temp_state_and_named_system_flags() {
        let mut data = EventData::new();

        let first_stored_temp_flag = TEMP_FLAGS_START + 1;
        data.flag_set(first_stored_temp_flag).unwrap();
        data.flag_set(TEMP_FLAGS_END).unwrap();
        data.var_set(TEMP_VARS_START, 7).unwrap();
        data.var_set(TEMP_VARS_END, 9).unwrap();

        data.flag_set(FLAG_SYS_ENC_UP_ITEM).unwrap();
        data.flag_set(FLAG_SYS_ENC_DOWN_ITEM).unwrap();
        data.flag_set(FLAG_SYS_USE_STRENGTH).unwrap();
        data.flag_set(FLAG_SYS_CTRL_OBJ_DELETE).unwrap();
        data.flag_set(FLAG_NURSE_UNION_ROOM_REMINDER).unwrap();

        let outside_flag = TEMP_FLAGS_END + 1;
        let outside_var = TEMP_VARS_END + 1;
        data.flag_set(outside_flag).unwrap();
        data.var_set(outside_var, 123).unwrap();

        data.clear_temp_field_event_data();

        assert_eq!(data.flag_get(first_stored_temp_flag), Ok(false));
        assert_eq!(data.flag_get(TEMP_FLAGS_END), Ok(false));
        assert_eq!(data.var_get(TEMP_VARS_START), Ok(0));
        assert_eq!(data.var_get(TEMP_VARS_END), Ok(0));
        assert_eq!(data.flag_get(FLAG_SYS_ENC_UP_ITEM), Ok(false));
        assert_eq!(data.flag_get(FLAG_SYS_ENC_DOWN_ITEM), Ok(false));
        assert_eq!(data.flag_get(FLAG_SYS_USE_STRENGTH), Ok(false));
        assert_eq!(data.flag_get(FLAG_SYS_CTRL_OBJ_DELETE), Ok(false));
        assert_eq!(data.flag_get(FLAG_NURSE_UNION_ROOM_REMINDER), Ok(false));

        assert_eq!(
            data.flag_get(outside_flag),
            Ok(true),
            "flags outside the temp range must survive the clear"
        );
        assert_eq!(
            data.var_get(outside_var),
            Ok(123),
            "vars outside the temp range must survive the clear"
        );
    }

    #[test]
    fn clear_white_out_state_clears_field_mode_flags_and_ends_pending_abnormal_weather() {
        let mut data = EventData::new();

        data.flag_set(FLAG_SYS_CYCLING_ROAD).unwrap();
        data.flag_set(FLAG_SYS_CRUISE_MODE).unwrap();
        data.flag_set(FLAG_SYS_SAFARI_MODE).unwrap();
        data.flag_set(FLAG_SYS_USE_STRENGTH).unwrap();
        data.flag_set(FLAG_SYS_USE_FLASH).unwrap();
        data.var_set(VAR_SHOULD_END_ABNORMAL_WEATHER, 1).unwrap();
        data.var_set(VAR_ABNORMAL_WEATHER_LOCATION, 3).unwrap();

        let unrelated_flag = FLAG_SYS_ENC_UP_ITEM;
        data.flag_set(unrelated_flag).unwrap();

        data.clear_white_out_state();

        assert_eq!(data.flag_get(FLAG_SYS_CYCLING_ROAD), Ok(false));
        assert_eq!(data.flag_get(FLAG_SYS_CRUISE_MODE), Ok(false));
        assert_eq!(data.flag_get(FLAG_SYS_SAFARI_MODE), Ok(false));
        assert_eq!(data.flag_get(FLAG_SYS_USE_STRENGTH), Ok(false));
        assert_eq!(data.flag_get(FLAG_SYS_USE_FLASH), Ok(false));
        assert_eq!(data.var_get(VAR_SHOULD_END_ABNORMAL_WEATHER), Ok(0));
        assert_eq!(data.var_get(VAR_ABNORMAL_WEATHER_LOCATION), Ok(0));
        assert_eq!(
            data.flag_get(unrelated_flag),
            Ok(true),
            "flags outside the named set must survive the clear"
        );
    }

    #[test]
    fn clear_white_out_state_leaves_abnormal_weather_alone_when_not_pending() {
        let mut data = EventData::new();
        data.var_set(VAR_ABNORMAL_WEATHER_LOCATION, 3).unwrap();

        data.clear_white_out_state();

        assert_eq!(
            data.var_get(VAR_SHOULD_END_ABNORMAL_WEATHER),
            Ok(0),
            "setup: was never pending"
        );
        assert_eq!(
            data.var_get(VAR_ABNORMAL_WEATHER_LOCATION),
            Ok(3),
            "a location with no pending end-request must survive the clear"
        );
    }

    #[test]
    fn saved_state_round_trips_ordinary_flags_and_vars_only() {
        let mut data = EventData::new();
        data.flag_set(100).unwrap();
        data.var_set(VARS_START, 0xBEEF).unwrap();
        data.flag_set(SPECIAL_FLAGS_START).unwrap();
        data.var_set(SPECIAL_VARS_START, 999).unwrap();

        let restored = EventData::from_saved_state(*data.flag_bytes(), *data.vars_raw());

        assert_eq!(restored.flag_get(100), Ok(true));
        assert_eq!(restored.var_get(VARS_START), Ok(0xBEEF));
        assert_eq!(restored.flag_get(SPECIAL_FLAGS_START), Ok(false));
        assert_eq!(restored.var_get(SPECIAL_VARS_START), Ok(0));
    }

    const PACIFIDLOG_TM_VAR: u16 = 0x40C2;

    fn pacifidlog_data_received_on(day: u16) -> EventData {
        let mut data = EventData::new();
        data.var_set(PACIFIDLOG_TM_VAR, day).unwrap();
        data
    }

    #[test]
    fn pacifidlog_tm_wait_counts_down_and_reaches_zero_at_seven_days() {
        let data = pacifidlog_data_received_on(10);
        let waits: [(i16, u16); 7] = [
            (10, 7),
            (11, 6),
            (12, 5),
            (14, 3),
            (15, 2),
            (16, 1),
            (17, 0),
        ];
        for (local_days, expected) in waits {
            assert_eq!(
                data.days_until_pacifidlog_tm_available(local_days),
                expected,
                "local day {local_days}"
            );
        }
        assert_eq!(data.days_until_pacifidlog_tm_available(18), 0);
        assert_eq!(data.days_until_pacifidlog_tm_available(i16::MAX), 0);
    }

    #[test]
    fn pacifidlog_tm_wait_on_fresh_data_is_a_full_week_on_day_zero() {
        let data = EventData::new();
        assert_eq!(data.days_until_pacifidlog_tm_available(0), 7);
        assert_eq!(data.days_until_pacifidlog_tm_available(6), 1);
        assert_eq!(data.days_until_pacifidlog_tm_available(7), 0);
    }

    #[test]
    fn pacifidlog_tm_wait_for_a_recorded_day_ahead_is_not_clamped() {
        let data = pacifidlog_data_received_on(10);
        assert_eq!(data.days_until_pacifidlog_tm_available(9), 8);
        assert_eq!(data.days_until_pacifidlog_tm_available(3), 14);
        assert_eq!(data.days_until_pacifidlog_tm_available(0), 17);
    }

    #[test]
    fn pacifidlog_tm_wait_wraps_at_the_u16_return_edge() {
        let data = pacifidlog_data_received_on(u16::MAX);
        assert_eq!(data.days_until_pacifidlog_tm_available(0), 6);
        assert_eq!(data.days_until_pacifidlog_tm_available(5), 1);
        assert_eq!(data.days_until_pacifidlog_tm_available(6), 0);
        assert_eq!(data.days_until_pacifidlog_tm_available(7), 65535);

        let data = pacifidlog_data_received_on(0x7FFF);
        assert_eq!(data.days_until_pacifidlog_tm_available(0), 0x8006);
        assert_eq!(data.days_until_pacifidlog_tm_available(i16::MAX), 7);
    }

    #[test]
    fn pacifidlog_tm_wait_for_a_negative_local_day_is_eight() {
        for received in [0, 1, 0x7FFF, 0x8000, u16::MAX] {
            let data = pacifidlog_data_received_on(received);
            assert_eq!(data.days_until_pacifidlog_tm_available(-1), 8, "{received}");
            assert_eq!(
                data.days_until_pacifidlog_tm_available(i16::MIN),
                8,
                "{received}"
            );
        }
        let data = pacifidlog_data_received_on(0);
        assert_eq!(data.days_until_pacifidlog_tm_available(0), 7);
    }

    #[test]
    fn pacifidlog_tm_received_day_is_stored_and_returned_as_u16() {
        let cases: [(i16, u16); 6] = [
            (0, 0),
            (1, 1),
            (i16::MAX, 0x7FFF),
            (-1, 0xFFFF),
            (-2, 0xFFFE),
            (i16::MIN, 0x8000),
        ];
        for (local_days, expected) in cases {
            let mut data = EventData::new();
            assert_eq!(data.set_pacifidlog_tm_received_day(local_days), expected);
            assert_eq!(
                data.var_get(PACIFIDLOG_TM_VAR),
                Ok(expected),
                "{local_days}"
            );
        }
    }

    #[test]
    fn pacifidlog_tm_received_day_overwrites_the_previous_day_and_starts_a_new_wait() {
        let mut data = pacifidlog_data_received_on(10);
        assert_eq!(data.days_until_pacifidlog_tm_available(20), 0);
        data.set_pacifidlog_tm_received_day(20);
        assert_eq!(data.days_until_pacifidlog_tm_available(20), 7);
        assert_eq!(data.days_until_pacifidlog_tm_available(26), 1);
        assert_eq!(data.days_until_pacifidlog_tm_available(27), 0);
    }

    #[test]
    fn pacifidlog_tm_received_day_after_a_negative_day_reads_as_ahead_of_day_zero() {
        let mut data = EventData::new();
        data.set_pacifidlog_tm_received_day(-1);
        assert_eq!(data.days_until_pacifidlog_tm_available(-1), 8);
        assert_eq!(data.days_until_pacifidlog_tm_available(0), 6);
    }

    #[test]
    fn pacifidlog_tm_helpers_touch_only_their_own_variable() {
        let mut data = EventData::new();
        data.flag_set(100).unwrap();
        data.var_set(VARS_START, 0xBEEF).unwrap();
        data.var_set(PACIFIDLOG_TM_VAR - 1, 0x4321).unwrap();
        data.var_set(PACIFIDLOG_TM_VAR + 1, 0x1234).unwrap();
        data.var_set(SPECIAL_VARS_START, 999).unwrap();
        let mut expected = data.clone();
        expected.var_set(PACIFIDLOG_TM_VAR, 42).unwrap();

        data.set_pacifidlog_tm_received_day(42);
        let _ = data.days_until_pacifidlog_tm_available(45);

        assert_eq!(data.flag_bytes(), expected.flag_bytes());
        assert_eq!(data.vars_raw(), expected.vars_raw());
        assert_eq!(data.var_get(SPECIAL_VARS_START), Ok(999));
    }

    #[test]
    fn pacifidlog_tm_received_day_survives_a_save_block_round_trip() {
        use crate::save::block::SaveBlock1;

        let mut block = SaveBlock1::default();
        block.event_data.set_pacifidlog_tm_received_day(-3);
        let bytes = block.to_bytes(0x1234_5678);
        let restored = SaveBlock1::from_bytes(&bytes, 0x1234_5678).unwrap();

        assert_eq!(restored.event_data.var_get(PACIFIDLOG_TM_VAR), Ok(0xFFFD));
        assert_eq!(
            restored.event_data.days_until_pacifidlog_tm_available(-3),
            8
        );
        assert_eq!(restored.event_data.days_until_pacifidlog_tm_available(0), 4);
    }

    #[test]
    fn error_display_is_human_readable() {
        let out_of_range = EventDataError::OutOfRange(0x1000);
        assert_eq!(out_of_range.to_string(), "id 0x1000 is out of range");
    }
}
