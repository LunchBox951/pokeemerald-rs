//! Metatile behavior ids and predicates used by overworld systems.
//!
//! Constants retain their encoded byte values because extracted metatile
//! attributes store behavior identities directly.

/// Ordinary ground with no behavior-specific handling.
pub const MB_NORMAL: u8 = 0;

/// Tall grass that permits land encounters.
pub const MB_TALL_GRASS: u8 = 0x02;

/// Long grass that permits land encounters.
pub const MB_LONG_GRASS: u8 = 0x03;

/// An unused-named behavior that nevertheless permits land encounters.
pub const MB_UNUSED_05: u8 = 0x05;

/// Deep sand that permits land encounters.
pub const MB_DEEP_SAND: u8 = 0x06;

/// Cave floor that permits land encounters.
pub const MB_CAVE: u8 = 0x08;

/// Indoor floor that permits land encounters.
pub const MB_INDOOR_ENCOUNTER: u8 = 0x0B;

/// Ash-covered grass that permits land encounters.
pub const MB_ASHGRASS: u8 = 0x24;

/// Footprint-recording ground that permits land encounters.
pub const MB_FOOTPRINTS: u8 = 0x25;

/// A directional wall along a tile's east edge.
pub const MB_IMPASSABLE_EAST: u8 = 0x30;

/// A directional wall along a tile's west edge.
pub const MB_IMPASSABLE_WEST: u8 = 0x31;

/// A directional wall along a tile's north edge.
pub const MB_IMPASSABLE_NORTH: u8 = 0x32;

/// A directional wall along a tile's south edge.
pub const MB_IMPASSABLE_SOUTH: u8 = 0x33;

/// Directional walls along a tile's north and east edges.
pub const MB_IMPASSABLE_NORTHEAST: u8 = 0x34;

/// Directional walls along a tile's north and west edges.
pub const MB_IMPASSABLE_NORTHWEST: u8 = 0x35;

/// Directional walls along a tile's south and east edges.
pub const MB_IMPASSABLE_SOUTHEAST: u8 = 0x36;

/// Directional walls along a tile's south and west edges.
pub const MB_IMPASSABLE_SOUTHWEST: u8 = 0x37;

/// A non-animated-door behavior supported as a door-shaped warp trigger.
pub const MB_NON_ANIMATED_DOOR: u8 = 0x60;

/// An animated-door behavior supported as a door-shaped warp trigger.
pub const MB_ANIMATED_DOOR: u8 = 0x69;

/// A north-facing arrow-warp alias used by the Abandoned Ship stairs.
pub const MB_STAIRS_OUTSIDE_ABANDONED_SHIP: u8 = 0x1B;

/// A south-facing arrow-warp alias used by the Shoal Cave entrance.
pub const MB_SHOAL_CAVE_ENTRANCE: u8 = 0x1C;

/// An east-facing arrow warp.
pub const MB_EAST_ARROW_WARP: u8 = 0x62;

/// A west-facing arrow warp.
pub const MB_WEST_ARROW_WARP: u8 = 0x63;

/// A north-facing arrow warp.
pub const MB_NORTH_ARROW_WARP: u8 = 0x64;

/// A south-facing arrow warp.
pub const MB_SOUTH_ARROW_WARP: u8 = 0x65;

/// A non-animated-door alias used only to determine arrival facing.
pub const MB_WATER_DOOR: u8 = 0x6C;

/// A south-facing arrow-warp alias used on water.
pub const MB_WATER_SOUTH_ARROW_WARP: u8 = 0x6D;

/// A south-warp identity that takes precedence over general door arrival facing.
pub const MB_DEEP_SOUTH_WARP: u8 = 0x6E;

/// A door alias used only to determine arrival facing.
pub const MB_PETALBURG_GYM_DOOR: u8 = 0x8D;

/// A breakable secret-base door that blocks its east and west edges.
pub const MB_SECRET_BASE_BREAKABLE_DOOR: u8 = 0xBE;

/// Directional walls along a tile's south and north edges.
pub const MB_IMPASSABLE_SOUTH_AND_NORTH: u8 = 0xC0;

/// Directional walls along a tile's west and east edges.
pub const MB_IMPASSABLE_WEST_AND_EAST: u8 = 0xC1;

/// Returns whether a tile has a directional wall along its east edge.
#[must_use]
pub const fn is_east_blocked(behavior: u8) -> bool {
    matches!(
        behavior,
        MB_IMPASSABLE_EAST
            | MB_IMPASSABLE_NORTHEAST
            | MB_IMPASSABLE_SOUTHEAST
            | MB_IMPASSABLE_WEST_AND_EAST
            | MB_SECRET_BASE_BREAKABLE_DOOR
    )
}

/// Returns whether a tile has a directional wall along its west edge.
#[must_use]
pub const fn is_west_blocked(behavior: u8) -> bool {
    matches!(
        behavior,
        MB_IMPASSABLE_WEST
            | MB_IMPASSABLE_NORTHWEST
            | MB_IMPASSABLE_SOUTHWEST
            | MB_IMPASSABLE_WEST_AND_EAST
            | MB_SECRET_BASE_BREAKABLE_DOOR
    )
}

/// Returns whether a tile has a directional wall along its north edge.
#[must_use]
pub const fn is_north_blocked(behavior: u8) -> bool {
    matches!(
        behavior,
        MB_IMPASSABLE_NORTH
            | MB_IMPASSABLE_NORTHEAST
            | MB_IMPASSABLE_NORTHWEST
            | MB_IMPASSABLE_SOUTH_AND_NORTH
    )
}

/// Returns whether a tile has a directional wall along its south edge.
#[must_use]
pub const fn is_south_blocked(behavior: u8) -> bool {
    matches!(
        behavior,
        MB_IMPASSABLE_SOUTH
            | MB_IMPASSABLE_SOUTHEAST
            | MB_IMPASSABLE_SOUTHWEST
            | MB_IMPASSABLE_SOUTH_AND_NORTH
    )
}

/// Returns whether a behavior is one of the supported door-shaped warp triggers.
///
/// Arrival-facing door aliases are not triggers.
#[must_use]
pub const fn is_door(behavior: u8) -> bool {
    matches!(behavior, MB_NON_ANIMATED_DOOR | MB_ANIMATED_DOOR)
}

/// Returns whether a behavior unconditionally triggers a supported door-shaped warp.
///
/// Unknown behaviors and direction-gated arrow warps fail closed.
#[must_use]
pub const fn is_warp_trigger(behavior: u8) -> bool {
    is_door(behavior)
}

/// Returns whether a behavior permits land encounters.
///
/// This contains the complete non-surfable encounter set; surfable encounter
/// behaviors return `false`.
#[must_use]
pub const fn is_land_wild_encounter(behavior: u8) -> bool {
    matches!(
        behavior,
        MB_TALL_GRASS
            | MB_LONG_GRASS
            | MB_UNUSED_05
            | MB_DEEP_SAND
            | MB_CAVE
            | MB_INDOOR_ENCOUNTER
            | MB_ASHGRASS
            | MB_FOOTPRINTS
    )
}

/// Returns whether a behavior is an east-facing arrow warp.
#[must_use]
pub const fn is_east_arrow_warp(behavior: u8) -> bool {
    behavior == MB_EAST_ARROW_WARP
}

/// Returns whether a behavior is a west-facing arrow warp.
#[must_use]
pub const fn is_west_arrow_warp(behavior: u8) -> bool {
    behavior == MB_WEST_ARROW_WARP
}

/// Returns whether a behavior is a north-facing arrow warp or its stairs alias.
#[must_use]
pub const fn is_north_arrow_warp(behavior: u8) -> bool {
    matches!(
        behavior,
        MB_NORTH_ARROW_WARP | MB_STAIRS_OUTSIDE_ABANDONED_SHIP
    )
}

/// Returns whether a behavior is a south-facing arrow warp or one of its aliases.
#[must_use]
pub const fn is_south_arrow_warp(behavior: u8) -> bool {
    matches!(
        behavior,
        MB_SOUTH_ARROW_WARP | MB_WATER_SOUTH_ARROW_WARP | MB_SHOAL_CAVE_ENTRANCE
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const MB_SHORT_GRASS: u8 = 0x07;
    const MB_LONG_GRASS_SOUTH_EDGE: u8 = 0x09;
    const MB_POND_WATER: u8 = 0x10;
    const MB_DEEP_WATER: u8 = 0x12;
    const MB_LADDER: u8 = 0x61;
    const MB_UP_ESCALATOR: u8 = 0x6A;

    #[test]
    fn supported_door_ids_are_recognized() {
        assert_eq!(MB_NON_ANIMATED_DOOR, 0x60);
        assert_eq!(MB_ANIMATED_DOOR, 0x69);
        assert!(is_door(MB_NON_ANIMATED_DOOR));
        assert!(is_door(MB_ANIMATED_DOOR));
        assert!(!is_door(MB_SOUTH_ARROW_WARP));
    }

    #[test]
    fn normal_ground_is_not_a_door() {
        assert!(!is_door(MB_NORMAL));
    }

    #[test]
    fn unported_warp_behaviors_fail_closed() {
        assert!(!is_warp_trigger(MB_LADDER));
        assert!(!is_warp_trigger(MB_UP_ESCALATOR));
    }

    #[test]
    fn door_behaviors_are_warp_triggers() {
        assert!(is_warp_trigger(MB_NON_ANIMATED_DOOR));
        assert!(is_warp_trigger(MB_ANIMATED_DOOR));
    }

    #[test]
    fn arrow_warp_and_arrival_facing_only_ids_are_not_door_shaped_warp_triggers() {
        for behavior in [
            MB_STAIRS_OUTSIDE_ABANDONED_SHIP,
            MB_SHOAL_CAVE_ENTRANCE,
            MB_EAST_ARROW_WARP,
            MB_WEST_ARROW_WARP,
            MB_NORTH_ARROW_WARP,
            MB_SOUTH_ARROW_WARP,
            MB_WATER_DOOR,
            MB_WATER_SOUTH_ARROW_WARP,
            MB_DEEP_SOUTH_WARP,
            MB_PETALBURG_GYM_DOOR,
        ] {
            assert!(
                !is_warp_trigger(behavior),
                "{behavior:#04x} must stay outside is_warp_trigger's door-shaped set"
            );
        }
    }

    #[test]
    fn door_alias_ids_match_no_arrow_warp_predicate() {
        for behavior in [MB_WATER_DOOR, MB_DEEP_SOUTH_WARP, MB_PETALBURG_GYM_DOOR] {
            assert!(!is_north_arrow_warp(behavior));
            assert!(!is_south_arrow_warp(behavior));
            assert!(!is_west_arrow_warp(behavior));
            assert!(!is_east_arrow_warp(behavior));
        }
    }

    #[test]
    fn arrow_warp_predicates_match_only_their_own_direction() {
        assert!(is_east_arrow_warp(MB_EAST_ARROW_WARP));
        assert!(is_west_arrow_warp(MB_WEST_ARROW_WARP));
        assert!(is_north_arrow_warp(MB_NORTH_ARROW_WARP));
        assert!(is_north_arrow_warp(MB_STAIRS_OUTSIDE_ABANDONED_SHIP));
        assert!(is_south_arrow_warp(MB_SOUTH_ARROW_WARP));
        assert!(is_south_arrow_warp(MB_WATER_SOUTH_ARROW_WARP));
        assert!(is_south_arrow_warp(MB_SHOAL_CAVE_ENTRANCE));

        assert!(!is_west_arrow_warp(MB_EAST_ARROW_WARP));
        assert!(!is_north_arrow_warp(MB_EAST_ARROW_WARP));
        assert!(!is_south_arrow_warp(MB_EAST_ARROW_WARP));
        assert!(!is_east_arrow_warp(MB_WEST_ARROW_WARP));
        assert!(!is_south_arrow_warp(MB_NORTH_ARROW_WARP));
        assert!(!is_north_arrow_warp(MB_SOUTH_ARROW_WARP));

        assert!(!is_east_arrow_warp(MB_NORMAL));
        assert!(!is_north_arrow_warp(MB_NORMAL));
        assert!(!is_south_arrow_warp(MB_NORMAL));
        assert!(!is_west_arrow_warp(MB_NORMAL));
    }

    #[test]
    fn arrival_facing_ids_have_expected_encoded_values() {
        assert_eq!(MB_STAIRS_OUTSIDE_ABANDONED_SHIP, 0x1B);
        assert_eq!(MB_SHOAL_CAVE_ENTRANCE, 0x1C);
        assert_eq!(MB_EAST_ARROW_WARP, 0x62);
        assert_eq!(MB_WEST_ARROW_WARP, 0x63);
        assert_eq!(MB_NORTH_ARROW_WARP, 0x64);
        assert_eq!(MB_SOUTH_ARROW_WARP, 0x65);
        assert_eq!(MB_WATER_DOOR, 0x6C);
        assert_eq!(MB_WATER_SOUTH_ARROW_WARP, 0x6D);
        assert_eq!(MB_DEEP_SOUTH_WARP, 0x6E);
        assert_eq!(MB_PETALBURG_GYM_DOOR, 0x8D);
    }

    #[test]
    fn land_encounter_ids_have_expected_encoded_values() {
        assert_eq!(MB_TALL_GRASS, 0x02);
        assert_eq!(MB_LONG_GRASS, 0x03);
        assert_eq!(MB_UNUSED_05, 0x05);
        assert_eq!(MB_DEEP_SAND, 0x06);
        assert_eq!(MB_CAVE, 0x08);
        assert_eq!(MB_INDOOR_ENCOUNTER, 0x0B);
        assert_eq!(MB_ASHGRASS, 0x24);
        assert_eq!(MB_FOOTPRINTS, 0x25);
    }

    #[test]
    fn land_encounter_predicate_matches_complete_non_surfable_set() {
        let expected = [
            MB_TALL_GRASS,
            MB_LONG_GRASS,
            MB_UNUSED_05,
            MB_DEEP_SAND,
            MB_CAVE,
            MB_INDOOR_ENCOUNTER,
            MB_ASHGRASS,
            MB_FOOTPRINTS,
        ];
        let matching: Vec<u8> = (0..=u8::MAX)
            .filter(|b| is_land_wild_encounter(*b))
            .collect();
        assert_eq!(matching, expected);
    }

    #[test]
    fn directional_wall_ids_have_expected_encoded_values() {
        assert_eq!(MB_IMPASSABLE_EAST, 0x30);
        assert_eq!(MB_IMPASSABLE_WEST, 0x31);
        assert_eq!(MB_IMPASSABLE_NORTH, 0x32);
        assert_eq!(MB_IMPASSABLE_SOUTH, 0x33);
        assert_eq!(MB_IMPASSABLE_NORTHEAST, 0x34);
        assert_eq!(MB_IMPASSABLE_NORTHWEST, 0x35);
        assert_eq!(MB_IMPASSABLE_SOUTHEAST, 0x36);
        assert_eq!(MB_IMPASSABLE_SOUTHWEST, 0x37);
        assert_eq!(MB_SECRET_BASE_BREAKABLE_DOOR, 0xBE);
        assert_eq!(MB_IMPASSABLE_SOUTH_AND_NORTH, 0xC0);
        assert_eq!(MB_IMPASSABLE_WEST_AND_EAST, 0xC1);
    }

    #[test]
    fn blocked_predicates_match_complete_directional_wall_sets() {
        let matching = |p: fn(u8) -> bool| (0..=u8::MAX).filter(|b| p(*b)).collect::<Vec<u8>>();

        assert_eq!(
            matching(is_east_blocked),
            vec![
                MB_IMPASSABLE_EAST,
                MB_IMPASSABLE_NORTHEAST,
                MB_IMPASSABLE_SOUTHEAST,
                MB_SECRET_BASE_BREAKABLE_DOOR,
                MB_IMPASSABLE_WEST_AND_EAST,
            ]
        );
        assert_eq!(
            matching(is_west_blocked),
            vec![
                MB_IMPASSABLE_WEST,
                MB_IMPASSABLE_NORTHWEST,
                MB_IMPASSABLE_SOUTHWEST,
                MB_SECRET_BASE_BREAKABLE_DOOR,
                MB_IMPASSABLE_WEST_AND_EAST,
            ]
        );
        assert_eq!(
            matching(is_north_blocked),
            vec![
                MB_IMPASSABLE_NORTH,
                MB_IMPASSABLE_NORTHEAST,
                MB_IMPASSABLE_NORTHWEST,
                MB_IMPASSABLE_SOUTH_AND_NORTH,
            ]
        );
        assert_eq!(
            matching(is_south_blocked),
            vec![
                MB_IMPASSABLE_SOUTH,
                MB_IMPASSABLE_SOUTHEAST,
                MB_IMPASSABLE_SOUTHWEST,
                MB_IMPASSABLE_SOUTH_AND_NORTH,
            ]
        );
    }

    #[test]
    fn north_south_wall_leaves_east_and_west_open() {
        assert!(is_north_blocked(MB_IMPASSABLE_SOUTH_AND_NORTH));
        assert!(is_south_blocked(MB_IMPASSABLE_SOUTH_AND_NORTH));
        assert!(!is_east_blocked(MB_IMPASSABLE_SOUTH_AND_NORTH));
        assert!(!is_west_blocked(MB_IMPASSABLE_SOUTH_AND_NORTH));
    }

    #[test]
    fn ordinary_ground_blocks_no_edge() {
        for behavior in [MB_NORMAL, MB_TALL_GRASS, MB_NON_ANIMATED_DOOR] {
            assert!(!is_north_blocked(behavior));
            assert!(!is_south_blocked(behavior));
            assert!(!is_east_blocked(behavior));
            assert!(!is_west_blocked(behavior));
        }
    }

    #[test]
    fn decorative_grass_and_surfable_water_are_not_land_encounters() {
        assert!(!is_land_wild_encounter(MB_NORMAL));
        assert!(!is_land_wild_encounter(MB_SHORT_GRASS));
        assert!(!is_land_wild_encounter(MB_LONG_GRASS_SOUTH_EDGE));
        assert!(!is_land_wild_encounter(MB_POND_WATER));
        assert!(!is_land_wild_encounter(MB_DEEP_WATER));
    }
}
