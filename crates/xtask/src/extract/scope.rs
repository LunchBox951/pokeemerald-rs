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

#[derive(Clone, Copy)]
pub(crate) struct FontSource {
    pub(crate) filename: &'static str,
    pub(crate) pack_id: &'static str,
}

pub(crate) const FONTS: [FontSource; 5] = [
    FontSource {
        filename: "latin_small.png",
        pack_id: "font/small/glyphs",
    },
    FontSource {
        filename: "latin_normal.png",
        pack_id: "font/normal/glyphs",
    },
    FontSource {
        filename: "latin_short.png",
        pack_id: "font/short/glyphs",
    },
    FontSource {
        filename: "latin_narrow.png",
        pack_id: "font/narrow/glyphs",
    },
    FontSource {
        filename: "latin_small_narrow.png",
        pack_id: "font/small_narrow/glyphs",
    },
];

#[derive(Clone, Copy)]
pub(crate) struct LayoutSource {
    pub(crate) upstream_id: &'static str,
    pub(crate) pack_name: &'static str,
}

pub(crate) const LAYOUTS: [LayoutSource; 10] = [
    LayoutSource {
        upstream_id: "LAYOUT_LITTLEROOT_TOWN",
        pack_name: "littleroot_town",
    },
    LayoutSource {
        upstream_id: "LAYOUT_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F",
        pack_name: "littleroot_town_brendans_house_1f",
    },
    LayoutSource {
        upstream_id: "LAYOUT_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F",
        pack_name: "littleroot_town_brendans_house_2f",
    },
    LayoutSource {
        upstream_id: "LAYOUT_LITTLEROOT_TOWN_MAYS_HOUSE_1F",
        pack_name: "littleroot_town_mays_house_1f",
    },
    LayoutSource {
        upstream_id: "LAYOUT_LITTLEROOT_TOWN_MAYS_HOUSE_2F",
        pack_name: "littleroot_town_mays_house_2f",
    },
    LayoutSource {
        upstream_id: "LAYOUT_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB",
        pack_name: "littleroot_town_professor_birchs_lab",
    },
    LayoutSource {
        upstream_id: "LAYOUT_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB_WITH_TABLE",
        pack_name: "littleroot_town_professor_birchs_lab_with_table",
    },
    LayoutSource {
        upstream_id: "LAYOUT_ROUTE101",
        pack_name: "route101",
    },
    LayoutSource {
        upstream_id: "LAYOUT_OLDALE_TOWN",
        pack_name: "oldale_town",
    },
    LayoutSource {
        upstream_id: "LAYOUT_ROUTE103",
        pack_name: "route103",
    },
];

/// Every layout pack id the generator requires: each layout's map and border.
pub(crate) fn layout_ids() -> Vec<String> {
    LAYOUTS
        .iter()
        .flat_map(|layout| {
            [
                format!("layout/{}/map", layout.pack_name),
                format!("layout/{}/border", layout.pack_name),
            ]
        })
        .collect()
}

pub(crate) const MESSAGE_BOX_STEM: &str = "message_box";

pub(crate) const TEXT_WINDOW_IMAGE_STEMS: [&str; 21] = [
    "1",
    "2",
    "3",
    "4",
    "5",
    "6",
    "7",
    "8",
    "9",
    "10",
    "11",
    "12",
    "13",
    "14",
    "15",
    "16",
    "17",
    "18",
    "19",
    "20",
    MESSAGE_BOX_STEM,
];

pub(crate) const TEXT_WINDOW_PALETTE_STEMS: [&str; 4] =
    ["text_pal1", "text_pal2", "text_pal3", "text_pal4"];

/// Every text-window pack id the generator requires: each frame image and
/// its palette (the PNG's embedded one), then the standalone palette banks.
pub(crate) fn text_window_ids() -> Vec<String> {
    let images = TEXT_WINDOW_IMAGE_STEMS.map(|s| format!("text-window/image/{s}"));
    let image_palettes = TEXT_WINDOW_IMAGE_STEMS.map(|s| format!("text-window/palette/{s}"));
    let banks = TEXT_WINDOW_PALETTE_STEMS.map(|s| format!("text-window/palette/{s}"));
    images
        .into_iter()
        .chain(image_palettes)
        .chain(banks)
        .collect()
}
