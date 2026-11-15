//! Pack-free sprite motion, animation and OAM geometry for the chooser
//! (`pokeemerald/src/starter_choose.c:99-282`, `:638-667`).
//!
//! Upstream positions sprites by their centre; the native [`OamEntry`] takes
//! the footprint's top-left corner, so every builder here subtracts half the
//! footprint (`pokeemerald/src/sprite.c:339-355`).

use rendering::{AffineMatrix, AffineMode, BitDepth, OamEntry, ObjShape};

use super::StarterChoice;

/// Poké Ball centres, in [`StarterChoice::ALL`] order (`starter_choose.c:104-108`).
pub(super) const BALL_CENTERS: [(i32, i32); 3] = [(60, 64), (120, 88), (180, 64)];
/// Hand centres, in [`StarterChoice::ALL`] order (`starter_choose.c:204-209`).
const HAND_CENTERS: [(i32, i32); 3] = [(60, 32), (120, 56), (180, 32)];
/// Where the preview circle and Pokémon come to rest (`starter_choose.c:29-30`).
pub(super) const PREVIEW_TARGET: (i32, i32) = (120, 64);

const PREVIEW_STEP_X: i32 = 4;
const PREVIEW_STEP_Y: i32 = 2;

/// OAM coordinate fields wrap at their hardware widths: 9 bits of X, 8 of Y.
const OAM_X_SPACE: i32 = 512;
const OAM_Y_SPACE: i32 = 256;

const OBJ_SIZE_32X32: u8 = 2;
const OBJ_SIZE_64X64: u8 = 3;
const SMALL_SIDE: i32 = 32;
const LARGE_SIDE: i32 = 64;

/// OBJ priority of the hand, balls and circle (`starter_choose.c:153-202`).
const SHEET_OBJ_PRIORITY: u8 = 1;
const MON_OBJ_PRIORITY: u8 = 0;

/// The chooser's OBJ tile layout: each sheet reserves 64 tiles
/// (`starter_choose.c:284-315`), and the previewed Pokémon follows them.
pub(super) const BALL_SHEET_TILE: u16 = 0;
pub(super) const CIRCLE_SHEET_TILE: u16 = 64;
pub(super) const MON_SHEET_TILE: u16 = 128;
const HAND_TILE_OFFSET: u16 = 48;

/// Palette banks of the OBJ layout: ball sheet, circle sheet, then bank 14
/// for the Pokémon picture (`starter_choose.c:633`).
pub(super) const BALL_BANK: u8 = 0;
pub(super) const CIRCLE_BANK: u8 = 1;
pub(super) const MON_BANK: u8 = 14;

const MON_MATRIX: u8 = 0;
const CIRCLE_MATRIX: u8 = 1;

/// Affine scale steps, in 1/256ths (`starter_choose.c:267-282`).
const MON_SCALE_STEP: i32 = 16;
const CIRCLE_SCALE_STEP: i32 = 20;
/// Sprite callbacks that apply a scale step before the animation ends.
const SCALE_STEPS: u32 = 16;
const MATRIX_NUMERATOR: i32 = 1 << 16;

/// Sheet-frame tile offsets and durations of the selected ball's 128-frame
/// loop (`starter_choose.c:226-243`).
const SELECTED_BALL_FRAMES: [(u16, u32); 17] = [
    (16, 4),
    (0, 4),
    (32, 4),
    (0, 4),
    (16, 4),
    (0, 4),
    (32, 4),
    (0, 4),
    (0, 32),
    (16, 8),
    (0, 8),
    (32, 8),
    (0, 8),
    (16, 8),
    (0, 8),
    (32, 8),
    (0, 8),
];

/// `Sin(phase, 8)` at the 64 phases the hand visits, phase `0, 4, .. 252`
/// (`starter_choose.c:638-645`, `trig.c:514-517`).
const HAND_BOB: [i8; 64] = [
    0, 0, 1, 2, 3, 3, 4, 5, 5, 6, 6, 7, 7, 7, 7, 7, 8, 7, 7, 7, 7, 7, 6, 6, 5, 5, 4, 3, 3, 2, 1, 0,
    0, -1, -2, -3, -4, -4, -5, -6, -6, -7, -7, -8, -8, -8, -8, -8, -8, -8, -8, -8, -8, -8, -7, -7,
    -6, -6, -5, -4, -4, -3, -2, -1,
];

/// The hand's vertical float, advancing one phase per frame and never reset.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct HandBob {
    frame: usize,
    offset: i8,
}

impl HandBob {
    /// Runs one `SpriteCB_SelectionHand` call.
    pub(super) fn step(&mut self) {
        self.offset = HAND_BOB[self.frame % HAND_BOB.len()];
        self.frame += 1;
    }

    pub(super) const fn offset(self) -> i8 {
        self.offset
    }
}

/// One Poké Ball's tile animation: still, or the selected-ball loop.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct BallAnimation {
    selected: bool,
    age: u32,
    tile_offset: u16,
}

impl BallAnimation {
    /// Runs one `SpriteCB_Pokeball` call: restart only when the animation
    /// number changes (`StartSpriteAnimIfDifferent`), then advance a frame.
    pub(super) fn step(&mut self, selected: bool) {
        if selected != self.selected {
            self.selected = selected;
            self.age = 0;
        }
        self.tile_offset = if selected {
            selected_frame_offset(self.age)
        } else {
            0
        };
        self.age += 1;
    }

    pub(super) const fn tile_offset(self) -> u16 {
        self.tile_offset
    }
}

/// Tile offset the selected ball shows `age` frames after its animation began.
pub(super) fn selected_frame_offset(age: u32) -> u16 {
    let mut remaining = age % SELECTED_BALL_FRAMES.iter().map(|&(_, d)| d).sum::<u32>();
    for &(offset, duration) in &SELECTED_BALL_FRAMES {
        if remaining < duration {
            return offset;
        }
        remaining -= duration;
    }
    unreachable!("the remainder is smaller than the loop length")
}

/// The previewed Pokémon and its circle: they share one path and are created
/// on the same frame (`starter_choose.c:493-504`).
#[derive(Debug, Clone, Copy)]
pub(super) struct Preview {
    choice: StarterChoice,
    center: (i32, i32),
    callbacks_run: u32,
}

impl Preview {
    pub(super) const fn new(choice: StarterChoice) -> Self {
        Self {
            choice,
            center: BALL_CENTERS[choice.ordinal()],
            callbacks_run: 0,
        }
    }

    pub(super) const fn choice(self) -> StarterChoice {
        self.choice
    }

    #[cfg(test)]
    pub(super) const fn center(self) -> (i32, i32) {
        self.center
    }

    /// Runs one `SpriteCB_StarterPokemon` call and then the affine animation.
    pub(super) fn step(&mut self) {
        self.center.0 += (PREVIEW_TARGET.0 - self.center.0).clamp(-PREVIEW_STEP_X, PREVIEW_STEP_X);
        self.center.1 += (PREVIEW_TARGET.1 - self.center.1).clamp(-PREVIEW_STEP_Y, PREVIEW_STEP_Y);
        self.callbacks_run += 1;
    }

    /// Whether the circle's affine animation has set its end flag. The flag
    /// rises one callback after the last scale step runs.
    pub(super) const fn affine_ended(self) -> bool {
        self.callbacks_run > SCALE_STEPS
    }

    /// Whether the circle has stopped at the preview target.
    pub(super) const fn settled(self) -> bool {
        self.center.0 == PREVIEW_TARGET.0 && self.center.1 == PREVIEW_TARGET.1
    }

    /// The displayed scale (1/256ths) of a sprite whose steps are `step` wide.
    fn scale(self, step: i32) -> i32 {
        let applied = self.callbacks_run.clamp(1, SCALE_STEPS);
        step * i32::try_from(applied).unwrap_or(0)
    }

    pub(super) fn mon_scale(self) -> i32 {
        self.scale(MON_SCALE_STEP)
    }

    pub(super) fn circle_scale(self) -> i32 {
        self.scale(CIRCLE_SCALE_STEP)
    }
}

/// The OAM matrix that displays a sprite at `scale` (1/256ths): hardware
/// samples with the reciprocal (`sprite.c:1067-1121`).
fn inverse_scale_matrix(scale: i32) -> AffineMatrix {
    let reciprocal = i16::try_from(MATRIX_NUMERATOR / scale).unwrap_or(i16::MAX);
    AffineMatrix::new(reciprocal, 0, 0, reciprocal)
}

/// The two OAM matrices: the Pokémon's and the circle's.
pub(super) fn matrices(preview: Option<Preview>) -> [AffineMatrix; 2] {
    let mut matrices = [AffineMatrix::IDENTITY; 2];
    if let Some(preview) = preview {
        matrices[usize::from(MON_MATRIX)] = inverse_scale_matrix(preview.mon_scale());
        matrices[usize::from(CIRCLE_MATRIX)] = inverse_scale_matrix(preview.circle_scale());
    }
    matrices
}

/// The sprites' per-frame state the scene owns.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct Sprites {
    pub(super) hand: HandBob,
    pub(super) balls: [BallAnimation; 3],
    pub(super) preview: Option<Preview>,
}

impl Sprites {
    /// Runs every sprite callback and animation for one frame.
    pub(super) fn step(&mut self, selected: StarterChoice) {
        self.hand.step();
        for (ordinal, ball) in self.balls.iter_mut().enumerate() {
            ball.step(ordinal == selected.ordinal());
        }
        if let Some(preview) = &mut self.preview {
            preview.step();
        }
    }

    /// The OAM list in draw-priority order: Pokémon, circle, hand, balls.
    pub(super) fn oam_entries(&self, selected: StarterChoice) -> Vec<OamEntry> {
        let mut entries = Vec::with_capacity(6);
        if let Some(preview) = self.preview {
            entries.push(mon_entry(preview.center));
            entries.push(circle_entry(preview.center));
        }
        entries.push(hand_entry(selected, self.hand.offset()));
        for (ordinal, ball) in self.balls.iter().enumerate() {
            entries.push(ball_entry(ordinal, ball.tile_offset()));
        }
        entries
    }
}

fn sheet_entry(origin: (i32, i32), tile: u16, bank: u8, size: u8, priority: u8) -> OamEntry {
    OamEntry::new(
        // Both coordinates wrap into the OAM fields' own widths.
        u16::try_from(origin.0.rem_euclid(OAM_X_SPACE)).unwrap_or(0),
        u8::try_from(origin.1.rem_euclid(OAM_Y_SPACE)).unwrap_or(0),
        tile,
        bank,
        BitDepth::Bpp4,
        false,
        false,
        ObjShape::Square,
        size,
        priority,
        true,
    )
}

fn hand_entry(selected: StarterChoice, bob: i8) -> OamEntry {
    let (x, y) = HAND_CENTERS[selected.ordinal()];
    sheet_entry(
        (x - SMALL_SIDE / 2, y - SMALL_SIDE / 2 + i32::from(bob)),
        BALL_SHEET_TILE + HAND_TILE_OFFSET,
        BALL_BANK,
        OBJ_SIZE_32X32,
        SHEET_OBJ_PRIORITY,
    )
}

fn ball_entry(ordinal: usize, tile_offset: u16) -> OamEntry {
    let (x, y) = BALL_CENTERS[ordinal];
    sheet_entry(
        (x - SMALL_SIDE / 2, y - SMALL_SIDE / 2),
        BALL_SHEET_TILE + tile_offset,
        BALL_BANK,
        OBJ_SIZE_32X32,
        SHEET_OBJ_PRIORITY,
    )
}

fn mon_entry(center: (i32, i32)) -> OamEntry {
    sheet_entry(
        (center.0 - LARGE_SIDE / 2, center.1 - LARGE_SIDE / 2),
        MON_SHEET_TILE,
        MON_BANK,
        OBJ_SIZE_64X64,
        MON_OBJ_PRIORITY,
    )
    .with_affine(AffineMode::Affine {
        matrix_num: MON_MATRIX,
    })
}

/// The circle's OAM footprint is the double-size 128x128 box around its centre.
fn circle_entry(center: (i32, i32)) -> OamEntry {
    sheet_entry(
        (center.0 - LARGE_SIDE, center.1 - LARGE_SIDE),
        CIRCLE_SHEET_TILE,
        CIRCLE_BANK,
        OBJ_SIZE_64X64,
        SHEET_OBJ_PRIORITY,
    )
    .with_affine(AffineMode::AffineDoubleSize {
        matrix_num: CIRCLE_MATRIX,
    })
}
