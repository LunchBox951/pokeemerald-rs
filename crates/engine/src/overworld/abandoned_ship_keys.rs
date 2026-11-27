//! The Abandoned Ship room-key checks (`FoundAbandonedShipRoom1Key`, `2Key`, `4Key`, `6Key`,
//! `src/field_specials.c:1328-1370`), called by `data/maps/AbandonedShip_HiddenFloorRooms/scripts.inc:47-83`.
//!
//! The check only; the sparkle, the scripts that call it and the key pickup are out of scope.

use crate::event_data::{EventData, EventDataError, SPECIAL_VARS_START};

/// `gSpecialVar_0x8004`, which every check overwrites with its flag id.
const VAR_0X8004: u16 = SPECIAL_VARS_START + 4;

/// A hidden-floor room that holds a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AbandonedShipRoom {
    /// `FoundAbandonedShipRoom1Key`.
    Room1,
    /// `FoundAbandonedShipRoom2Key`.
    Room2,
    /// `FoundAbandonedShipRoom4Key`.
    Room4,
    /// `FoundAbandonedShipRoom6Key`.
    Room6,
}

impl AbandonedShipRoom {
    const fn key_flag_name(self) -> &'static str {
        match self {
            Self::Room1 => "FLAG_HIDDEN_ITEM_ABANDONED_SHIP_RM_1_KEY",
            Self::Room2 => "FLAG_HIDDEN_ITEM_ABANDONED_SHIP_RM_2_KEY",
            Self::Room4 => "FLAG_HIDDEN_ITEM_ABANDONED_SHIP_RM_4_KEY",
            Self::Room6 => "FLAG_HIDDEN_ITEM_ABANDONED_SHIP_RM_6_KEY",
        }
    }

    /// The room's key flag id.
    ///
    /// # Panics
    ///
    /// Never in practice: the hidden-item flag table lists all four names, and a test pins that.
    #[must_use]
    pub fn key_flag(self) -> u16 {
        assets::hidden_item_flags::resolve(self.key_flag_name())
            .expect("the hidden-item flag table lists every room key")
    }
}

/// Reports whether the player has already found `room`'s key.
///
/// Stores the room's flag id in `VAR_0x8004` before reading the flag, on both answers.
/// Nothing else changes.
///
/// # Errors
///
/// Returns [`EventDataError::OutOfRange`] if event data rejects the flag or variable id.
pub fn found_abandoned_ship_room_key(
    event_data: &mut EventData,
    room: AbandonedShipRoom,
) -> Result<bool, EventDataError> {
    let flag = room.key_flag();
    event_data.var_set(VAR_0X8004, flag)?;
    event_data.flag_get(flag)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_data::{SPECIAL_VARS_END, VARS_END, VARS_START};

    /// Room, then its flag id (`FLAG_HIDDEN_ITEMS_START + 0x1F..=0x22`, `include/constants/flags.h:577-580`).
    const ROOMS: [(AbandonedShipRoom, u16); 4] = [
        (AbandonedShipRoom::Room1, 0x213),
        (AbandonedShipRoom::Room2, 0x214),
        (AbandonedShipRoom::Room4, 0x215),
        (AbandonedShipRoom::Room6, 0x216),
    ];

    fn special_vars(data: &EventData) -> Vec<u16> {
        (SPECIAL_VARS_START..=SPECIAL_VARS_END)
            .map(|id| data.var_get(id).unwrap())
            .collect()
    }

    fn ordinary_vars(data: &EventData) -> Vec<u16> {
        (VARS_START..=VARS_END)
            .map(|id| data.var_get(id).unwrap())
            .collect()
    }

    #[test]
    fn each_room_reads_its_own_flag() {
        for (room, flag) in ROOMS {
            assert_eq!(room.key_flag(), flag);
        }
    }

    #[test]
    fn a_clear_flag_answers_false_and_stores_the_flag_id() {
        for (room, flag) in ROOMS {
            let mut data = EventData::new();
            assert_eq!(found_abandoned_ship_room_key(&mut data, room), Ok(false));
            assert_eq!(data.var_get(VAR_0X8004), Ok(flag));
        }
    }

    #[test]
    fn a_set_flag_answers_true_and_stores_the_flag_id() {
        for (room, flag) in ROOMS {
            let mut data = EventData::new();
            data.flag_set(flag).unwrap();
            assert_eq!(found_abandoned_ship_room_key(&mut data, room), Ok(true));
            assert_eq!(data.var_get(VAR_0X8004), Ok(flag));
        }
    }

    #[test]
    fn only_the_rooms_own_flag_answers_true() {
        for (room, own) in ROOMS {
            let mut data = EventData::new();
            for (_, other) in ROOMS.into_iter().filter(|&(_, id)| id != own) {
                data.flag_set(other).unwrap();
            }
            assert_eq!(found_abandoned_ship_room_key(&mut data, room), Ok(false));
        }
    }

    #[test]
    fn the_stored_id_replaces_a_previous_value() {
        for (room, flag) in ROOMS {
            for found in [false, true] {
                let mut data = EventData::new();
                data.var_set(VAR_0X8004, 0xBEEF).unwrap();
                if found {
                    data.flag_set(flag).unwrap();
                }
                found_abandoned_ship_room_key(&mut data, room).unwrap();
                assert_eq!(data.var_get(VAR_0X8004), Ok(flag));
            }
        }
    }

    #[test]
    fn nothing_but_var_0x8004_changes() {
        for (room, flag) in ROOMS {
            for found in [false, true] {
                let mut data = EventData::new();
                if found {
                    data.flag_set(flag).unwrap();
                }
                for (_, other) in ROOMS.into_iter().filter(|&(_, id)| id != flag) {
                    data.flag_set(other).unwrap();
                }
                data.flag_set(0x1).unwrap();
                data.var_set(VARS_START, 0x1234).unwrap();
                data.var_set(VARS_END, 0x5678).unwrap();
                data.var_set(SPECIAL_VARS_START, 0x9ABC).unwrap();
                data.var_set(SPECIAL_VARS_END, 0xDEF0).unwrap();

                let flags_before = *data.flag_bytes();
                let ordinary_before = ordinary_vars(&data);
                let mut special_before = special_vars(&data);

                found_abandoned_ship_room_key(&mut data, room).unwrap();

                assert_eq!(*data.flag_bytes(), flags_before);
                assert_eq!(ordinary_vars(&data), ordinary_before);
                special_before[usize::from(VAR_0X8004 - SPECIAL_VARS_START)] = flag;
                assert_eq!(special_vars(&data), special_before);
            }
        }
    }
}
