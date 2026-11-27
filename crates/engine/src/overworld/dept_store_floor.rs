//! Default floor row of the department-store elevator menu (`GetDeptStoreDefaultFloorChoice`,
//! `pokeemerald/src/field_specials.c:1777-1810`). Upstream's two file-static scratch values are not modelled.

use crate::save::WarpData;

/// Map group of the Lilycove department store, rooftop, and elevator (`gMapGroup_IndoorLilycove`).
const DEPT_STORE_MAP_GROUP: i8 = 13;
/// Menu rows run from the 5th floor at 0 down to the 1st floor at 4.
const FLOOR_MAP_NUMS_BY_ROW: [i8; 5] = [20, 19, 18, 17, 16];
/// Row for any warp outside the five shopping floors.
const FALLBACK_ROW: u16 = 0;

/// The multichoice row to highlight for the saved dynamic warp.
pub fn default_floor_choice(dynamic_warp: &WarpData) -> u16 {
    if dynamic_warp.map_group != DEPT_STORE_MAP_GROUP {
        return FALLBACK_ROW;
    }
    FLOOR_MAP_NUMS_BY_ROW
        .iter()
        .position(|&map_num| map_num == dynamic_warp.map_num)
        .map_or(FALLBACK_ROW, |row| row as u16)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn warp(map_group: i8, map_num: i8) -> WarpData {
        WarpData {
            map_group,
            map_num,
            ..WarpData::default()
        }
    }

    #[test]
    fn each_floor_maps_to_its_menu_row() {
        assert_eq!(default_floor_choice(&warp(13, 20)), 0);
        assert_eq!(default_floor_choice(&warp(13, 19)), 1);
        assert_eq!(default_floor_choice(&warp(13, 18)), 2);
        assert_eq!(default_floor_choice(&warp(13, 17)), 3);
        assert_eq!(default_floor_choice(&warp(13, 16)), 4);
    }

    #[test]
    fn rooftop_and_other_same_group_maps_fall_back() {
        assert_eq!(default_floor_choice(&warp(13, 21)), 0);
        assert_eq!(default_floor_choice(&warp(13, 22)), 0);
        assert_eq!(default_floor_choice(&warp(13, 0)), 0);
    }

    #[test]
    fn a_floor_number_in_another_group_falls_back() {
        assert_eq!(default_floor_choice(&warp(12, 16)), 0);
        assert_eq!(default_floor_choice(&warp(14, 17)), 0);
    }

    #[test]
    fn warp_id_and_coordinates_do_not_change_the_row() {
        let moved = WarpData {
            map_group: 13,
            map_num: 18,
            warp_id: -1,
            x: -300,
            y: 700,
        };
        assert_eq!(default_floor_choice(&moved), 2);
        let moved_rooftop = WarpData {
            map_num: 21,
            ..moved
        };
        assert_eq!(default_floor_choice(&moved_rooftop), 0);
    }
}
