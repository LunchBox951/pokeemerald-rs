//! The asset inventories extraction fixes and `gen-rom-profile` requires.
//!
//! One list per domain, read by the extractor and, for each domain the ROM
//! profile covers, by the profile generator so a pack missing a root is
//! refused rather than half-described. The naming-screen inventory is
//! extraction-only until the generator gains that family.

use std::path::Path;

use super::bag::{fixed_ids, icon_ids};
use super::battle_interface::healthbox_element_pack_ids;
use super::battle_interface::healthbox_frame_pack_ids;
use super::battle_interface::pack_ids as battle_interface_pack_ids;
use super::opening_scene::pack_ids as opening_scene_pack_ids;
use super::sprites::{FIRST_GENERIC_NPC_PALETTE, LAST_GENERIC_NPC_PALETTE};
use super::starter_chooser::pack_ids as chooser_pack_ids;
use super::ExtractError;

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

pub(crate) const TILESETS: [TilesetSource; 9] = [
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
    TilesetSource {
        category: "secondary",
        name: "generic_building",
        animations: &[],
    },
    TilesetSource {
        category: "secondary",
        name: "pokemon_center",
        animations: &[],
    },
    TilesetSource {
        category: "secondary",
        name: "shop",
        animations: &[],
    },
    TilesetSource {
        category: "secondary",
        name: "inside_of_truck",
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

/// A non-people object-event sheet: its directory under
/// `graphics/object_events/pics` and its file stem. The pack id is
/// `sprite/<dir>/<stem>`, which no people sheet's `sprite/<stem>` can equal.
#[derive(Clone, Copy)]
pub(crate) struct ObjectSheet {
    pub(crate) dir: &'static str,
    pub(crate) stem: &'static str,
}

impl ObjectSheet {
    /// The pack id of this sheet.
    pub(crate) fn pack_id(self) -> String {
        format!("sprite/{}/{}", self.dir, self.stem)
    }
}

// The non-people `gObjectEventGraphicsInfo_*` pictures the checkpoint maps'
// fixed object events name (`data/maps/*/map.json`, `graphics_id`): the
// truck and moving box, Birch's bag, item balls, cuttable trees, the
// Vigoroth and Zigzagoon, the two bedroom dolls, and the berry-tree stages
// (`dirt_pile`, `sprout`) with the Cheri and Leppa sheets Route 103 grows.
pub(crate) const OBJECT_SHEETS: [ObjectSheet; 13] = [
    ObjectSheet {
        dir: "berry_trees",
        stem: "cheri",
    },
    ObjectSheet {
        dir: "berry_trees",
        stem: "dirt_pile",
    },
    ObjectSheet {
        dir: "berry_trees",
        stem: "leppa",
    },
    ObjectSheet {
        dir: "berry_trees",
        stem: "sprout",
    },
    ObjectSheet {
        dir: "dolls",
        stem: "pichu_doll",
    },
    ObjectSheet {
        dir: "dolls",
        stem: "swablu_doll",
    },
    ObjectSheet {
        dir: "misc",
        stem: "birchs_bag",
    },
    ObjectSheet {
        dir: "misc",
        stem: "cuttable_tree",
    },
    ObjectSheet {
        dir: "misc",
        stem: "item_ball",
    },
    ObjectSheet {
        dir: "misc",
        stem: "moving_box",
    },
    ObjectSheet {
        dir: "misc",
        stem: "truck",
    },
    ObjectSheet {
        dir: "pokemon",
        stem: "enemy_zigzagoon",
    },
    ObjectSheet {
        dir: "pokemon",
        stem: "vigoroth",
    },
];

/// The palettes beside the generic `npc_*` banks that those pictures load
/// through `sObjectEventSpritePalettes`.
pub(crate) const OBJECT_PALETTES: [&str; 4] =
    ["enemy_zigzagoon", "moving_box", "truck", "vigoroth"];

/// Every required object-event sheet id: the people sheets, then the
/// non-people ones, in manifest order.
pub(crate) fn sprite_sheet_ids() -> Vec<String> {
    PEOPLE_SHEETS
        .map(|stem| format!("sprite/{stem}"))
        .into_iter()
        .chain(OBJECT_SHEETS.map(ObjectSheet::pack_id))
        .collect()
}

/// Every required sprite palette id, in extraction order.
pub(crate) fn sprite_palette_ids() -> Vec<String> {
    ["brendan", "may"]
        .into_iter()
        .map(|name| format!("sprite/palette/{name}"))
        .chain(
            (FIRST_GENERIC_NPC_PALETTE..=LAST_GENERIC_NPC_PALETTE)
                .map(|n| format!("sprite/palette/npc_{n}")),
        )
        .chain(OBJECT_PALETTES.map(|name| format!("sprite/palette/{name}")))
        .collect()
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

/// The pack id of the main-menu background palette.
pub(crate) const MAIN_MENU_BG_PALETTE_ID: &str = "interface/palette/main_menu_bg";

/// Every interface palette pack id extraction writes and the generator requires.
pub(crate) const INTERFACE_PALETTE_IDS: [&str; 1] = [MAIN_MENU_BG_PALETTE_ID];

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

pub(crate) const LAYOUTS: [LayoutSource; 16] = [
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
    LayoutSource {
        upstream_id: "LAYOUT_INSIDE_OF_TRUCK",
        pack_name: "inside_of_truck",
    },
    LayoutSource {
        upstream_id: "LAYOUT_HOUSE1",
        pack_name: "house1",
    },
    LayoutSource {
        upstream_id: "LAYOUT_HOUSE2",
        pack_name: "house2",
    },
    LayoutSource {
        upstream_id: "LAYOUT_MART",
        pack_name: "mart",
    },
    LayoutSource {
        upstream_id: "LAYOUT_POKEMON_CENTER_1F",
        pack_name: "pokemon_center_1f",
    },
    LayoutSource {
        upstream_id: "LAYOUT_POKEMON_CENTER_2F",
        pack_name: "pokemon_center_2f",
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

/// One starter whose static battler art the pack carries.
#[derive(Clone, Copy)]
pub(crate) struct BattlerSource {
    /// The directory under `graphics/pokemon/` and the pack id's species stem.
    pub(crate) name: &'static str,
    /// The species' row in `gMonStillFrontPicTable` and the three tables beside it.
    pub(crate) species: assets::SpeciesId,
}

impl BattlerSource {
    /// The front image id.
    pub(crate) fn front_id(self) -> String {
        format!("pokemon/{}/front", self.name)
    }

    /// The back image id.
    pub(crate) fn back_id(self) -> String {
        format!("pokemon/{}/back", self.name)
    }

    /// The normal palette id.
    pub(crate) fn normal_id(self) -> String {
        format!("pokemon/{}/palette/normal", self.name)
    }

    /// The shiny palette id.
    pub(crate) fn shiny_id(self) -> String {
        format!("pokemon/{}/palette/shiny", self.name)
    }

    /// Every id the species owns: front, back, normal, shiny.
    pub(crate) fn ids(self) -> [String; 4] {
        [
            self.front_id(),
            self.back_id(),
            self.normal_id(),
            self.shiny_id(),
        ]
    }
}

/// Treecko, Torchic, and Mudkip (`still_front_pic_table.h:283,286,289`,
/// `back_pic_table.h:280,283,286`).
pub(crate) const BATTLERS: [BattlerSource; 3] = [
    BattlerSource {
        name: "treecko",
        species: assets::SpeciesId::TREECKO,
    },
    BattlerSource {
        name: "torchic",
        species: assets::SpeciesId::TORCHIC,
    },
    BattlerSource {
        name: "mudkip",
        species: assets::SpeciesId::MUDKIP,
    },
];

/// Every `bag/` pack id extraction emits: the fixed manifest, then the
/// item icons `gItemIconTable` selects.
///
/// # Errors
///
/// [`ExtractError`] if the checkout's icon table cannot be read.
pub(crate) fn bag_ids(upstream: &Path) -> Result<Vec<String>, ExtractError> {
    Ok([fixed_ids(), icon_ids(upstream)?].concat())
}

/// Every `starter-chooser/` pack id extraction emits: three sheets, their
/// palettes, and two background tilemaps.
pub(crate) fn starter_chooser_ids() -> Vec<String> {
    chooser_pack_ids()
}

/// Every `opening-scene/` pack id extraction emits: five sheets, eight
/// palettes, and three tilemaps.
pub(crate) fn opening_scene_ids() -> Vec<String> {
    opening_scene_pack_ids()
}

/// Where the naming screen's sources live, relative to the checkout.
pub(crate) const NAMING_SCREEN_DIRECTORY: &str = "graphics/naming_screen";

/// One naming-screen sheet: its file stem, its PNG's size, and how many of its
/// leading 8x8 tiles upstream's build keeps (`None` keeps the whole sheet).
#[derive(Clone, Copy, Debug)]
pub(crate) struct NamingScreenSheet {
    pub(crate) stem: &'static str,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) kept_tiles: Option<u32>,
}

const fn whole_sheet(stem: &'static str, width: u32, height: u32) -> NamingScreenSheet {
    NamingScreenSheet {
        stem,
        width,
        height,
        kept_tiles: None,
    }
}

/// The cursor sheets are built with `-num_tiles 5` (`graphics.c:1410-1412`).
const fn cursor_sheet(stem: &'static str) -> NamingScreenSheet {
    NamingScreenSheet {
        stem,
        width: 16,
        height: 24,
        kept_tiles: Some(5),
    }
}

/// Naming-screen sheets as `naming-screen/image/<stem>`: `graphics.c:1403-1414`
/// plus the two PC icons `naming_screen.c:185-186` declares locally. The
/// walking sheets the player icon shows are the sprite manifest's.
pub(crate) const NAMING_SCREEN_SHEETS: [NamingScreenSheet; 15] = [
    whole_sheet("menu", 24, 128),
    whole_sheet("page_swap_frame", 40, 32),
    whole_sheet("back_button", 40, 24),
    whole_sheet("ok_button", 40, 24),
    whole_sheet("page_swap_upper", 40, 8),
    whole_sheet("page_swap_lower", 40, 8),
    whole_sheet("page_swap_others", 40, 8),
    cursor_sheet("cursor"),
    cursor_sheet("cursor_squished"),
    cursor_sheet("cursor_filled"),
    whole_sheet("page_swap_button", 32, 16),
    whole_sheet("input_arrow", 8, 8),
    whole_sheet("underscore", 8, 8),
    whole_sheet("pc_icon_off", 16, 24),
    whole_sheet("pc_icon_on", 16, 24),
];

/// Naming-screen palettes as `naming-screen/palette/<stem>`: the six rows of
/// `gNamingScreenMenu_Pal` (`graphics.c:1394-1402`), then `sKeyboard_Pal`
/// (`naming_screen.c:187`). `rival.pal` is unused upstream (`:188`).
pub(crate) const NAMING_SCREEN_PALETTES: [&str; 7] = [
    "menu",
    "page_swap_upper",
    "page_swap_lower",
    "page_swap_others",
    "buttons",
    "cursor",
    "keyboard",
];

/// Naming-screen tilemaps as `naming-screen/raw/<stem>` (`graphics.c:1416-1419`).
pub(crate) const NAMING_SCREEN_TILEMAPS: [&str; 4] = [
    "background",
    "keyboard_upper",
    "keyboard_lower",
    "keyboard_symbols",
];

/// A 32x20 tilemap of 2-byte entries.
pub(crate) const NAMING_SCREEN_TILEMAP_BYTES: usize = 32 * 20 * 2;

/// Every `naming-screen/` pack id extraction emits: sheets, then palettes,
/// then tilemaps.
pub(crate) fn naming_screen_ids() -> Vec<String> {
    let images = NAMING_SCREEN_SHEETS
        .iter()
        .map(|sheet| format!("naming-screen/image/{}", sheet.stem));
    let palettes = NAMING_SCREEN_PALETTES
        .iter()
        .map(|stem| format!("naming-screen/palette/{stem}"));
    let tilemaps = NAMING_SCREEN_TILEMAPS
        .iter()
        .map(|stem| format!("naming-screen/raw/{stem}"));
    images.chain(palettes).chain(tilemaps).collect()
}

/// Every `battle-interface/` pack id extraction emits: the textbox sheet,
/// its three palettes, and the textbox tilemap.
pub(crate) fn battle_interface_ids() -> Vec<String> {
    battle_interface_pack_ids()
}

/// Every `battle-interface/` pack id the health-box extractor emits: the
/// thirteen health-box elements and the battle bar. No ROM root imports them
/// yet (#2178), so profile generation leaves them unclaimed.
pub(crate) fn healthbox_element_ids() -> Vec<String> {
    healthbox_element_pack_ids()
}

/// Every `battle-interface/` pack id the health-box frame extractor emits: the
/// singles player and opponent frames, the enemy shadow, and the two ball
/// palettes. No ROM root imports them yet (#2178), so profile generation
/// leaves them unclaimed.
pub(crate) fn healthbox_frame_ids() -> Vec<String> {
    healthbox_frame_pack_ids()
}
