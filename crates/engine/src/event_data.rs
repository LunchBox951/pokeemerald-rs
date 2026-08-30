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
const FLAG_SYS_USE_STRENGTH: u16 = SYSTEM_FLAGS + 0x29;
const FLAG_SYS_ENC_UP_ITEM: u16 = SYSTEM_FLAGS + 0x4D;
const FLAG_SYS_ENC_DOWN_ITEM: u16 = SYSTEM_FLAGS + 0x4E;
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

    #[test]
    fn error_display_is_human_readable() {
        let out_of_range = EventDataError::OutOfRange(0x1000);
        assert_eq!(out_of_range.to_string(), "id 0x1000 is out of range");
    }
}
