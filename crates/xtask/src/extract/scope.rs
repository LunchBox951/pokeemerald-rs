//! The asset inventories extraction fixes and `gen-rom-profile` requires.
//!
//! One list per domain, read by both the extractor and the profile generator
//! so a pack missing a root is refused rather than half-described.

#[derive(Clone, Copy)]
pub(crate) struct TilesetSource {
    pub(crate) category: &'static str,
    pub(crate) name: &'static str,
    /// The animations this tileset must carry, as `(name, frame count)`.
    pub(crate) animations: &'static [(&'static str, u32)],
}

impl TilesetSource {
    /// Every required animation frame id, as `tileset/<name>/anim/<anim>/<n>`.
    pub(crate) fn animation_ids(self) -> Vec<String> {
        self.animations
            .iter()
            .flat_map(|&(anim, frames)| {
                (0..frames).map(move |n| format!("tileset/{}/anim/{anim}/{n}", self.name))
            })
            .collect()
    }

    /// The pack id of this tileset's tile sheet.
    pub(crate) fn tiles_id(self) -> String {
        format!("tileset/{}/tiles", self.name)
    }
}

// Unique frame files of `src/tileset_anims.c`'s `sTilesetAnims_General_*` and
// `sTilesetAnims_Building_TVTurnedOn` declarations at the pinned upstream
// commit. Playback repeats (flower, sand-water-edge) are not extra assets.
const GENERAL_ANIMATIONS: [(&str, u32); 5] = [
    ("flower", 3),
    ("land_water_edge", 4),
    ("sand_water_edge", 7),
    ("water", 8),
    ("waterfall", 4),
];
const BUILDING_ANIMATIONS: [(&str, u32); 1] = [("tv_turned_on", 2)];

/// Every required tileset animation frame id, in manifest order.
pub(crate) fn tileset_animation_ids() -> Vec<String> {
    TILESETS
        .into_iter()
        .flat_map(TilesetSource::animation_ids)
        .collect()
}

pub(crate) const TILESETS: [TilesetSource; 5] = [
    TilesetSource {
        category: "primary",
        name: "general",
        animations: &GENERAL_ANIMATIONS,
    },
    TilesetSource {
        category: "primary",
        name: "building",
        animations: &BUILDING_ANIMATIONS,
    },
    TilesetSource {
        category: "secondary",
        name: "petalburg",
        animations: &[],
    },
    TilesetSource {
        category: "secondary",
        name: "brendans_mays_house",
        animations: &[],
    },
    TilesetSource {
        category: "secondary",
        name: "lab",
        animations: &[],
    },
];

// Every people sheet under `graphics/object_events/pics/people`, as pack ids
// `sprite/<stem>`; overworld construction loads the player sheets unconditionally.
pub(crate) const PEOPLE_SHEETS: [&str; 133] = [
    "artist",
    "beauty",
    "black_belt",
    "boy_1",
    "boy_2",
    "boy_3",
    "brendan/acro_bike",
    "brendan/decorating",
    "brendan/field_move",
    "brendan/fishing",
    "brendan/mach_bike",
    "brendan/running",
    "brendan/surfing",
    "brendan/underwater",
    "brendan/walking",
    "brendan/watering",
    "bug_catcher",
    "cameraman",
    "camper",
    "contest_judge",
    "cook",
    "cycling_triathlete_f",
    "cycling_triathlete_m",
    "devon_employee",
    "elite_four/drake",
    "elite_four/glacia",
    "elite_four/phoebe",
    "elite_four/sidney",
    "expert_f",
    "expert_m",
    "fat_man",
    "fisherman",
    "frontier_brains/anabel",
    "frontier_brains/brandon",
    "frontier_brains/greta",
    "frontier_brains/lucy",
    "frontier_brains/noland",
    "frontier_brains/spenser",
    "frontier_brains/tucker",
    "gameboy_kid",
    "gentleman",
    "girl_1",
    "girl_2",
    "girl_3",
    "gym_leaders/brawly",
    "gym_leaders/flannery",
    "gym_leaders/juan",
    "gym_leaders/liza",
    "gym_leaders/norman",
    "gym_leaders/roxanne",
    "gym_leaders/tate",
    "gym_leaders/wattson",
    "gym_leaders/winona",
    "hex_maniac",
    "hiker",
    "hot_springs_old_woman",
    "lass",
    "leaf",
    "link_receptionist",
    "little_boy",
    "little_girl",
    "man_1",
    "man_2",
    "man_3",
    "man_4",
    "man_5",
    "maniac",
    "mart_employee",
    "mauville_old_man_1",
    "mauville_old_man_2",
    "may/acro_bike",
    "may/decorating",
    "may/field_move",
    "may/fishing",
    "may/mach_bike",
    "may/running",
    "may/surfing",
    "may/underwater",
    "may/walking",
    "may/watering",
    "mom",
    "mystery_event_deliveryman",
    "ninja_boy",
    "nurse",
    "old_man",
    "old_woman",
    "picnicker",
    "pokefan_f",
    "pokefan_m",
    "prof_birch",
    "psychic_m",
    "quinty_plump",
    "red",
    "reporter_f",
    "reporter_m",
    "rich_boy",
    "rooftop_sale_woman",
    "rs_little_boy",
    "ruby_sapphire_brendan/running",
    "ruby_sapphire_brendan/walking",
    "ruby_sapphire_may/running",
    "ruby_sapphire_may/walking",
    "running_triathlete_f",
    "running_triathlete_m",
    "sailor",
    "school_kid_m",
    "scientist_1",
    "scientist_2",
    "scott",
    "steven",
    "swimmer_f",
    "swimmer_m",
    "teala",
    "team_aqua/aqua_member_f",
    "team_aqua/aqua_member_m",
    "team_aqua/archie",
    "team_magma/magma_member_f",
    "team_magma/magma_member_m",
    "team_magma/maxie",
    "tuber_f",
    "tuber_m",
    "tuber_m_swimming",
    "twin",
    "union_room_attendant",
    "unused_woman",
    "wallace",
    "wally",
    "woman_1",
    "woman_2",
    "woman_3",
    "woman_4",
    "woman_5",
    "youngster",
];

/// Every required people sprite sheet id, in manifest order.
pub(crate) fn sprite_sheet_ids() -> Vec<String> {
    PEOPLE_SHEETS.map(|stem| format!("sprite/{stem}")).to_vec()
}

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
