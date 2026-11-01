//! A synthetic pack and input driver for the chooser's tests.

use assets::AssetPack;
use pack_format::PackEntry;
use platform::{ButtonState, Buttons};
use rendering::{Bgr555, Rgb888};

use crate::pack_test_support::{
    image_entry, pack_bytes, palette_entry_with_colors, write_scratch_exclusive,
};
use crate::starter_chooser::state::{Chooser, FrameResult};
use crate::starter_chooser::StarterChooserScene;

pub(super) const OPAQUE: u8 = 1;
const BANK_COLORS: u16 = 16;

pub(super) const BG_COLOR: Bgr555 = Bgr555::from_channels(4, 8, 12);
pub(super) const GRASS_COLOR: Bgr555 = Bgr555::from_channels(2, 20, 4);
/// Ball sheet frames 0, 1, 2 and the hand, one colour each.
pub(super) const SHEET_COLORS: [Bgr555; 4] = [
    Bgr555::from_channels(31, 0, 0),
    Bgr555::from_channels(0, 31, 0),
    Bgr555::from_channels(0, 0, 31),
    Bgr555::from_channels(31, 31, 0),
];
pub(super) const CIRCLE_COLOR: Bgr555 = Bgr555::from_channels(31, 31, 31);
pub(super) const MON_COLORS: [Bgr555; 3] = [
    Bgr555::from_channels(10, 31, 10),
    Bgr555::from_channels(31, 20, 5),
    Bgr555::from_channels(5, 20, 31),
];
pub(super) const BORDER_COLOR: Bgr555 = Bgr555::from_channels(0, 31, 31);
pub(super) const MESSAGE_FILL: Bgr555 = Bgr555::from_channels(30, 30, 30);
pub(super) const MESSAGE_FOREGROUND: Bgr555 = Bgr555::from_channels(3, 3, 3);
pub(super) const MESSAGE_SHADOW: Bgr555 = Bgr555::from_channels(20, 20, 20);

const TILES_SIDE: u32 = 128;
const BAG_ROWS: usize = 20;
const MAP_COLUMNS: usize = 32;
const GRASS_ROWS: usize = 32;
/// The tiles sheet's tile 1 is fully transparent so the grass behind the bag
/// shows through; every other tile is opaque.
const TRANSPARENT_TILE: u16 = 1;
/// The transparent tile sits in the bottom row's last visible column (x 232..240).
pub(super) const TRANSPARENT_COLUMN: usize = 29;
const BANK_ONE_ENTRY: u16 = 0x1000;

fn indexed(id: &'static str, width: u32, height: u32, pixels: Vec<u8>) -> PackEntry {
    pack_format::image_entry(id.into(), width, height, 4, pixels).unwrap()
}

fn raw_entries(id: &'static str, entries: &[u16]) -> PackEntry {
    let bytes = entries
        .iter()
        .flat_map(|entry| entry.to_le_bytes())
        .collect();
    pack_format::raw_entry(id.into(), bytes)
}

fn tiles_image() -> PackEntry {
    let mut pixels = vec![OPAQUE; (TILES_SIDE * TILES_SIDE) as usize];
    for y in 0..8 {
        for x in 8..16 {
            pixels[y * TILES_SIDE as usize + x] = 0;
        }
    }
    indexed(
        "starter-chooser/image/tiles",
        TILES_SIDE,
        TILES_SIDE,
        pixels,
    )
}

/// Ball sheet frame `f` (rows `32f..32f+32`) is filled with index `f + 1`.
fn pokeball_image() -> PackEntry {
    let pixels = (0..32 * 128)
        .map(|index| u8::try_from(index / (32 * 32)).unwrap() + 1)
        .collect();
    indexed("starter-chooser/image/pokeball_selection", 32, 128, pixels)
}

fn bag_entries() -> Vec<u16> {
    let mut entries = vec![0; MAP_COLUMNS * BAG_ROWS];
    entries[0] = BANK_ONE_ENTRY;
    entries[(BAG_ROWS - 1) * MAP_COLUMNS + TRANSPARENT_COLUMN] = TRANSPARENT_TILE;
    entries
}

fn starter_entries() -> Vec<PackEntry> {
    let mut bank_colors = [Bgr555::default(); 32];
    bank_colors[1] = BG_COLOR;
    bank_colors[17] = GRASS_COLOR;
    let tiles_palette: Vec<(u8, Bgr555)> = bank_colors
        .iter()
        .enumerate()
        .map(|(index, &color)| (u8::try_from(index).unwrap(), color))
        .collect();
    let sheet_colors: Vec<(u8, Bgr555)> = SHEET_COLORS
        .iter()
        .enumerate()
        .map(|(index, &color)| (u8::try_from(index + 1).unwrap(), color))
        .collect();
    let mut entries = vec![
        tiles_image(),
        palette_entry_with_colors("starter-chooser/palette/tiles", 32, &tiles_palette),
        pokeball_image(),
        palette_entry_with_colors(
            "starter-chooser/palette/pokeball_selection",
            BANK_COLORS,
            &sheet_colors,
        ),
        image_entry("starter-chooser/image/starter_circle", 64, 64, 4, OPAQUE),
        palette_entry_with_colors(
            "starter-chooser/palette/starter_circle",
            BANK_COLORS,
            &[(1, CIRCLE_COLOR)],
        ),
        raw_entries("starter-chooser/raw/birch_bag", &bag_entries()),
        raw_entries(
            "starter-chooser/raw/birch_grass",
            &vec![BANK_ONE_ENTRY; MAP_COLUMNS * GRASS_ROWS],
        ),
    ];
    for (name, color) in ["treecko", "torchic", "mudkip"].into_iter().zip(MON_COLORS) {
        let (image, palette) = mon_ids(name);
        entries.push(image_entry(image, 64, 64, 4, OPAQUE));
        entries.push(palette_entry_with_colors(
            palette,
            BANK_COLORS,
            &[(1, color)],
        ));
    }
    entries
}

fn mon_ids(name: &str) -> (&'static str, &'static str) {
    match name {
        "treecko" => ("pokemon/treecko/front", "pokemon/treecko/palette/normal"),
        "torchic" => ("pokemon/torchic/front", "pokemon/torchic/palette/normal"),
        _ => ("pokemon/mudkip/front", "pokemon/mudkip/palette/normal"),
    }
}

fn text_entries() -> Vec<PackEntry> {
    let message_colors = [
        (1, MESSAGE_FILL),
        (2, MESSAGE_FOREGROUND),
        (3, MESSAGE_SHADOW),
    ];
    let mut entries = vec![
        image_entry("text-window/image/1", 24, 24, 4, OPAQUE),
        palette_entry_with_colors("text-window/palette/1", BANK_COLORS, &[(1, BORDER_COLOR)]),
        image_entry("text-window/image/message_box", 56, 16, 4, 0),
        palette_entry_with_colors(
            "text-window/palette/message_box",
            BANK_COLORS,
            &message_colors,
        ),
    ];
    for font in ["normal", "narrow"] {
        let id: &'static str = Box::leak(format!("font/{font}/glyphs").into_boxed_str());
        entries.push(image_entry(
            id,
            assets::fonts::SHEET_WIDTH,
            assets::fonts::SHEET_HEIGHT,
            2,
            OPAQUE,
        ));
    }
    entries
}

pub(super) fn pack_entries() -> Vec<PackEntry> {
    let mut entries = starter_entries();
    entries.extend(text_entries());
    entries
}

/// Loads a pack built from `entries`, removing its scratch file once read.
pub(super) fn load_pack(label: &str, entries: Vec<PackEntry>) -> AssetPack {
    let path = std::env::temp_dir().join(format!(
        "pokeemerald-rs-starter-chooser-{label}-{}-{:?}.pack",
        std::process::id(),
        std::thread::current().id()
    ));
    write_scratch_exclusive(&path, pack_bytes(entries));
    let pack = AssetPack::load(&path);
    let _ = std::fs::remove_file(&path);
    pack.expect("the synthetic pack is well-formed")
}

pub(super) fn synthetic_scene(label: &str) -> StarterChooserScene {
    let pack = load_pack(label, pack_entries());
    StarterChooserScene::from_pack(&pack).expect("the synthetic pack builds the chooser")
}

/// Frames the entry fade needs before the palette is fully restored.
pub(super) const FADE_FRAMES: usize = 24;

/// Feeds one held-button set per frame, computing each frame's fresh edges.
#[derive(Debug, Default)]
pub(super) struct Pad {
    state: ButtonState,
}

impl Pad {
    pub(super) fn frame(&mut self, held: Buttons) -> ButtonState {
        self.state.update(held);
        self.state
    }
}

/// A [`Chooser`] driven the way a player would: press for a frame.
#[derive(Debug)]
pub(super) struct Driver {
    pub(super) chooser: Chooser,
    pub(super) frames: usize,
    pad: Pad,
}

impl Driver {
    pub(super) fn new() -> Self {
        Self {
            chooser: Chooser::new(),
            frames: 0,
            pad: Pad::default(),
        }
    }

    /// Runs one frame with `held` down.
    pub(super) fn frame(&mut self, held: Buttons) -> FrameResult {
        let buttons = self.pad.frame(held);
        self.frames += 1;
        self.chooser.step(buttons)
    }

    /// Runs `count` frames with nothing pressed.
    pub(super) fn idle(&mut self, count: usize) {
        for _ in 0..count {
            self.frame(Buttons::NONE);
        }
    }

    /// Presses `button` for one frame, then releases it for one.
    pub(super) fn tap(&mut self, button: Buttons) -> FrameResult {
        let result = self.frame(button);
        self.frame(Buttons::NONE);
        result
    }

    /// A chooser that has drawn its selection and awaits input.
    pub(super) fn ready() -> Self {
        let mut driver = Self::new();
        driver.frame(Buttons::NONE);
        driver
    }

    /// Selects `target` from Torchic through the real input path.
    pub(super) fn select(&mut self, target: crate::starter_chooser::StarterChoice) {
        use crate::starter_chooser::StarterChoice;
        let button = match target {
            StarterChoice::Treecko => Buttons::LEFT,
            StarterChoice::Mudkip => Buttons::RIGHT,
            StarterChoice::Torchic => return,
        };
        self.tap(button);
        self.idle(3);
    }

    /// Presses A on the selection and runs until the YES/NO menu accepts input.
    pub(super) fn open_confirmation(&mut self) {
        self.tap(Buttons::A);
        while self.chooser.phase != crate::starter_chooser::StarterChooserPhase::ConfirmInput {
            self.frame(Buttons::NONE);
        }
    }
}

pub(super) const fn rgb(color: Bgr555) -> Rgb888 {
    color.to_rgb888()
}
