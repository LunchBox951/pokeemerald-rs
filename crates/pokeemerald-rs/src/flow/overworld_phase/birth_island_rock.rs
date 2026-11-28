//! The Birth Island rock puzzle's step counter (`IncrementBirthIslandRockStepCount`,
//! `pokeemerald/src/field_specials.c:3377-3387`), split out of [`super::step`].

use assets::MapId;

use super::OverworldPhase;

/// `VAR_DEOXYS_ROCK_STEP_COUNT` (`pokeemerald/include/constants/vars.h:72`).
const VAR_DEOXYS_ROCK_STEP_COUNT: u16 = 0x4034;
const BIRTH_ISLAND_EXTERIOR: MapId = MapId("MAP_BIRTH_ISLAND_EXTERIOR");
const LAST_COUNTED_STEP: u16 = 99;

impl OverworldPhase {
    /// `field_specials.c:3377-3387`: counts a `tookStep` landing on the Birth Island exterior.
    pub(super) fn increment_birth_island_rock_step_count(&mut self) {
        if self.map_id != BIRTH_ISLAND_EXTERIOR {
            return;
        }
        let event_data = &mut self.save1.event_data;
        let next = event_data
            .var_get(VAR_DEOXYS_ROCK_STEP_COUNT)
            .unwrap_or(0)
            .wrapping_add(1);
        let _ = event_data.var_set(
            VAR_DEOXYS_ROCK_STEP_COUNT,
            if next > LAST_COUNTED_STEP { 0 } else { next },
        );
    }
}
