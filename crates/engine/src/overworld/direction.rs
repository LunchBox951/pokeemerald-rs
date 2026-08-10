//! The four cardinal walking directions (S-5).
//!
//! Mirrors the `DIR_NONE`/`DIR_SOUTH`/`DIR_NORTH`/`DIR_WEST`/`DIR_EAST`
//! subset of upstream `include/constants/global.h`'s `DIR_*` enum that
//! ordinary on-foot movement uses. Upstream's `DIR_*` also has four
//! bike-only diagonals (`DIR_SOUTHWEST`..`DIR_NORTHEAST`, 5-8) and a
//! `DIR_NONE` (0) sentinel for "no input"; this v1 slice only ports ordinary
//! walking (no bike, no running), so [`Direction`] models the four
//! cardinals only, and "no input" is expressed as `Option<Direction>` at the
//! call site ([`crate::overworld::player::PlayerState::step`]) rather than a
//! fifth variant here.

use assets::Direction as ConnectionDirection;

/// One of the four cardinal directions the player avatar can face or step
/// toward.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    /// `DIR_SOUTH` (1): facing/moving toward increasing `y`.
    South,
    /// `DIR_NORTH` (2): facing/moving toward decreasing `y`.
    North,
    /// `DIR_WEST` (3): facing/moving toward decreasing `x`.
    West,
    /// `DIR_EAST` (4): facing/moving toward increasing `x`.
    East,
}

impl Direction {
    /// The `(dx, dy)` tile offset one step in this direction applies,
    /// mirroring upstream `MoveCoords`'s use of `sDirectionToVectors`
    /// (`event_object_movement.c`): south is `+y`, north is `-y`, west is
    /// `-x`, east is `+x` (screen-space, top-down convention — `y` grows
    /// downward).
    #[must_use]
    pub const fn delta(self) -> (i32, i32) {
        match self {
            Self::South => (0, 1),
            Self::North => (0, -1),
            Self::West => (-1, 0),
            Self::East => (1, 0),
        }
    }

    /// The upstream `CONNECTION_*` counterpart of this direction
    /// (`include/constants/global.h`): `CONNECTION_SOUTH`/`_NORTH`/`_WEST`/
    /// `_EAST` share the same numeric values as `DIR_SOUTH`/`_NORTH`/`_WEST`/
    /// `_EAST` (1-4), a coincidence this port relies on only for the
    /// `MapConnection::direction` comparison in
    /// [`crate::overworld::map_runtime::MapRuntime::resolve_connection`], not
    /// for identity between the two enums in general (`assets::Direction`
    /// also carries `Dive`/`Emerge`, which have no walking-direction
    /// meaning).
    #[must_use]
    pub const fn to_connection_direction(self) -> ConnectionDirection {
        match self {
            Self::South => ConnectionDirection::South,
            Self::North => ConnectionDirection::North,
            Self::West => ConnectionDirection::West,
            Self::East => ConnectionDirection::East,
        }
    }

    /// This direction's raw upstream `DIR_*` id
    /// (`include/constants/global.h`): south 1, north 2, west 3, east 4.
    ///
    /// The one place a walking direction has to become a plain byte is the
    /// save file — `struct ObjectEvent`'s `facingDirection:4` nibble
    /// ([`crate::save::SavedObjectEvent`]) — so the mapping lives here,
    /// beside the enum it belongs to, rather than being re-derived at the
    /// serialization boundary.
    #[must_use]
    pub const fn to_dir_id(self) -> u8 {
        match self {
            Self::South => 1,
            Self::North => 2,
            Self::West => 3,
            Self::East => 4,
        }
    }

    /// The direction a raw `DIR_*` id names, or `None` for one this port
    /// does not walk in: `DIR_NONE` (0, upstream's "no input" sentinel —
    /// also what a zeroed, never-written save entry holds) and the four
    /// bike-only diagonals `DIR_SOUTHWEST`..`DIR_NORTHEAST` (5-8), which
    /// this v1 slice does not model (module docs).
    #[must_use]
    pub const fn from_dir_id(id: u8) -> Option<Self> {
        match id {
            1 => Some(Self::South),
            2 => Some(Self::North),
            3 => Some(Self::West),
            4 => Some(Self::East),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delta_matches_sdirectiontovectors() {
        // pokeemerald/src/event_object_movement.c `sDirectionToVectors`:
        // south {0,1}, north {0,-1}, west {-1,0}, east {1,0}.
        assert_eq!(Direction::South.delta(), (0, 1));
        assert_eq!(Direction::North.delta(), (0, -1));
        assert_eq!(Direction::West.delta(), (-1, 0));
        assert_eq!(Direction::East.delta(), (1, 0));
    }

    /// `include/constants/global.h`'s `DIR_*` numbering, and the round trip
    /// the save file's `facingDirection:4` nibble depends on. `DIR_NONE`
    /// (0) and the bike diagonals (5-8) have no [`Direction`], so a save
    /// holding one must decode as `None` rather than as some cardinal.
    #[test]
    fn dir_ids_match_upstream_numbering_and_round_trip() {
        for (direction, id) in [
            (Direction::South, 1),
            (Direction::North, 2),
            (Direction::West, 3),
            (Direction::East, 4),
        ] {
            assert_eq!(direction.to_dir_id(), id);
            assert_eq!(Direction::from_dir_id(id), Some(direction));
        }
        assert_eq!(Direction::from_dir_id(0), None, "DIR_NONE is not walkable");
        for diagonal in 5..=8 {
            assert_eq!(
                Direction::from_dir_id(diagonal),
                None,
                "the bike diagonals are not modelled"
            );
        }
    }

    #[test]
    fn connection_direction_matches_upstream_numbering() {
        assert_eq!(Direction::South.to_connection_direction().id(), 1);
        assert_eq!(Direction::North.to_connection_direction().id(), 2);
        assert_eq!(Direction::West.to_connection_direction().id(), 3);
        assert_eq!(Direction::East.to_connection_direction().id(), 4);
    }
}
