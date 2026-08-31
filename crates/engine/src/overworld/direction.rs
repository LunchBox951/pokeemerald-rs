//! Cardinal directions used by overworld movement.

use assets::Direction as ConnectionDirection;

const SOUTH_DIR_ID: u8 = 1;
const NORTH_DIR_ID: u8 = 2;
const WEST_DIR_ID: u8 = 3;
const EAST_DIR_ID: u8 = 4;

/// A cardinal direction the player can face or move toward.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    South,
    North,
    West,
    East,
}

impl Direction {
    /// Returns the one-tile offset, with positive `y` pointing down the screen.
    #[must_use]
    pub const fn delta(self) -> (i32, i32) {
        match self {
            Self::South => (0, 1),
            Self::North => (0, -1),
            Self::West => (-1, 0),
            Self::East => (1, 0),
        }
    }

    /// Returns the map-connection direction with the same cardinal meaning.
    #[must_use]
    pub const fn to_connection_direction(self) -> ConnectionDirection {
        match self {
            Self::South => ConnectionDirection::South,
            Self::North => ConnectionDirection::North,
            Self::West => ConnectionDirection::West,
            Self::East => ConnectionDirection::East,
        }
    }

    /// Returns the direction's encoded save-file id.
    #[must_use]
    pub const fn to_dir_id(self) -> u8 {
        match self {
            Self::South => SOUTH_DIR_ID,
            Self::North => NORTH_DIR_ID,
            Self::West => WEST_DIR_ID,
            Self::East => EAST_DIR_ID,
        }
    }

    /// Decodes a cardinal save-file id.
    ///
    /// Returns `None` for the no-direction sentinel and diagonal directions.
    #[must_use]
    pub const fn from_dir_id(id: u8) -> Option<Self> {
        match id {
            SOUTH_DIR_ID => Some(Self::South),
            NORTH_DIR_ID => Some(Self::North),
            WEST_DIR_ID => Some(Self::West),
            EAST_DIR_ID => Some(Self::East),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delta_uses_screen_coordinates() {
        assert_eq!(Direction::South.delta(), (0, 1));
        assert_eq!(Direction::North.delta(), (0, -1));
        assert_eq!(Direction::West.delta(), (-1, 0));
        assert_eq!(Direction::East.delta(), (1, 0));
    }

    #[test]
    fn encoded_ids_round_trip_cardinal_directions() {
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
    fn enum_discriminants_remain_zero_based() {
        assert_eq!(Direction::South as u8, 0);
        assert_eq!(Direction::North as u8, 1);
        assert_eq!(Direction::West as u8, 2);
        assert_eq!(Direction::East as u8, 3);
    }

    #[test]
    fn connection_directions_preserve_cardinal_identity() {
        assert_eq!(Direction::South.to_connection_direction().id(), 1);
        assert_eq!(Direction::North.to_connection_direction().id(), 2);
        assert_eq!(Direction::West.to_connection_direction().id(), 3);
        assert_eq!(Direction::East.to_connection_direction().id(), 4);
    }
}
