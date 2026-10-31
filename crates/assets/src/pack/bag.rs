//! Typed ordinary-bag graphics (upstream `src/item_menu.c`,
//! `src/item_menu_icons.c`, and `src/item_icon.c`).
//!
//! Every lookup re-validates shapes, since a hand-built or corrupt pack can
//! bypass extraction.

use crate::items::{ItemId, ITEMS_COUNT};

use super::{AssetPack, ImageRef, PackError, PaletteRef, PalettedImage};

const MENU_IMAGE_ID: &str = "bag/image/menu";
const MENU_MALE_PALETTE_ID: &str = "bag/palette/menu/male";
const MENU_FEMALE_PALETTE_ID: &str = "bag/palette/menu/female";
const MENU_TILEMAP_ID: &str = "bag/raw/menu";
const MALE_IMAGE_ID: &str = "bag/image/male";
const FEMALE_IMAGE_ID: &str = "bag/image/female";
const BAG_PALETTE_ID: &str = "bag/palette/bag";
const ROTATING_BALL_IMAGE_ID: &str = "bag/image/rotating_ball";
const ROTATING_BALL_PALETTE_ID: &str = "bag/palette/rotating_ball";
const SELECT_BUTTON_IMAGE_ID: &str = "bag/image/select_button";
const HM_IMAGE_ID: &str = "bag/image/hm";
const SCROLL_INDICATOR_IMAGE_ID: &str = "bag/image/scroll_indicator";
const SCROLL_INDICATOR_PALETTE_ID: &str = "bag/palette/scroll_indicator";
const SWAP_LINE_IMAGE_ID: &str = "bag/image/swap_line";
const SWAP_LINE_PALETTE_ID: &str = "bag/palette/swap_line";
const MENU_INFO_IMAGE_ID: &str = "bag/image/menu_info";
const MENU_INFO_PALETTE_IDS: [&str; MENU_INFO_PALETTES] = [
    "bag/palette/menu_info1",
    "bag/palette/menu_info2",
    "bag/palette/menu_info3",
];
const ICON_IMAGE_PREFIX: &str = "bag/item-icon/image/";
const ICON_PALETTE_PREFIX: &str = "bag/item-icon/palette/";

const SOURCE_BIT_DEPTH: u8 = 4;
const BANK_PALETTE_COLORS: u16 = 16;
const MENU_PALETTE_COLORS: u16 = 32;
const MENU_WIDTH: u32 = 128;
const MENU_HEIGHT: u32 = 32;
const BAG_WIDTH: u32 = 64;
const BAG_HEIGHT: u32 = 384;
const ROTATING_BALL_SIDE: u32 = 16;
const SELECT_BUTTON_WIDTH: u32 = 24;
const SELECT_BUTTON_HEIGHT: u32 = 16;
const HM_SIDE: u32 = 16;
const SCROLL_INDICATOR_WIDTH: u32 = 16;
const SCROLL_INDICATOR_HEIGHT: u32 = 32;
const SWAP_LINE_WIDTH: u32 = 16;
const SWAP_LINE_HEIGHT: u32 = 32;
const MENU_INFO_SIDE: u32 = 128;
const ICON_SIDE: u32 = 24;
const TILEMAP_BYTES: usize = 32 * 32 * 2;
const PALETTE_ENTRY_BYTES: usize = 2;

/// Upstream `gMenuInfoElements{1,2,3}_Pal` (`src/graphics.c:1264-1266`).
const MENU_INFO_PALETTES: usize = 3;
/// Tiles per row of the menu-info sheet, which `gMenuInfoElements_Gfx` stores
/// tile by tile and `sMenuInfoIcons` indexes by tile.
const MENU_INFO_TILES_PER_ROW: u32 = MENU_INFO_SIDE / 8;
const TYPE_ICON_WIDTH: u32 = 32;
const LABEL_ICON_WIDTH: u32 = 42;
const MENU_INFO_ICON_HEIGHT: u32 = 12;

/// `sMenuInfoIcons[TYPE_x + 1].offset` (`src/menu.c:115-133`) in tiles, indexed
/// by the combat-type id `gBattleMoves[].type`, `TYPE_NORMAL` through
/// `TYPE_DARK` (the `TYPE_MYSTERY` slot included).
const TYPE_ICON_OFFSETS: [u16; 18] = [
    0x20, 0x64, 0x60, 0x80, 0x48, 0x44, 0x6C, 0x68, 0x88, 0xA4, 0x24, 0x28, 0x2C, 0x40, 0x84, 0x4C,
    0xA0, 0x8C,
];

/// Upstream `ITEM_LIST_END`, which selects the "return to field" arrow icon.
const LIST_END: ItemId = ItemId(u16::MAX);

/// Upstream `gItemIconTable` as (image stem, palette stem) per [`ItemId`],
/// then the list-end arrow; upstream shares sheets and palettes independently.
const ICON_STEMS: [(&str, &str); ITEMS_COUNT + 1] = [
    ("question_mark", "question_mark"),
    ("master_ball", "master_ball"),
    ("ultra_ball", "ultra_ball"),
    ("great_ball", "great_ball"),
    ("poke_ball", "poke_ball"),
    ("safari_ball", "safari_ball"),
    ("net_ball", "net_ball"),
    ("dive_ball", "dive_ball"),
    ("nest_ball", "nest_ball"),
    ("repeat_ball", "repeat_ball"),
    ("timer_ball", "repeat_ball"),
    ("luxury_ball", "luxury_ball"),
    ("premier_ball", "luxury_ball"),
    ("potion", "potion"),
    ("antidote", "antidote"),
    ("status_heal", "burn_heal"),
    ("status_heal", "ice_heal"),
    ("status_heal", "awakening"),
    ("status_heal", "paralyze_heal"),
    ("large_potion", "full_restore"),
    ("large_potion", "max_potion"),
    ("potion", "hyper_potion"),
    ("potion", "super_potion"),
    ("full_heal", "full_heal"),
    ("revive", "revive"),
    ("max_revive", "revive"),
    ("fresh_water", "fresh_water"),
    ("soda_pop", "soda_pop"),
    ("lemonade", "lemonade"),
    ("moomoo_milk", "moomoo_milk"),
    ("powder", "energy_powder"),
    ("energy_root", "energy_root"),
    ("powder", "heal_powder"),
    ("revival_herb", "revival_herb"),
    ("ether", "ether"),
    ("ether", "max_ether"),
    ("ether", "elixir"),
    ("ether", "max_elixir"),
    ("lava_cookie", "lava_cookie_and_letter"),
    ("flute", "blue_flute"),
    ("flute", "yellow_flute"),
    ("flute", "red_flute"),
    ("flute", "black_flute"),
    ("flute", "white_flute"),
    ("berry_juice", "berry_juice"),
    ("sacred_ash", "sacred_ash"),
    ("powder", "shoal_salt"),
    ("shoal_shell", "shell"),
    ("shard", "red_shard"),
    ("shard", "blue_shard"),
    ("shard", "yellow_shard"),
    ("shard", "green_shard"),
    ("question_mark", "question_mark"),
    ("question_mark", "question_mark"),
    ("question_mark", "question_mark"),
    ("question_mark", "question_mark"),
    ("question_mark", "question_mark"),
    ("question_mark", "question_mark"),
    ("question_mark", "question_mark"),
    ("question_mark", "question_mark"),
    ("question_mark", "question_mark"),
    ("question_mark", "question_mark"),
    ("question_mark", "question_mark"),
    ("hp_up", "hp_up"),
    ("vitamin", "protein"),
    ("vitamin", "iron"),
    ("vitamin", "carbos"),
    ("vitamin", "calcium"),
    ("rare_candy", "rare_candy"),
    ("pp_up", "pp_up"),
    ("vitamin", "zinc"),
    ("pp_max", "pp_max"),
    ("question_mark", "question_mark"),
    ("battle_stat_item", "guard_spec"),
    ("battle_stat_item", "dire_hit"),
    ("battle_stat_item", "x_attack"),
    ("battle_stat_item", "x_defend"),
    ("battle_stat_item", "x_speed"),
    ("battle_stat_item", "x_accuracy"),
    ("battle_stat_item", "x_special"),
    ("poke_doll", "poke_doll"),
    ("fluffy_tail", "fluffy_tail"),
    ("question_mark", "question_mark"),
    ("repel", "super_repel"),
    ("repel", "max_repel"),
    ("escape_rope", "escape_rope"),
    ("repel", "repel"),
    ("question_mark", "question_mark"),
    ("question_mark", "question_mark"),
    ("question_mark", "question_mark"),
    ("question_mark", "question_mark"),
    ("question_mark", "question_mark"),
    ("question_mark", "question_mark"),
    ("sun_stone", "sun_stone"),
    ("moon_stone", "moon_stone"),
    ("fire_stone", "fire_stone"),
    ("thunder_stone", "thunder_stone"),
    ("water_stone", "water_stone"),
    ("leaf_stone", "leaf_stone"),
    ("question_mark", "question_mark"),
    ("question_mark", "question_mark"),
    ("question_mark", "question_mark"),
    ("question_mark", "question_mark"),
    ("tiny_mushroom", "mushroom"),
    ("big_mushroom", "mushroom"),
    ("question_mark", "question_mark"),
    ("pearl", "pearl"),
    ("big_pearl", "pearl"),
    ("stardust", "star"),
    ("star_piece", "star"),
    ("nugget", "nugget"),
    ("heart_scale", "heart_scale"),
    ("question_mark", "question_mark"),
    ("question_mark", "question_mark"),
    ("question_mark", "question_mark"),
    ("question_mark", "question_mark"),
    ("question_mark", "question_mark"),
    ("question_mark", "question_mark"),
    ("question_mark", "question_mark"),
    ("question_mark", "question_mark"),
    ("question_mark", "question_mark"),
    ("orange_mail", "orange_mail"),
    ("harbor_mail", "harbor_mail"),
    ("glitter_mail", "glitter_mail"),
    ("mech_mail", "mech_mail"),
    ("wood_mail", "wood_mail"),
    ("wave_mail", "wave_mail"),
    ("bead_mail", "bead_mail"),
    ("shadow_mail", "shadow_mail"),
    ("tropic_mail", "tropic_mail"),
    ("dream_mail", "dream_mail"),
    ("fab_mail", "fab_mail"),
    ("retro_mail", "retro_mail"),
    ("cheri_berry", "cheri_berry"),
    ("chesto_berry", "chesto_berry"),
    ("pecha_berry", "pecha_berry"),
    ("rawst_berry", "rawst_berry"),
    ("aspear_berry", "aspear_berry"),
    ("leppa_berry", "leppa_berry"),
    ("oran_berry", "oran_berry"),
    ("persim_berry", "persim_berry"),
    ("lum_berry", "lum_berry"),
    ("sitrus_berry", "sitrus_berry"),
    ("figy_berry", "figy_berry"),
    ("wiki_berry", "wiki_berry"),
    ("mago_berry", "mago_berry"),
    ("aguav_berry", "aguav_berry"),
    ("iapapa_berry", "iapapa_berry"),
    ("razz_berry", "razz_berry"),
    ("bluk_berry", "bluk_berry"),
    ("nanab_berry", "nanab_berry"),
    ("wepear_berry", "wepear_berry"),
    ("pinap_berry", "pinap_berry"),
    ("pomeg_berry", "pomeg_berry"),
    ("kelpsy_berry", "kelpsy_berry"),
    ("qualot_berry", "qualot_berry"),
    ("hondew_berry", "hondew_berry"),
    ("grepa_berry", "grepa_berry"),
    ("tamato_berry", "tamato_berry"),
    ("cornn_berry", "cornn_berry"),
    ("magost_berry", "magost_berry"),
    ("rabuta_berry", "rabuta_berry"),
    ("nomel_berry", "nomel_berry"),
    ("spelon_berry", "spelon_berry"),
    ("pamtre_berry", "pamtre_berry"),
    ("watmel_berry", "watmel_berry"),
    ("durin_berry", "durin_berry"),
    ("belue_berry", "belue_berry"),
    ("liechi_berry", "liechi_berry"),
    ("ganlon_berry", "ganlon_berry"),
    ("salac_berry", "salac_berry"),
    ("petaya_berry", "petaya_berry"),
    ("apicot_berry", "apicot_berry"),
    ("lansat_berry", "lansat_berry"),
    ("starf_berry", "starf_berry"),
    ("enigma_berry", "enigma_berry"),
    ("question_mark", "question_mark"),
    ("question_mark", "question_mark"),
    ("question_mark", "question_mark"),
    ("bright_powder", "bright_powder"),
    ("in_battle_herb", "white_herb"),
    ("macho_brace", "macho_brace"),
    ("exp_share", "exp_share"),
    ("quick_claw", "quick_claw"),
    ("soothe_bell", "soothe_bell"),
    ("in_battle_herb", "mental_herb"),
    ("choice_band", "choice_band"),
    ("kings_rock", "kings_rock"),
    ("silver_powder", "silver_powder"),
    ("amulet_coin", "amulet_coin"),
    ("cleanse_tag", "cleanse_tag"),
    ("soul_dew", "soul_dew"),
    ("deep_sea_tooth", "deep_sea_tooth"),
    ("deep_sea_scale", "deep_sea_scale"),
    ("smoke_ball", "smoke_ball"),
    ("everstone", "everstone"),
    ("focus_band", "focus_band"),
    ("lucky_egg", "lucky_egg"),
    ("scope_lens", "scope_lens"),
    ("metal_coat", "metal_coat"),
    ("leftovers", "leftovers"),
    ("dragon_scale", "dragon_scale"),
    ("light_ball", "light_ball"),
    ("soft_sand", "soft_sand"),
    ("hard_stone", "hard_stone"),
    ("miracle_seed", "miracle_seed"),
    ("black_glasses", "black_type_enhancing_item"),
    ("black_belt", "black_type_enhancing_item"),
    ("magnet", "magnet"),
    ("mystic_water", "mystic_water"),
    ("sharp_beak", "sharp_beak"),
    ("poison_barb", "poison_barb"),
    ("never_melt_ice", "never_melt_ice"),
    ("spell_tag", "spell_tag"),
    ("twisted_spoon", "twisted_spoon"),
    ("charcoal", "charcoal"),
    ("dragon_fang", "dragon_fang"),
    ("silk_scarf", "silk_scarf"),
    ("up_grade", "up_grade"),
    ("shell_bell", "shell"),
    ("sea_incense", "sea_incense"),
    ("lax_incense", "lax_incense"),
    ("lucky_punch", "lucky_punch"),
    ("metal_powder", "metal_powder"),
    ("thick_club", "thick_club"),
    ("stick", "stick"),
    ("question_mark", "question_mark"),
    ("question_mark", "question_mark"),
    ("question_mark", "question_mark"),
    ("question_mark", "question_mark"),
    ("question_mark", "question_mark"),
    ("question_mark", "question_mark"),
    ("question_mark", "question_mark"),
    ("question_mark", "question_mark"),
    ("question_mark", "question_mark"),
    ("question_mark", "question_mark"),
    ("question_mark", "question_mark"),
    ("question_mark", "question_mark"),
    ("question_mark", "question_mark"),
    ("question_mark", "question_mark"),
    ("question_mark", "question_mark"),
    ("question_mark", "question_mark"),
    ("question_mark", "question_mark"),
    ("question_mark", "question_mark"),
    ("question_mark", "question_mark"),
    ("question_mark", "question_mark"),
    ("question_mark", "question_mark"),
    ("question_mark", "question_mark"),
    ("question_mark", "question_mark"),
    ("question_mark", "question_mark"),
    ("question_mark", "question_mark"),
    ("question_mark", "question_mark"),
    ("question_mark", "question_mark"),
    ("question_mark", "question_mark"),
    ("scarf", "red_scarf"),
    ("scarf", "blue_scarf"),
    ("scarf", "pink_scarf"),
    ("scarf", "green_scarf"),
    ("scarf", "yellow_scarf"),
    ("mach_bike", "mach_bike"),
    ("coin_case", "coin_case"),
    ("itemfinder", "itemfinder"),
    ("old_rod", "old_rod"),
    ("good_rod", "good_rod"),
    ("super_rod", "super_rod"),
    ("ss_ticket", "ss_ticket"),
    ("contest_pass", "contest_pass"),
    ("question_mark", "question_mark"),
    ("wailmer_pail", "wailmer_pail"),
    ("devon_goods", "devon_goods"),
    ("soot_sack", "soot_sack"),
    ("basement_key", "old_key"),
    ("acro_bike", "acro_bike"),
    ("pokeblock_case", "pokeblock_case"),
    ("letter", "lava_cookie_and_letter"),
    ("eon_ticket", "eon_ticket"),
    ("orb", "red_orb"),
    ("orb", "blue_orb"),
    ("scanner", "scanner"),
    ("go_goggles", "go_goggles"),
    ("meteorite", "meteorite"),
    ("room1_key", "key"),
    ("room2_key", "key"),
    ("room4_key", "key"),
    ("room6_key", "key"),
    ("storage_key", "old_key"),
    ("root_fossil", "hoenn_fossil"),
    ("claw_fossil", "hoenn_fossil"),
    ("devon_scope", "devon_scope"),
    ("tm", "fighting_tm_hm"),
    ("tm", "dragon_tm_hm"),
    ("tm", "water_tm_hm"),
    ("tm", "psychic_tm_hm"),
    ("tm", "normal_tm_hm"),
    ("tm", "poison_tm_hm"),
    ("tm", "ice_tm_hm"),
    ("tm", "fighting_tm_hm"),
    ("tm", "grass_tm_hm"),
    ("tm", "normal_tm_hm"),
    ("tm", "fire_tm_hm"),
    ("tm", "dark_tm_hm"),
    ("tm", "ice_tm_hm"),
    ("tm", "ice_tm_hm"),
    ("tm", "normal_tm_hm"),
    ("tm", "psychic_tm_hm"),
    ("tm", "normal_tm_hm"),
    ("tm", "water_tm_hm"),
    ("tm", "grass_tm_hm"),
    ("tm", "normal_tm_hm"),
    ("tm", "normal_tm_hm"),
    ("tm", "grass_tm_hm"),
    ("tm", "steel_tm_hm"),
    ("tm", "electric_tm_hm"),
    ("tm", "electric_tm_hm"),
    ("tm", "ground_tm_hm"),
    ("tm", "normal_tm_hm"),
    ("tm", "ground_tm_hm"),
    ("tm", "psychic_tm_hm"),
    ("tm", "ghost_tm_hm"),
    ("tm", "fighting_tm_hm"),
    ("tm", "normal_tm_hm"),
    ("tm", "psychic_tm_hm"),
    ("tm", "electric_tm_hm"),
    ("tm", "fire_tm_hm"),
    ("tm", "poison_tm_hm"),
    ("tm", "rock_tm_hm"),
    ("tm", "fire_tm_hm"),
    ("tm", "rock_tm_hm"),
    ("tm", "flying_tm_hm"),
    ("tm", "dark_tm_hm"),
    ("tm", "normal_tm_hm"),
    ("tm", "normal_tm_hm"),
    ("tm", "psychic_tm_hm"),
    ("tm", "normal_tm_hm"),
    ("tm", "dark_tm_hm"),
    ("tm", "steel_tm_hm"),
    ("tm", "psychic_tm_hm"),
    ("tm", "dark_tm_hm"),
    ("tm", "fire_tm_hm"),
    ("hm", "normal_tm_hm"),
    ("hm", "flying_tm_hm"),
    ("hm", "water_tm_hm"),
    ("hm", "normal_tm_hm"),
    ("hm", "normal_tm_hm"),
    ("hm", "fighting_tm_hm"),
    ("hm", "water_tm_hm"),
    ("hm", "water_tm_hm"),
    ("question_mark", "question_mark"),
    ("question_mark", "question_mark"),
    ("oaks_parcel", "oaks_parcel"),
    ("poke_flute", "poke_flute"),
    ("secret_key", "secret_key"),
    ("bike_voucher", "bike_voucher"),
    ("gold_teeth", "gold_teeth"),
    ("old_amber", "old_amber"),
    ("card_key", "card_key"),
    ("lift_key", "key"),
    ("helix_fossil", "kanto_fossil"),
    ("dome_fossil", "kanto_fossil"),
    ("silph_scope", "silph_scope"),
    ("bicycle", "bicycle"),
    ("town_map", "town_map"),
    ("vs_seeker", "vs_seeker"),
    ("fame_checker", "fame_checker"),
    ("tm_case", "tm_case"),
    ("berry_pouch", "berry_pouch"),
    ("teachy_tv", "teachy_tv"),
    ("tri_pass", "tri_pass"),
    ("rainbow_pass", "rainbow_pass"),
    ("tea", "tea"),
    ("mystic_ticket", "mystic_ticket"),
    ("aurora_ticket", "aurora_ticket"),
    ("powder_jar", "powder_jar"),
    ("gem", "ruby"),
    ("gem", "sapphire"),
    ("magma_emblem", "magma_emblem"),
    ("old_sea_map", "old_sea_map"),
    ("return_to_field_arrow", "return_to_field_arrow"),
];

/// A rectangle of the menu-info sheet, in pixels from its top-left corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MenuInfoIcon {
    /// Left edge.
    pub x: u32,
    /// Top edge.
    pub y: u32,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

impl MenuInfoIcon {
    fn at_tile(offset: u16, width: u32) -> Self {
        let tile = u32::from(offset);
        Self {
            x: tile % MENU_INFO_TILES_PER_ROW * 8,
            y: tile / MENU_INFO_TILES_PER_ROW * 8,
            width,
            height: MENU_INFO_ICON_HEIGHT,
        }
    }
}

/// A label icon of the TM/HM move-data window, in `sMenuInfoIcons` order
/// (`MENU_INFO_ICON_TYPE` through `MENU_INFO_ICON_PP`, `include/menu.h:17-20`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuInfoLabel {
    /// "TYPE" (`sMenuInfoIcons` tile offset 0xA8).
    Type,
    /// "POWER" (tile offset 0xC0).
    Power,
    /// "ACCURACY" (tile offset 0xC8).
    Accuracy,
    /// "PP" (tile offset 0xE0).
    Pp,
}

impl MenuInfoLabel {
    /// Every label, in `sMenuInfoIcons` order.
    pub const ALL: [Self; 4] = [Self::Type, Self::Power, Self::Accuracy, Self::Pp];

    const fn tile_offset(self) -> u16 {
        match self {
            Self::Type => 0xA8,
            Self::Power => 0xC0,
            Self::Accuracy => 0xC8,
            Self::Pp => 0xE0,
        }
    }
}

/// The menu-info icon sheet (`gMenuInfoElements_Gfx`) with its three palettes
/// (`gMenuInfoElements{1,2,3}_Pal`), as `BlitMenuInfoIcon` reads them
/// (`src/menu.c:2098-2101`): a 128x128 sheet holding the 18 type icons (32x12)
/// and the TYPE, POWER, ACCURACY and PP labels (42x12).
#[derive(Debug, Clone, Copy)]
pub struct MenuInfoAssets<'a> {
    /// The 128x128 sheet: 4bpp palette indices (`.4bpp` keeps each PNG index modulo 16).
    pub sheet: ImageRef<'a>,
    /// `ListMenuLoadStdPalAt` palettes 0, 1 and 2: 16 colours each.
    pub palettes: [PaletteRef<'a>; MENU_INFO_PALETTES],
}

impl<'a> MenuInfoAssets<'a> {
    /// The 32x12 icon for combat type id `type_id` (`gBattleMoves[].type`,
    /// 0 for `TYPE_NORMAL` through 17 for `TYPE_DARK`, 9 being `TYPE_MYSTERY`),
    /// or `None` for an id outside the 18 upstream types.
    #[must_use]
    pub fn type_icon(&self, type_id: u8) -> Option<MenuInfoIcon> {
        let offset = *TYPE_ICON_OFFSETS.get(usize::from(type_id))?;
        Some(MenuInfoIcon::at_tile(offset, TYPE_ICON_WIDTH))
    }

    /// The 42x12 icon for `label`.
    #[must_use]
    pub fn label_icon(&self, label: MenuInfoLabel) -> MenuInfoIcon {
        MenuInfoIcon::at_tile(label.tile_offset(), LABEL_ICON_WIDTH)
    }

    /// Row `row` of `icon`'s pixels: `icon.width` palette indices from the
    /// sheet, or `None` when `row` is past the icon's height or any part of the
    /// icon lies outside the sheet.
    #[must_use]
    pub fn icon_row(&self, icon: MenuInfoIcon, row: u32) -> Option<&'a [u8]> {
        let right = icon.x.checked_add(icon.width)?;
        let bottom = icon.y.checked_add(icon.height)?;
        if row >= icon.height || right > self.sheet.width || bottom > self.sheet.height {
            return None;
        }
        // `icon.y + row < bottom`, which fits in `u32`.
        let start =
            (u64::from(icon.y) + u64::from(row)) * u64::from(self.sheet.width) + u64::from(icon.x);
        let start = usize::try_from(start).ok()?;
        self.sheet
            .pixels
            .get(start..start.checked_add(usize::try_from(icon.width).ok()?)?)
    }
}

/// Every ordinary-bag graphic. Fixed entries are validated by
/// [`AssetPack::bag_assets`]; icons by [`item_icon`](Self::item_icon); the
/// list-scroll arrows and the swap line, which the screen draws only on demand,
/// by [`scroll_indicator`](Self::scroll_indicator) and
/// [`swap_line`](Self::swap_line); the TM/HM move-data window's icon sheet by
/// [`menu_info`](Self::menu_info).
#[derive(Debug, Clone, Copy)]
pub struct BagAssets<'a> {
    /// Bag screen tile sheet (`menu.png`), 4bpp bank-local indices.
    pub screen: ImageRef<'a>,
    /// Bag screen tilemap (`menu.bin`): 32x32 two-byte little-endian entries.
    pub screen_tilemap: &'a [u8],
    /// Bag screen palette for the male player: two 16-colour banks.
    pub screen_male_palette: PaletteRef<'a>,
    /// Bag screen palette for the female player: two 16-colour banks.
    pub screen_female_palette: PaletteRef<'a>,
    /// Male bag animation sheet with the shared bag palette.
    pub male: PalettedImage<'a>,
    /// Female bag animation sheet with the shared bag palette.
    pub female: PalettedImage<'a>,
    /// Rotating Poké Ball with its own palette.
    pub rotating_ball: PalettedImage<'a>,
    /// Registered-select mark; indexes the item-list window's palette, bank 1 of the screen palette.
    pub registered_select: ImageRef<'a>,
    /// HM mark; indexes the item-list window's palette, bank 1 of the screen palette.
    pub hm: ImageRef<'a>,
    pack: &'a AssetPack,
}

impl AssetPack {
    /// Bundle every fixed ordinary-bag graphic.
    ///
    /// # Errors
    ///
    /// [`PackError::UnknownAsset`] or [`PackError::WrongKind`] for a missing or
    /// wrong-kind entry; [`PackError::MalformedBagAsset`] when dimensions,
    /// bit depth, palette size, pixel indices, or tilemap length differ from
    /// upstream.
    pub fn bag_assets(&self) -> Result<BagAssets<'_>, PackError> {
        let bank = BANK_PALETTE_COLORS;
        let bag = (BAG_WIDTH, BAG_HEIGHT);
        let ball = (ROTATING_BALL_SIDE, ROTATING_BALL_SIDE);
        Ok(BagAssets {
            screen: self.bag_sheet(MENU_IMAGE_ID, (MENU_WIDTH, MENU_HEIGHT))?,
            screen_tilemap: self.bag_tilemap()?,
            screen_male_palette: self.bag_palette(MENU_MALE_PALETTE_ID, MENU_PALETTE_COLORS)?,
            screen_female_palette: self.bag_palette(MENU_FEMALE_PALETTE_ID, MENU_PALETTE_COLORS)?,
            male: self.bag_paletted(MALE_IMAGE_ID, BAG_PALETTE_ID, bag, bank)?,
            female: self.bag_paletted(FEMALE_IMAGE_ID, BAG_PALETTE_ID, bag, bank)?,
            rotating_ball: self.bag_paletted(
                ROTATING_BALL_IMAGE_ID,
                ROTATING_BALL_PALETTE_ID,
                ball,
                bank,
            )?,
            registered_select: self.bag_sheet(
                SELECT_BUTTON_IMAGE_ID,
                (SELECT_BUTTON_WIDTH, SELECT_BUTTON_HEIGHT),
            )?,
            hm: self.bag_sheet(HM_IMAGE_ID, (HM_SIDE, HM_SIDE))?,
            pack: self,
        })
    }

    fn bag_tilemap(&self) -> Result<&[u8], PackError> {
        let bytes = self.raw(MENU_TILEMAP_ID)?;
        if bytes.len() != TILEMAP_BYTES {
            return Err(malformed(
                MENU_TILEMAP_ID,
                format!("is {} bytes: expected {TILEMAP_BYTES}", bytes.len()),
            ));
        }
        Ok(bytes)
    }

    fn bag_sheet(&self, id: &str, (width, height): (u32, u32)) -> Result<ImageRef<'_>, PackError> {
        let image = self.image(id)?;
        if image.width != width || image.height != height {
            return Err(malformed(
                id,
                format!(
                    "is {}x{}: expected {width}x{height}",
                    image.width, image.height
                ),
            ));
        }
        if image.bit_depth != SOURCE_BIT_DEPTH {
            return Err(malformed(
                id,
                format!(
                    "has bit depth {}: expected {SOURCE_BIT_DEPTH}",
                    image.bit_depth
                ),
            ));
        }
        if u64::try_from(image.pixels.len()).ok() != Some(u64::from(width) * u64::from(height)) {
            return Err(malformed(
                id,
                format!(
                    "carries {} payload bytes: expected one per pixel",
                    image.pixels.len()
                ),
            ));
        }
        if let Some(&pixel) = image
            .pixels
            .iter()
            .find(|&&pixel| u16::from(pixel) >= BANK_PALETTE_COLORS)
        {
            return Err(malformed(
                id,
                format!("has pixel index {pixel}: a 4bpp index is below {BANK_PALETTE_COLORS}"),
            ));
        }
        Ok(image)
    }

    fn bag_palette(&self, id: &str, colors: u16) -> Result<PaletteRef<'_>, PackError> {
        let palette = self.palette(id)?;
        if palette.color_count != colors
            || palette.raw.len() != usize::from(colors) * PALETTE_ENTRY_BYTES
        {
            return Err(malformed(
                id,
                format!(
                    "declares {} colours in {} bytes: expected {colors} colours",
                    palette.color_count,
                    palette.raw.len()
                ),
            ));
        }
        Ok(palette)
    }

    fn bag_paletted(
        &self,
        image_id: &str,
        palette_id: &str,
        size: (u32, u32),
        colors: u16,
    ) -> Result<PalettedImage<'_>, PackError> {
        self.paired_image(
            image_id,
            palette_id,
            size,
            SOURCE_BIT_DEPTH,
            colors,
            |id, detail| PackError::MalformedBagAsset { id, detail },
        )
    }
}

impl<'a> BagAssets<'a> {
    /// The icon a bag row shows for `item`, per upstream
    /// `GetItemIconPicOrPalette`: the list-end id selects the "return to field"
    /// arrow and any other id outside the item table selects [`ItemId::NONE`].
    ///
    /// # Errors
    ///
    /// [`PackError::UnknownAsset`], [`PackError::WrongKind`], or
    /// [`PackError::MalformedBagAsset`] when the icon's entries are missing,
    /// of the wrong kind, or not a 24x24 image with a 16-colour palette.
    pub fn item_icon(&self, item: ItemId) -> Result<PalettedImage<'a>, PackError> {
        let row = if item == LIST_END {
            ITEMS_COUNT
        } else if usize::from(item.index()) >= ITEMS_COUNT {
            usize::from(ItemId::NONE.index())
        } else {
            usize::from(item.index())
        };
        let (image_stem, palette_stem) = ICON_STEMS[row];
        self.pack.bag_paletted(
            &format!("{ICON_IMAGE_PREFIX}{image_stem}"),
            &format!("{ICON_PALETTE_PREFIX}{palette_stem}"),
            (ICON_SIDE, ICON_SIDE),
            BANK_PALETTE_COLORS,
        )
    }

    /// The list scroll-arrow sheet (`graphics/interface/scroll_indicator.png`)
    /// with its `red.pal` palette, as `AddScrollIndicatorArrowPair` loads them
    /// (`src/list_menu.c:289-290`): 16x32, two 16x16 frames, the up arrow at
    /// tile offset 4 and the down arrow the same tiles flipped vertically.
    ///
    /// # Errors
    ///
    /// [`PackError::UnknownAsset`], [`PackError::WrongKind`], or
    /// [`PackError::MalformedBagAsset`] when the sheet or palette is missing,
    /// of the wrong kind, or not a 16x32 image with a 16-colour palette.
    pub fn scroll_indicator(&self) -> Result<PalettedImage<'a>, PackError> {
        self.pack.bag_paletted(
            SCROLL_INDICATOR_IMAGE_ID,
            SCROLL_INDICATOR_PALETTE_ID,
            (SCROLL_INDICATOR_WIDTH, SCROLL_INDICATOR_HEIGHT),
            BANK_PALETTE_COLORS,
        )
    }

    /// The SELECT swap-line sprite sheet (`gSwapLineGfx`) with its palette
    /// (`gSwapLinePal`), the PNG's own (`src/graphics.c:1151-1152`): 16x32.
    ///
    /// # Errors
    ///
    /// Same as [`scroll_indicator`](Self::scroll_indicator).
    pub fn swap_line(&self) -> Result<PalettedImage<'a>, PackError> {
        self.pack.bag_paletted(
            SWAP_LINE_IMAGE_ID,
            SWAP_LINE_PALETTE_ID,
            (SWAP_LINE_WIDTH, SWAP_LINE_HEIGHT),
            BANK_PALETTE_COLORS,
        )
    }

    /// The menu-info icon sheet (`graphics/interface/menu_info.png`) with its
    /// three palettes (`menu_info1.pal` through `menu_info3.pal`), the sources
    /// of the type and label icons `BlitMenuInfoIcon` blits into the TM/HM
    /// move-data window (`src/graphics.c:1264-1267`, `src/menu.c:2098`).
    ///
    /// # Errors
    ///
    /// [`PackError::UnknownAsset`], [`PackError::WrongKind`], or
    /// [`PackError::MalformedBagAsset`] when the sheet or a palette is missing,
    /// of the wrong kind, or not a 128x128 4bpp image with three
    /// 16-colour palettes.
    pub fn menu_info(&self) -> Result<MenuInfoAssets<'a>, PackError> {
        let sheet = self
            .pack
            .bag_sheet(MENU_INFO_IMAGE_ID, (MENU_INFO_SIDE, MENU_INFO_SIDE))?;
        let [first, second, third] = MENU_INFO_PALETTE_IDS;
        let palette = |id| self.pack.bag_palette(id, BANK_PALETTE_COLORS);
        Ok(MenuInfoAssets {
            sheet,
            palettes: [palette(first)?, palette(second)?, palette(third)?],
        })
    }

    /// The "return to field" arrow icon that ends an item list.
    ///
    /// # Errors
    ///
    /// Same as [`item_icon`](Self::item_icon).
    pub fn list_end_icon(&self) -> Result<PalettedImage<'a>, PackError> {
        self.item_icon(LIST_END)
    }
}

fn malformed(id: &str, detail: String) -> PackError {
    PackError::MalformedBagAsset {
        id: id.to_owned(),
        detail,
    }
}

#[cfg(test)]
pub(super) fn icon_stems() -> &'static [(&'static str, &'static str)] {
    &ICON_STEMS
}
