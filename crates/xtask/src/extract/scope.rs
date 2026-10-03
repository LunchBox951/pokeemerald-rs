//! The asset inventories extraction fixes and `gen-rom-profile` requires.
//!
//! One list per domain, read by both the extractor and the profile generator
//! so a pack missing a root is refused rather than half-described.

#[derive(Clone, Copy)]
pub(crate) struct TilesetSource {
    pub(crate) category: &'static str,
    pub(crate) name: &'static str,
}

impl TilesetSource {
    /// The pack id of this tileset's tile sheet.
    pub(crate) fn tiles_id(self) -> String {
        format!("tileset/{}/tiles", self.name)
    }
}

pub(crate) const TILESETS: [TilesetSource; 5] = [
    TilesetSource {
        category: "primary",
        name: "general",
    },
    TilesetSource {
        category: "primary",
        name: "building",
    },
    TilesetSource {
        category: "secondary",
        name: "petalburg",
    },
    TilesetSource {
        category: "secondary",
        name: "brendans_mays_house",
    },
    TilesetSource {
        category: "secondary",
        name: "lab",
    },
];

/// Title sheets built from a PNG, as `title/image/<stem>`.
pub(crate) const TITLE_SCREEN_IMAGES: [&str; 6] = [
    "clouds",
    "emerald_version",
    "logo_shine",
    "pokemon_logo",
    "press_start",
    "rayquaza",
];

/// Title tilemaps built from a `.bin`, as `title/raw/<stem>`.
pub(crate) const TITLE_SCREEN_TILEMAPS: [&str; 3] = ["clouds", "pokemon_logo", "rayquaza"];

// `src/graphics.c`'s `gTitleScreenEmeraldVersionPal` and
// `gTitleScreenPressStartPal` declarations build these palettes from each
// PNG's embedded `PLTE` chunk instead of a sibling `.pal` file.
pub(crate) const TITLE_SCREEN_EMBEDDED_PALETTE_SHEETS: [&str; 2] =
    ["emerald_version", "press_start"];

/// Title palettes built from a sibling `.pal`, as `title/palette/<stem>`.
pub(crate) const TITLE_SCREEN_FILE_PALETTES: [&str; 3] =
    ["pokemon_logo", "rayquaza_and_clouds", "unused"];

// `graphics_file_rules.mk` builds `pokemon_logo.gbapal` with this limit.
pub(crate) const TITLE_SCREEN_PALETTE_CUTS: [(&str, usize); 1] = [("pokemon_logo", 224)];

/// Every title pack id the generator requires, in manifest order.
pub(crate) fn title_ids() -> Vec<String> {
    let images = TITLE_SCREEN_IMAGES.map(|s| format!("title/image/{s}"));
    let tilemaps = TITLE_SCREEN_TILEMAPS.map(|s| format!("title/raw/{s}"));
    let palettes = TITLE_SCREEN_EMBEDDED_PALETTE_SHEETS
        .into_iter()
        .chain(TITLE_SCREEN_FILE_PALETTES)
        .map(|s| format!("title/palette/{s}"));
    images.into_iter().chain(tilemaps).chain(palettes).collect()
}
