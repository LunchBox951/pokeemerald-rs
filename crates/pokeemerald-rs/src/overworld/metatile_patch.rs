//! Runtime edits to a scene's metatile grid.
//!
//! Upstream keeps the map grid in RAM (`gBackupMapLayout`), so a field effect
//! can rewrite a cell and have both the collision lookups and the next map
//! redraw read it (`MapGridSetMetatileIdAt`, `pokeemerald/src/fieldmap.c:357-364`).
//! The scene owns the same grid bytes; these accessors are the one place that
//! reads and writes a packed cell in them.

use super::OverworldScene;

/// Bytes per packed grid cell.
const CELL_BYTES: usize = 2;

impl OverworldScene {
    /// The packed grid value at `(x, y)` in this map's own coordinates, or
    /// `None` outside the grid.
    pub(crate) fn grid_cell_raw(&self, x: i32, y: i32) -> Option<u16> {
        let offset = self.grid_offset(x, y)?;
        let bytes = self.grid_bytes.get(offset..offset + CELL_BYTES)?;
        Some(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    /// Overwrites the packed grid value at `(x, y)`; a position outside the
    /// grid changes nothing, as `MapGridSetMetatileIdAt` bounds-checks.
    pub(crate) fn set_grid_cell_raw(&mut self, x: i32, y: i32, raw: u16) {
        let Some(offset) = self.grid_offset(x, y) else {
            return;
        };
        if let Some(bytes) = self.grid_bytes.get_mut(offset..offset + CELL_BYTES) {
            bytes.copy_from_slice(&raw.to_le_bytes());
        }
    }

    fn grid_offset(&self, x: i32, y: i32) -> Option<usize> {
        let column = usize::try_from(x)
            .ok()
            .filter(|&c| c < usize::from(self.layout.width))?;
        let row = usize::try_from(y)
            .ok()
            .filter(|&r| r < usize::from(self.layout.height))?;
        Some((row * usize::from(self.layout.width) + column) * CELL_BYTES)
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::synthetic_scene;

    #[test]
    fn a_cell_reads_back_what_was_written_and_nothing_else_changes() {
        let mut scene = synthetic_scene(4, 3);
        let before: Vec<Option<u16>> = (0..3)
            .flat_map(|y| (0..4).map(move |x| (x, y)))
            .map(|(x, y)| scene.grid_cell_raw(x, y))
            .collect();
        scene.set_grid_cell_raw(2, 1, 0x4289);
        let after: Vec<Option<u16>> = (0..3)
            .flat_map(|y| (0..4).map(move |x| (x, y)))
            .map(|(x, y)| scene.grid_cell_raw(x, y))
            .collect();
        for (index, (old, new)) in before.iter().zip(&after).enumerate() {
            if index == 6 {
                assert_eq!(*new, Some(0x4289));
            } else {
                assert_eq!(old, new, "cell {index}");
            }
        }
    }

    #[test]
    fn positions_outside_the_grid_read_none_and_write_nothing() {
        let mut scene = synthetic_scene(4, 3);
        for (x, y) in [(-1, 0), (0, -1), (4, 0), (0, 3)] {
            assert_eq!(scene.grid_cell_raw(x, y), None, "({x}, {y})");
            scene.set_grid_cell_raw(x, y, 0xFFFF);
        }
        assert_eq!(
            scene.grid_cell_raw(3, 2).map(|raw| raw != 0xFFFF),
            Some(true)
        );
    }
}
