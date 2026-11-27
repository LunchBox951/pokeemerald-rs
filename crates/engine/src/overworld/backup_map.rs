//! Geometry of the backup map layout around an active map and the connection coordinate rules
//! that fill it (`InitMapLayoutData`, `InitBackupMapLayoutConnections`, `fieldmap.c`).

use assets::{Direction, MetatileCell};

/// Backup-layout margin past the west, north and south edges (`MAP_OFFSET`, `fieldmap.h:18`).
pub const MAP_OFFSET: i32 = 7;

/// Westmost and northmost backed coordinate.
pub const BACKUP_MAP_MIN: i32 = -MAP_OFFSET;

/// Backed columns past the east edge: the odd `MAP_OFFSET_W` reaches one past the margin.
pub const BACKUP_MAP_MAX_PAST_EAST: i32 = MAP_OFFSET;

/// Backed rows past the south edge, one fewer than east because `MAP_OFFSET_H` is even.
pub const BACKUP_MAP_MAX_PAST_SOUTH: i32 = MAP_OFFSET - 1;

/// Returns whether `(x, y)` lies in the backup layout of a `width` by `height` map, interior included.
#[must_use]
pub fn within_backup_band(width: u16, height: u16, x: i64, y: i64) -> bool {
    (i64::from(BACKUP_MAP_MIN)..=i64::from(width) + i64::from(BACKUP_MAP_MAX_PAST_EAST))
        .contains(&x)
        && (i64::from(BACKUP_MAP_MIN)..=i64::from(height) + i64::from(BACKUP_MAP_MAX_PAST_SOUTH))
            .contains(&y)
}

/// Maps an active-map `position` to the cell of a `direction` connection that backs it.
///
/// Returns `None` when the connection's side does not contain `position`; the target's bounds are
/// the caller's to check, since a translated coordinate may be negative.
#[must_use]
pub fn connection_coordinates(
    direction: Direction,
    offset: i32,
    active: (u16, u16),
    target: (u16, u16),
    position: (i64, i64),
) -> Option<(i64, i64)> {
    let (x, y) = position;
    let offset = i64::from(offset);
    let (width, height) = (i64::from(active.0), i64::from(active.1));
    match direction {
        Direction::South if y >= height => Some((x - offset, y - height)),
        Direction::North if y < 0 => Some((x - offset, i64::from(target.1) + y)),
        Direction::West if x < 0 => Some((i64::from(target.0) + x, y - offset)),
        Direction::East if x >= width => Some((x - width, y - offset)),
        _ => None,
    }
}

/// Picks the last connection whose `resolve` yields a cell: a later declaration overwrites an
/// earlier one where they overlap.
pub fn last_connection_cell<C>(
    connections: &[C],
    resolve: impl FnMut(&C) -> Option<MetatileCell>,
) -> Option<MetatileCell> {
    connections.iter().filter_map(resolve).next_back()
}
