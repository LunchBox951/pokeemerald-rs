//! The Island Cave Regice perimeter puzzle (`ShouldDoBrailleRegicePuzzle`,
//! `src/braille_puzzles.c:283-343`).
//!
//! After the player reads the braille message the puzzle is started; each step
//! onto one of the 36 perimeter tiles of [`PATH`] sets that tile's bit across
//! three variables, a step anywhere else fails the attempt, and the puzzle is
//! solved by returning to `(8, 21)` with every bit set.
//!
//! [`should_do_braille_regice_puzzle`] is the one state transition. It takes the
//! player's map and tile and mutates only event data; it draws no RNG. Its
//! caller (`src/field_control_avatar.c:565-568`), the braille message, the script
//! that opens the entrance and all presentation are not ported.

use assets::MapId;

use super::player::TilePos;
use crate::event_data::{
    EventData, EventDataError, FLAG_SYS_BRAILLE_REGICE_COMPLETED, FLAG_TEMP_REGICE_PUZZLE_FAILED,
    FLAG_TEMP_REGICE_PUZZLE_STARTED, VAR_REGICE_STEPS_1, VAR_REGICE_STEPS_2, VAR_REGICE_STEPS_3,
};

/// The only map the puzzle runs on (`MAP_ISLAND_CAVE`).
const ISLAND_CAVE: MapId = MapId("MAP_ISLAND_CAVE");

/// Perimeter tiles in path order (`sRegicePathCoords`, `src/braille_puzzles.c:17-55`); a tile's
/// index is its bit number across the three step variables.
pub const PATH: [TilePos; 36] = [
    (4, 21),
    (5, 21),
    (6, 21),
    (7, 21),
    (8, 21),
    (9, 21),
    (10, 21),
    (11, 21),
    (12, 21),
    (12, 22),
    (12, 23),
    (13, 23),
    (13, 24),
    (13, 25),
    (13, 26),
    (13, 27),
    (12, 27),
    (12, 28),
    (4, 29),
    (5, 29),
    (6, 29),
    (7, 29),
    (8, 29),
    (9, 29),
    (10, 29),
    (11, 29),
    (12, 29),
    (4, 28),
    (4, 27),
    (3, 27),
    (3, 26),
    (3, 25),
    (3, 24),
    (3, 23),
    (4, 23),
    (4, 22),
];

/// The tile the player returns to, once every bit is set, to complete the puzzle.
const FINISH: TilePos = (8, 21);
/// Bits of [`VAR_REGICE_STEPS_3`] a full lap sets (36 - 32 = 4).
const STEPS_3_FULL: u16 = 0xF;

/// Runs one step of the puzzle for a player standing on `pos` of `map`, returning whether the
/// puzzle is now complete.
///
/// Returns `false` and changes nothing on any other map, once
/// `FLAG_SYS_BRAILLE_REGICE_COMPLETED` is set, before the puzzle is started or after it failed.
/// Otherwise a perimeter tile sets its bit (repeat visits change nothing) and the result is `true`
/// only when all 36 bits are set exactly and the tile is `(8, 21)`; any other tile sets the failed
/// flag and clears the started flag.
///
/// # Errors
///
/// Returns [`EventDataError`] if an event flag or variable id is unsupported; the ids used are
/// fixed, so this does not occur in practice.
pub fn should_do_braille_regice_puzzle(
    map: MapId,
    pos: TilePos,
    event_data: &mut EventData,
) -> Result<bool, EventDataError> {
    if map != ISLAND_CAVE
        || event_data.flag_get(FLAG_SYS_BRAILLE_REGICE_COMPLETED)?
        || !event_data.flag_get(FLAG_TEMP_REGICE_PUZZLE_STARTED)?
        || event_data.flag_get(FLAG_TEMP_REGICE_PUZZLE_FAILED)?
    {
        return Ok(false);
    }

    let Some(index) = PATH.iter().position(|&tile| tile == pos) else {
        event_data.flag_set(FLAG_TEMP_REGICE_PUZZLE_FAILED)?;
        event_data.flag_clear(FLAG_TEMP_REGICE_PUZZLE_STARTED)?;
        return Ok(false);
    };

    let (var, bit) = match index {
        0..=15 => (VAR_REGICE_STEPS_1, index),
        16..=31 => (VAR_REGICE_STEPS_2, index - 16),
        _ => (VAR_REGICE_STEPS_3, index - 32),
    };
    let steps = event_data.var_get(var)?;
    event_data.var_set(var, steps | (1 << bit))?;

    let lap_complete = event_data.var_get(VAR_REGICE_STEPS_1)? == 0xFFFF
        && event_data.var_get(VAR_REGICE_STEPS_2)? == 0xFFFF
        && event_data.var_get(VAR_REGICE_STEPS_3)? == STEPS_3_FULL;
    Ok(lap_complete && pos == FINISH)
}

#[cfg(test)]
mod tests;
