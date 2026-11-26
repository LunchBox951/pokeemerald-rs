//! Rusturf Tunnel story-state transition.

use assets::MapId;

use crate::event_data::EventData;

const FLAG_RUSTURF_TUNNEL_OPENED: u16 = 0xC7;
const FLAG_HIDE_RUSTURF_TUNNEL_ROCK_1: u16 = 0x3A3;
const FLAG_HIDE_RUSTURF_TUNNEL_ROCK_2: u16 = 0x3A4;
const VAR_RUSTURF_TUNNEL_STATE: u16 = 0x409A;

const RUSTURF_TUNNEL: MapId = MapId("MAP_RUSTURF_TUNNEL");
const STATE_ROCK_1_HIDDEN: u16 = 4;
const STATE_ROCK_2_HIDDEN: u16 = 5;

/// Advances `VAR_RUSTURF_TUNNEL_STATE` for a player standing in `map`
/// (`TryUpdateRusturfTunnelState`, `src/field_specials.c:1398-1416`).
///
/// Only while the tunnel is still closed and `map` is Rusturf Tunnel does
/// either hidden-rock flag count: the first writes state 4 and takes
/// precedence over the second, which writes state 5. Returns whether a state
/// was written; otherwise nothing is changed. The caller owns the follow-up
/// script that reacts to the result.
pub fn try_update_rusturf_tunnel_state(map: MapId, data: &mut EventData) -> bool {
    if data.flag_get(FLAG_RUSTURF_TUNNEL_OPENED) != Ok(false) || map != RUSTURF_TUNNEL {
        return false;
    }
    let state = if data.flag_get(FLAG_HIDE_RUSTURF_TUNNEL_ROCK_1) == Ok(true) {
        STATE_ROCK_1_HIDDEN
    } else if data.flag_get(FLAG_HIDE_RUSTURF_TUNNEL_ROCK_2) == Ok(true) {
        STATE_ROCK_2_HIDDEN
    } else {
        return false;
    };
    data.var_set(VAR_RUSTURF_TUNNEL_STATE, state).is_ok()
}

#[cfg(test)]
mod tests {
    use assets::MapHeaderTable;

    use super::*;
    use crate::event_data::{VARS_COUNT, VARS_START};

    const STATE_INDEX: usize = (VAR_RUSTURF_TUNNEL_STATE - VARS_START) as usize;

    fn data_with(flags: &[u16], state: u16) -> EventData {
        let mut data = EventData::new();
        for &flag in flags {
            data.flag_set(flag).unwrap();
        }
        data.var_set(VAR_RUSTURF_TUNNEL_STATE, state).unwrap();
        data
    }

    fn unchanged(map: MapId, flags: &[u16]) {
        let mut data = data_with(flags, 0x1234);
        let before = data.clone();
        assert!(!try_update_rusturf_tunnel_state(map, &mut data));
        assert_eq!(data.flag_bytes(), before.flag_bytes());
        assert_eq!(data.vars_raw(), before.vars_raw());
    }

    #[test]
    fn upstream_ids_are_pinned() {
        assert_eq!(FLAG_RUSTURF_TUNNEL_OPENED, 199);
        assert_eq!(FLAG_HIDE_RUSTURF_TUNNEL_ROCK_1, 931);
        assert_eq!(FLAG_HIDE_RUSTURF_TUNNEL_ROCK_2, 932);
        assert_eq!(VAR_RUSTURF_TUNNEL_STATE, 16538);
        let header = MapHeaderTable::new().header(RUSTURF_TUNNEL).unwrap();
        assert_eq!((header.group, header.num), (24, 4));
    }

    #[test]
    fn first_rock_writes_four() {
        let mut data = data_with(&[FLAG_HIDE_RUSTURF_TUNNEL_ROCK_1], 0);
        assert!(try_update_rusturf_tunnel_state(RUSTURF_TUNNEL, &mut data));
        assert_eq!(data.var_get(VAR_RUSTURF_TUNNEL_STATE), Ok(4));
    }

    #[test]
    fn second_rock_writes_five() {
        let mut data = data_with(&[FLAG_HIDE_RUSTURF_TUNNEL_ROCK_2], 0);
        assert!(try_update_rusturf_tunnel_state(RUSTURF_TUNNEL, &mut data));
        assert_eq!(data.var_get(VAR_RUSTURF_TUNNEL_STATE), Ok(5));
    }

    #[test]
    fn first_rock_takes_precedence_when_both_are_set() {
        let mut data = data_with(
            &[
                FLAG_HIDE_RUSTURF_TUNNEL_ROCK_1,
                FLAG_HIDE_RUSTURF_TUNNEL_ROCK_2,
            ],
            5,
        );
        assert!(try_update_rusturf_tunnel_state(RUSTURF_TUNNEL, &mut data));
        assert_eq!(data.var_get(VAR_RUSTURF_TUNNEL_STATE), Ok(4));
    }

    #[test]
    fn overwrites_a_prior_state_even_when_it_is_already_the_target() {
        let mut data = data_with(&[FLAG_HIDE_RUSTURF_TUNNEL_ROCK_2], 4);
        assert!(try_update_rusturf_tunnel_state(RUSTURF_TUNNEL, &mut data));
        assert_eq!(data.var_get(VAR_RUSTURF_TUNNEL_STATE), Ok(5));
        let mut data = data_with(&[FLAG_HIDE_RUSTURF_TUNNEL_ROCK_1], 4);
        assert!(try_update_rusturf_tunnel_state(RUSTURF_TUNNEL, &mut data));
        assert_eq!(data.var_get(VAR_RUSTURF_TUNNEL_STATE), Ok(4));
    }

    #[test]
    fn already_open_tunnel_changes_nothing() {
        for rocks in [
            &[FLAG_HIDE_RUSTURF_TUNNEL_ROCK_1][..],
            &[FLAG_HIDE_RUSTURF_TUNNEL_ROCK_2],
            &[
                FLAG_HIDE_RUSTURF_TUNNEL_ROCK_1,
                FLAG_HIDE_RUSTURF_TUNNEL_ROCK_2,
            ],
        ] {
            let mut flags = vec![FLAG_RUSTURF_TUNNEL_OPENED];
            flags.extend_from_slice(rocks);
            unchanged(RUSTURF_TUNNEL, &flags);
        }
    }

    #[test]
    fn neither_rock_changes_nothing() {
        unchanged(RUSTURF_TUNNEL, &[]);
    }

    #[test]
    fn wrong_map_group_changes_nothing() {
        let table = MapHeaderTable::new();
        let other = table
            .get_by_position(0, 4)
            .expect("a map shares Rusturf Tunnel's number in another group");
        assert_ne!(other.group, 24);
        assert_eq!(other.num, 4);
        unchanged(other.id, &[FLAG_HIDE_RUSTURF_TUNNEL_ROCK_1]);
        unchanged(other.id, &[FLAG_HIDE_RUSTURF_TUNNEL_ROCK_2]);
    }

    #[test]
    fn wrong_map_number_changes_nothing() {
        let table = MapHeaderTable::new();
        let other = table
            .get_by_position(24, 5)
            .expect("a map shares Rusturf Tunnel's group with another number");
        assert_eq!(other.group, 24);
        assert_ne!(other.num, 4);
        unchanged(other.id, &[FLAG_HIDE_RUSTURF_TUNNEL_ROCK_1]);
        unchanged(other.id, &[FLAG_HIDE_RUSTURF_TUNNEL_ROCK_2]);
    }

    #[test]
    fn writes_only_its_variable_and_survives_a_save_round_trip() {
        let mut data = data_with(&[FLAG_HIDE_RUSTURF_TUNNEL_ROCK_2, 100, 0x3A5], 0);
        data.var_set(VAR_RUSTURF_TUNNEL_STATE - 1, 0x1234).unwrap();
        data.var_set(VAR_RUSTURF_TUNNEL_STATE + 1, 0x5678).unwrap();
        let before = data.clone();

        assert!(try_update_rusturf_tunnel_state(RUSTURF_TUNNEL, &mut data));

        let restored = EventData::from_saved_state(*data.flag_bytes(), *data.vars_raw());
        assert_eq!(restored.var_get(VAR_RUSTURF_TUNNEL_STATE), Ok(5));
        assert_eq!(restored.flag_bytes(), before.flag_bytes());
        for index in 0..VARS_COUNT {
            if index != STATE_INDEX {
                assert_eq!(restored.vars_raw()[index], before.vars_raw()[index]);
            }
        }
        assert_eq!(restored.var_get(VAR_RUSTURF_TUNNEL_STATE - 1), Ok(0x1234));
        assert_eq!(restored.var_get(VAR_RUSTURF_TUNNEL_STATE + 1), Ok(0x5678));
    }
}
