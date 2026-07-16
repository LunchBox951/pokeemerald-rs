//! Event flags and vars — the boolean/16-bit state scripts and the overworld
//! read and write (S-5).
//!
//! Behavioural re-implementation `(behavioral-fidelity)` of
//! `pokeemerald/src/event_data.c` + `include/event_data.h`'s `FlagGet`/
//! `FlagSet`/`FlagClear`, `VarGet`/`VarSet`, and `ClearTempFieldEventData`,
//! plus the id-space layout constants from `include/constants/flags.h` and
//! `include/constants/vars.h`. The backing storage is an owned [`EventData`]
//! value — upstream's `gSaveBlock1Ptr->flags`/`vars` indirection and the
//! static `sSpecialFlags` array become plain fields `(oop-boundaries)`; no
//! global mutable state.
//!
//! # Scope
//!
//! Per issue #65: the store, its accessors, and the id-space semantics only.
//! Deferred: the `gSpecialVars` dispatch table (those globals, e.g.
//! `gSpecialVar_LastTalked`, don't exist yet — [`EventData::var_get`]/
//! [`EventData::var_set`] surface the range as
//! [`EventDataError::SpecialVar`] rather than silently misreading it),
//! `VarGetObjectEventGraphicsId` (no object events yet), save-block
//! persistence, `ClearDailyFlags` (RTC-tied, explicitly out of scope —
//! `DAILY_FLAGS_START`/`DAILY_FLAGS_END` still appear below, private,
//! because `FLAGS_COUNT` is defined in terms of them upstream), and any
//! script command (this module only provides the primitives a later slice
//! wires into the command table).
//!
//! # Id spaces
//!
//! Flag ids and var ids are separate namespaces, each with an "ordinary"
//! range plus special sub-ranges — see [`classify_flag`] and
//! [`EventData::var_get`]/[`EventData::var_set`] for the exact branches, and
//! [`EventDataError`] for what happens off the end of each. Confusingly,
//! upstream numbers flags' [`SPECIAL_FLAGS_START`] and vars'
//! [`VARS_START`]/[`TEMP_VARS_START`] identically at `0x4000`; the overlap is
//! coincidental (the two id spaces are read by entirely different functions)
//! and never a collision in practice.
//!
//! Both spaces share one shape upstream leaves unchecked: `GetFlagPointer`/
//! `GetVarPointer` have no bounds check between the ordinary range and the
//! special range, or past the special range's end — an id there reads or
//! corrupts memory adjacent to the underlying fixed-size C arrays (undefined
//! behaviour, not deliberate design; no real `FLAG_*`/`VAR_*` constant ever
//! falls there). This port has no unsafe pointer arithmetic to reproduce that
//! with, so it refuses those ids as [`EventDataError::OutOfRange`] instead.

// ---------------------------------------------------------------------------
// Flag id space (`include/constants/flags.h`).
// ---------------------------------------------------------------------------

/// `TEMP_FLAGS_START`. Temporary flags are cleared every map load.
pub const TEMP_FLAGS_START: u16 = 0x0;
/// `TEMP_FLAGS_END` (`FLAG_TEMP_1F`).
pub const TEMP_FLAGS_END: u16 = 0x1F;
/// `NUM_TEMP_FLAGS`.
pub const NUM_TEMP_FLAGS: u16 = TEMP_FLAGS_END - TEMP_FLAGS_START + 1;

/// `DAILY_FLAGS_START`. Private: only used to derive [`FLAGS_COUNT`].
/// `ClearDailyFlags` itself (RTC-tied daily-flag clearing) is out of scope
/// for this slice.
const DAILY_FLAGS_START: u16 = 0x920;
/// `DAILY_FLAGS_END`.
const DAILY_FLAGS_END: u16 = 0x95F;

/// `FLAGS_COUNT` (`DAILY_FLAGS_END + 1`) — the number of ordinary flag ids,
/// `0..FLAGS_COUNT`.
pub const FLAGS_COUNT: u16 = DAILY_FLAGS_END + 1;

// Ground-truth check that `NUM_DAILY_FLAGS` (`DAILY_FLAGS_END -
// DAILY_FLAGS_START + 1`) matches upstream, keeping `DAILY_FLAGS_START` a
// meaningful part of the derivation rather than dead weight.
const _: () = assert!(DAILY_FLAGS_END - DAILY_FLAGS_START + 1 == 64);

/// `NUM_FLAG_BYTES` (`ROUND_BITS_TO_BYTES(FLAGS_COUNT)`) — the size of the
/// ordinary flag byte array.
const NUM_FLAG_BYTES: usize = (FLAGS_COUNT as usize).div_ceil(8);

/// `SPECIAL_FLAGS_START`.
pub const SPECIAL_FLAGS_START: u16 = 0x4000;
/// `SPECIAL_FLAGS_END` (`SPECIAL_FLAGS_START + 0x7F`).
pub const SPECIAL_FLAGS_END: u16 = SPECIAL_FLAGS_START + 0x7F;
/// `NUM_SPECIAL_FLAGS`.
pub const NUM_SPECIAL_FLAGS: u16 = SPECIAL_FLAGS_END - SPECIAL_FLAGS_START + 1;

/// `SPECIAL_FLAGS_SIZE` (`NUM_SPECIAL_FLAGS / 8`).
const SPECIAL_FLAGS_BYTES: usize = (NUM_SPECIAL_FLAGS as usize).div_ceil(8);

// Five named system flags `ClearTempFieldEventData` clears outside the temp
// range (`src/event_data.c`). All resolve well under `FLAGS_COUNT`, so
// they're ordinary flags; kept as private literals (with derivations
// documented) rather than pulling in the full `flags.h` constant table,
// which is out of scope for this slice.
/// `SYSTEM_FLAGS` (`TRAINER_FLAGS_END + 1`).
const SYSTEM_FLAGS: u16 = 0x860;
/// `FLAG_NURSE_UNION_ROOM_REMINDER` (`SYSTEM_FLAGS + 0x20`).
const FLAG_NURSE_UNION_ROOM_REMINDER: u16 = SYSTEM_FLAGS + 0x20;
/// `FLAG_SYS_USE_STRENGTH` (`SYSTEM_FLAGS + 0x29`).
const FLAG_SYS_USE_STRENGTH: u16 = SYSTEM_FLAGS + 0x29;
/// `FLAG_SYS_ENC_UP_ITEM` (`SYSTEM_FLAGS + 0x4D`).
const FLAG_SYS_ENC_UP_ITEM: u16 = SYSTEM_FLAGS + 0x4D;
/// `FLAG_SYS_ENC_DOWN_ITEM` (`SYSTEM_FLAGS + 0x4E`).
const FLAG_SYS_ENC_DOWN_ITEM: u16 = SYSTEM_FLAGS + 0x4E;
/// `FLAG_SYS_CTRL_OBJ_DELETE` (`SYSTEM_FLAGS + 0x61`).
const FLAG_SYS_CTRL_OBJ_DELETE: u16 = SYSTEM_FLAGS + 0x61;

// ---------------------------------------------------------------------------
// Var id space (`include/constants/vars.h`).
// ---------------------------------------------------------------------------

/// `VARS_START` — the offset subtracted from a var id to index the vars
/// array.
pub const VARS_START: u16 = 0x4000;
/// `VARS_END`.
pub const VARS_END: u16 = 0x40FF;
/// `VARS_COUNT` (`VARS_END - VARS_START + 1`).
pub const VARS_COUNT: usize = (VARS_END - VARS_START + 1) as usize;

/// `TEMP_VARS_START` (`== VARS_START`). Temporary vars are cleared every map
/// load.
pub const TEMP_VARS_START: u16 = VARS_START;
/// `TEMP_VARS_END`.
pub const TEMP_VARS_END: u16 = TEMP_VARS_START + 0xF;
/// `NUM_TEMP_VARS`.
pub const NUM_TEMP_VARS: u16 = TEMP_VARS_END - TEMP_VARS_START + 1;

/// `SPECIAL_VARS_START`. Out of scope for this slice — see the module docs.
pub const SPECIAL_VARS_START: u16 = 0x8000;
/// `SPECIAL_VARS_END` (`VAR_TRAINER_BATTLE_OPPONENT_A`).
pub const SPECIAL_VARS_END: u16 = 0x8015;

/// Errors surfaced by [`EventData`]'s flag and var accessors.
///
/// Upstream leaves the equivalent conditions either silently unreachable (no
/// `FLAG_*`/`VAR_*` constant is ever defined in the gap `OutOfRange`
/// describes) or dispatched to state this port doesn't have yet
/// (`SpecialVar`). Neither is a panic path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventDataError {
    /// `id` is in the special-var dispatch range
    /// (`SPECIAL_VARS_START..=SPECIAL_VARS_END`), which upstream serves via
    /// `gSpecialVars`, a table of pointers to standalone globals
    /// (`gSpecialVar_0x8000`, `gSpecialVar_LastTalked`, …). Those globals —
    /// and the subsystems that would own them — don't exist in this port
    /// yet; wiring them up is a later slice's job.
    SpecialVar(u16),
    /// `id` falls in a gap no real `FLAG_*`/`VAR_*` constant ever
    /// occupies, which upstream's `GetFlagPointer`/`GetVarPointer` leave
    /// completely unchecked (an id here reads or corrupts memory adjacent to
    /// the underlying C arrays — undefined behaviour, not a deliberate
    /// design). This port has no unsafe pointer arithmetic to reproduce
    /// that with, so it refuses the id instead.
    OutOfRange(u16),
}

impl std::fmt::Display for EventDataError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SpecialVar(id) => {
                write!(
                    f,
                    "var {id:#06x} is in the special-var range (not backed by this store yet)"
                )
            }
            Self::OutOfRange(id) => write!(f, "id {id:#06x} is out of range"),
        }
    }
}

impl std::error::Error for EventDataError {}

/// Where a flag id resolves to, mirroring upstream `GetFlagPointer`'s
/// branches.
enum FlagLocation {
    /// `id == 0`: deliberately tolerated dead space (see the module docs).
    Null,
    /// An ordinary flag: byte/bit offset into [`EventData::flags`].
    Ordinary { byte: usize, bit: u8 },
    /// A special flag: byte/bit offset into [`EventData::special_flags`].
    Special { byte: usize, bit: u8 },
}

/// Classify a flag id the way upstream's `GetFlagPointer` does, but with an
/// explicit bounds check standing in for its unchecked pointer arithmetic
/// (see the module docs).
// `idx % 8` is always `0..8`, so the `u8` casts below never truncate.
#[allow(clippy::cast_possible_truncation)]
fn classify_flag(id: u16) -> Result<FlagLocation, EventDataError> {
    if id == 0 {
        Ok(FlagLocation::Null)
    } else if id < FLAGS_COUNT {
        let idx = usize::from(id);
        Ok(FlagLocation::Ordinary {
            byte: idx / 8,
            bit: (idx % 8) as u8,
        })
    } else if (SPECIAL_FLAGS_START..=SPECIAL_FLAGS_END).contains(&id) {
        let idx = usize::from(id - SPECIAL_FLAGS_START);
        Ok(FlagLocation::Special {
            byte: idx / 8,
            bit: (idx % 8) as u8,
        })
    } else {
        Err(EventDataError::OutOfRange(id))
    }
}

/// The event-data store: Emerald's boolean flags and 16-bit vars.
///
/// Mirrors `gSaveBlock1Ptr->flags`/`vars` plus the static `sSpecialFlags`
/// array as plain owned fields — no global mutable state
/// `(oop-boundaries)`. A save system (out of scope here) would own an
/// `EventData` and (de)serialize `flags`/`vars`; `special_flags` mirrors
/// upstream's `sSpecialFlags`, which is session-only and was never part of
/// the save block to begin with.
#[derive(Debug, Clone)]
pub struct EventData {
    /// Ordinary flags, `0..FLAGS_COUNT`, packed 8 per byte. Upstream
    /// `gSaveBlock1Ptr->flags`.
    flags: [u8; NUM_FLAG_BYTES],
    /// Special flags, `SPECIAL_FLAGS_START..=SPECIAL_FLAGS_END`, packed 8 per
    /// byte. Upstream's static `sSpecialFlags`.
    special_flags: [u8; SPECIAL_FLAGS_BYTES],
    /// Ordinary vars, `VARS_START..=VARS_END`, indexed by `id - VARS_START`.
    /// Upstream `gSaveBlock1Ptr->vars`.
    vars: [u16; VARS_COUNT],
}

impl Default for EventData {
    fn default() -> Self {
        Self::new()
    }
}

impl EventData {
    /// A freshly initialized store: every flag clear, every var zero.
    /// Mirrors `InitEventData` (`memset` over `flags`, `vars`, and the
    /// static `sSpecialFlags`).
    #[must_use]
    pub const fn new() -> Self {
        Self {
            flags: [0; NUM_FLAG_BYTES],
            special_flags: [0; SPECIAL_FLAGS_BYTES],
            vars: [0; VARS_COUNT],
        }
    }

    /// Read a flag. Mirrors `FlagGet`.
    ///
    /// `id == 0` reports `false` (see the module docs); this is not an
    /// error.
    ///
    /// # Errors
    ///
    /// Returns [`EventDataError::OutOfRange`] if `id` falls in the
    /// unchecked-in-C gap described in the module docs.
    pub fn flag_get(&self, id: u16) -> Result<bool, EventDataError> {
        Ok(match classify_flag(id)? {
            FlagLocation::Null => false,
            FlagLocation::Ordinary { byte, bit } => (self.flags[byte] >> bit) & 1 != 0,
            FlagLocation::Special { byte, bit } => (self.special_flags[byte] >> bit) & 1 != 0,
        })
    }

    /// Set a flag. Mirrors `FlagSet`.
    ///
    /// `id == 0` is a no-op (see the module docs); this is not an error.
    ///
    /// # Errors
    ///
    /// Returns [`EventDataError::OutOfRange`] if `id` falls in the
    /// unchecked-in-C gap described in the module docs.
    pub fn flag_set(&mut self, id: u16) -> Result<(), EventDataError> {
        match classify_flag(id)? {
            FlagLocation::Null => {}
            FlagLocation::Ordinary { byte, bit } => self.flags[byte] |= 1 << bit,
            FlagLocation::Special { byte, bit } => self.special_flags[byte] |= 1 << bit,
        }
        Ok(())
    }

    /// Clear a flag. Mirrors `FlagClear`.
    ///
    /// `id == 0` is a no-op (see the module docs); this is not an error.
    ///
    /// # Errors
    ///
    /// Returns [`EventDataError::OutOfRange`] if `id` falls in the
    /// unchecked-in-C gap described in the module docs.
    pub fn flag_clear(&mut self, id: u16) -> Result<(), EventDataError> {
        match classify_flag(id)? {
            FlagLocation::Null => {}
            FlagLocation::Ordinary { byte, bit } => self.flags[byte] &= !(1 << bit),
            FlagLocation::Special { byte, bit } => self.special_flags[byte] &= !(1 << bit),
        }
        Ok(())
    }

    /// Clear an ordinary flag known (by the caller, at compile time) to be
    /// in `1..FLAGS_COUNT`. Used internally so
    /// [`clear_temp_field_event_data`](Self::clear_temp_field_event_data)
    /// doesn't have to thread a [`Result`] through calls that can never
    /// actually fail.
    fn clear_ordinary_flag(&mut self, id: u16) {
        debug_assert!(
            id != 0 && id < FLAGS_COUNT,
            "id {id} must be an ordinary flag id"
        );
        let idx = usize::from(id);
        self.flags[idx / 8] &= !(1 << (idx % 8));
    }

    /// Read a var. Mirrors `VarGet` (via `GetVarPointer`).
    ///
    /// `id < VARS_START` returns `id` itself unchanged — upstream's
    /// intentional immediate-passthrough (see the module docs); this is not
    /// an error.
    ///
    /// # Errors
    ///
    /// Returns [`EventDataError::SpecialVar`] if `id` is in the special-var
    /// dispatch range, or [`EventDataError::OutOfRange`] if `id` falls in
    /// the unchecked-in-C gap described in the module docs.
    pub fn var_get(&self, id: u16) -> Result<u16, EventDataError> {
        if id < VARS_START {
            Ok(id)
        } else if id <= VARS_END {
            Ok(self.vars[usize::from(id - VARS_START)])
        } else if (SPECIAL_VARS_START..=SPECIAL_VARS_END).contains(&id) {
            Err(EventDataError::SpecialVar(id))
        } else {
            Err(EventDataError::OutOfRange(id))
        }
    }

    /// Write a var. Mirrors `VarSet` (via `GetVarPointer`).
    ///
    /// `id < VARS_START` is a no-op — upstream's `VarSet` returns `FALSE`
    /// without writing anywhere (see the module docs); this is not an
    /// error.
    ///
    /// # Errors
    ///
    /// Returns [`EventDataError::SpecialVar`] if `id` is in the special-var
    /// dispatch range, or [`EventDataError::OutOfRange`] if `id` falls in
    /// the unchecked-in-C gap described in the module docs.
    pub fn var_set(&mut self, id: u16, value: u16) -> Result<(), EventDataError> {
        if id < VARS_START {
            Ok(())
        } else if id <= VARS_END {
            self.vars[usize::from(id - VARS_START)] = value;
            Ok(())
        } else if (SPECIAL_VARS_START..=SPECIAL_VARS_END).contains(&id) {
            Err(EventDataError::SpecialVar(id))
        } else {
            Err(EventDataError::OutOfRange(id))
        }
    }

    /// Clear per-map-load temporary state. Mirrors `ClearTempFieldEventData`:
    /// the temp flag range (`TEMP_FLAGS_START..=TEMP_FLAGS_END`), the temp
    /// var range (`TEMP_VARS_START..=TEMP_VARS_END`), and five named system
    /// flags upstream clears alongside them (`FLAG_SYS_ENC_UP_ITEM`,
    /// `FLAG_SYS_ENC_DOWN_ITEM`, `FLAG_SYS_USE_STRENGTH`,
    /// `FLAG_SYS_CTRL_OBJ_DELETE`, `FLAG_NURSE_UNION_ROOM_REMINDER`).
    /// Nothing outside these ranges is touched.
    pub fn clear_temp_field_event_data(&mut self) {
        // `TEMP_FLAGS_SIZE` bytes at `TEMP_FLAGS_START / 8` (== byte 0).
        let temp_flag_bytes = usize::from(NUM_TEMP_FLAGS) / 8;
        self.flags[..temp_flag_bytes].fill(0);

        // `NUM_TEMP_VARS` vars at `TEMP_VARS_START - VARS_START` (== index 0).
        let temp_var_count = usize::from(NUM_TEMP_VARS);
        self.vars[..temp_var_count].fill(0);

        self.clear_ordinary_flag(FLAG_SYS_ENC_UP_ITEM);
        self.clear_ordinary_flag(FLAG_SYS_ENC_DOWN_ITEM);
        self.clear_ordinary_flag(FLAG_SYS_USE_STRENGTH);
        self.clear_ordinary_flag(FLAG_SYS_CTRL_OBJ_DELETE);
        self.clear_ordinary_flag(FLAG_NURSE_UNION_ROOM_REMINDER);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Ground-truth values transcribed independently from
    // `include/constants/flags.h` / `include/constants/vars.h`, not derived
    // from this module's code, so they pin the id-space layout rather than
    // restate it.
    #[test]
    fn id_space_constants_match_upstream_headers() {
        assert_eq!(FLAGS_COUNT, 0x0960);
        assert_eq!(NUM_FLAG_BYTES, 300);
        // NUM_DAILY_FLAGS, the span FLAGS_COUNT is derived from.
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
        // Byte-boundary-crossing pair: id 7 is the top bit of byte 0, id 8 is
        // the bottom bit of byte 1.
        let mut data = EventData::new();
        data.flag_set(7).unwrap();
        assert_eq!(data.flag_get(7), Ok(true));
        assert_eq!(data.flag_get(8), Ok(false));
        assert_eq!(data.flag_get(6), Ok(false));

        data.flag_set(8).unwrap();
        assert_eq!(
            data.flag_get(7),
            Ok(true),
            "setting id 8 must not clear id 7"
        );
        assert_eq!(data.flag_get(8), Ok(true));

        data.flag_clear(7).unwrap();
        assert_eq!(data.flag_get(7), Ok(false));
        assert_eq!(
            data.flag_get(8),
            Ok(true),
            "clearing id 7 must not clear id 8"
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
        // Still false: id 0 is never actually stored anywhere.
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
        // A special flag and an ordinary flag never alias.
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
        // Distinct vars don't alias.
        assert_eq!(data.var_get(VARS_START), Ok(42));
    }

    #[test]
    fn var_id_below_vars_start_is_an_immediate_passthrough() {
        let mut data = EventData::new();
        assert_eq!(data.var_get(0), Ok(0));
        assert_eq!(data.var_get(0x1234), Ok(0x1234));
        // VarSet is a no-op here: the "value" argument is discarded, and the
        // id keeps reading back as itself.
        assert_eq!(data.var_set(0x1234, 0xFFFF), Ok(()));
        assert_eq!(data.var_get(0x1234), Ok(0x1234));
    }

    #[test]
    fn var_special_range_is_reported_as_not_stored_here() {
        let mut data = EventData::new();
        assert_eq!(
            data.var_get(SPECIAL_VARS_START),
            Err(EventDataError::SpecialVar(SPECIAL_VARS_START))
        );
        assert_eq!(
            data.var_get(SPECIAL_VARS_END),
            Err(EventDataError::SpecialVar(SPECIAL_VARS_END))
        );
        assert_eq!(
            data.var_set(SPECIAL_VARS_START, 1),
            Err(EventDataError::SpecialVar(SPECIAL_VARS_START))
        );
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

        // Temp flags/vars, at both ends of each range.
        data.flag_set(TEMP_FLAGS_START + 1).unwrap(); // id 0 is the tolerated null flag
        data.flag_set(TEMP_FLAGS_END).unwrap();
        data.var_set(TEMP_VARS_START, 7).unwrap();
        data.var_set(TEMP_VARS_END, 9).unwrap();

        // The five named system flags upstream clears alongside the temp
        // ranges.
        data.flag_set(FLAG_SYS_ENC_UP_ITEM).unwrap();
        data.flag_set(FLAG_SYS_ENC_DOWN_ITEM).unwrap();
        data.flag_set(FLAG_SYS_USE_STRENGTH).unwrap();
        data.flag_set(FLAG_SYS_CTRL_OBJ_DELETE).unwrap();
        data.flag_set(FLAG_NURSE_UNION_ROOM_REMINDER).unwrap();

        // Untouched state just outside every cleared range, to prove the
        // clear is selective rather than a global wipe.
        let outside_flag = TEMP_FLAGS_END + 1;
        let outside_var = TEMP_VARS_END + 1;
        data.flag_set(outside_flag).unwrap();
        data.var_set(outside_var, 123).unwrap();

        data.clear_temp_field_event_data();

        assert_eq!(data.flag_get(TEMP_FLAGS_START + 1), Ok(false));
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
    fn error_display_is_human_readable() {
        let special = EventDataError::SpecialVar(0x8000);
        let out_of_range = EventDataError::OutOfRange(0x1000);
        assert_eq!(
            special.to_string(),
            "var 0x8000 is in the special-var range (not backed by this store yet)"
        );
        assert_eq!(out_of_range.to_string(), "id 0x1000 is out of range");
    }
}
