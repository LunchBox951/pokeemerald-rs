//! The Poké Ball's roots: the one `BALL_POKE` record.

use crate::reader::GbaPtr;
use crate::roots::{ImageRoot, PaletteRoot};

/// The Poké Ball's art: the rows `BALL_POKE` selects in `gBallSpriteSheets` and
/// `gBallSpritePalettes`, and the opened-ball sheet beside them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PokeBallRoot {
    /// The 16x48 sheet `gBallSpriteSheets[BALL_POKE]` points at.
    pub sheet: ImageRoot,
    /// The palette `gBallSpritePalettes[BALL_POKE]` points at.
    pub palette: PaletteRoot,
    /// `gOpenPokeballGfx`, which no table row names.
    pub open_sheet: ImageRoot,
}

/// The Poké Ball's roots (`src/pokeball.c`).
///
/// The two tables are recorded so a reader can corroborate the `BALL_POKE`
/// row by pointer, size and tag rather than trust the addresses alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PokeBallRoots {
    /// `gBallSpriteSheets`: 8-byte `{data, size, tag}` rows.
    pub sheet_table: GbaPtr,
    /// `gBallSpritePalettes`: 8-byte `{data, tag}` rows.
    pub palette_table: GbaPtr,
    /// The record, or `None` for a profile that carries no Poké Ball.
    pub poke_ball: Option<PokeBallRoot>,
}

impl PokeBallRoots {
    /// No Poké Ball roots recorded: the domain writes nothing.
    pub const NONE: Self = Self {
        sheet_table: GbaPtr::AT_BASE,
        palette_table: GbaPtr::AT_BASE,
        poke_ball: None,
    };
}
