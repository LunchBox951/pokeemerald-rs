//! Canonical berry descriptors: names, firmness, size, yield limits, the two
//! description lines, stage duration, flavor values, and smoothness.
//!
//! This is the default `gBerries` table (`pokeemerald/src/berry.c:115-890`)
//! with its description strings (`:28-113`). Upstream `GetBerryInfo`
//! (`pokeemerald/src/berry.c:980-991`) first consults the save-provided Enigma
//! Berry override and only then this table; the override is mutable save state
//! and is deliberately not modelled here, so [`BerryTable`] only ever answers
//! with the canonical rows, including the canonical `ENIGMA` row.
//!
//! The table is indexed like upstream: row `i` describes item
//! `FIRST_BERRY_INDEX + i` (`ITEM_CHERI_BERRY` through `ITEM_ENIGMA_BERRY`,
//! `pokeemerald/include/constants/items.h:155-200`), and the *berry type*
//! `ITEM_TO_BERRY(item) = item - FIRST_BERRY_INDEX + 1`
//! (`pokeemerald/include/constants/items.h:444`) is that row plus one, with
//! `BERRY_NONE = 0` (`pokeemerald/include/constants/berry.h:4`).

use crate::items::ItemId;

/// Number of canonical berry descriptors, `ITEM_CHERI_BERRY..=ITEM_ENIGMA_BERRY`.
pub const BERRIES_COUNT: usize = 43;

/// First berry item, `FIRST_BERRY_INDEX` (`ITEM_CHERI_BERRY`).
pub const FIRST_BERRY_ITEM: ItemId = ItemId::CHERI_BERRY;

/// Last berry item, `LAST_BERRY_INDEX` (`ITEM_ENIGMA_BERRY`).
pub const LAST_BERRY_ITEM: ItemId = ItemId::ENIGMA_BERRY;

/// How firm a berry is (`BERRY_FIRMNESS_*`,
/// `pokeemerald/include/constants/berry.h:6-11`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum BerryFirmness {
    /// `BERRY_FIRMNESS_UNKNOWN`; only the Enigma Berry carries it.
    Unknown = 0,
    /// `BERRY_FIRMNESS_VERY_SOFT`.
    VerySoft = 1,
    /// `BERRY_FIRMNESS_SOFT`.
    Soft = 2,
    /// `BERRY_FIRMNESS_HARD`.
    Hard = 3,
    /// `BERRY_FIRMNESS_VERY_HARD`.
    VeryHard = 4,
    /// `BERRY_FIRMNESS_SUPER_HARD`.
    SuperHard = 5,
}

impl BerryFirmness {
    /// Returns the upstream `BERRY_FIRMNESS_*` value.
    #[must_use]
    pub const fn id(self) -> u8 {
        self as u8
    }
}

/// The number of flavors (`FLAVOR_COUNT`, `pokeemerald/include/constants/berry.h:18`).
pub const FLAVOR_COUNT: usize = 5;

/// The five flavor values of a berry, in upstream `FLAVOR_*` order: spicy, dry,
/// sweet, bitter, sour (`pokeemerald/include/constants/berry.h:13-17`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BerryFlavors {
    /// `.spicy` (`FLAVOR_SPICY`, 0).
    pub spicy: u8,
    /// `.dry` (`FLAVOR_DRY`, 1).
    pub dry: u8,
    /// `.sweet` (`FLAVOR_SWEET`, 2).
    pub sweet: u8,
    /// `.bitter` (`FLAVOR_BITTER`, 3).
    pub bitter: u8,
    /// `.sour` (`FLAVOR_SOUR`, 4).
    pub sour: u8,
}

impl BerryFlavors {
    /// Returns the values indexed by `FLAVOR_*`: spicy, dry, sweet, bitter, sour.
    #[must_use]
    pub const fn as_array(self) -> [u8; FLAVOR_COUNT] {
        [self.spicy, self.dry, self.sweet, self.bitter, self.sour]
    }
}

/// One canonical `struct Berry` row (`pokeemerald/include/global.berry.h:7-23`).
///
/// Rows are only obtainable from [`BerryTable`]: the fields are private so a
/// caller cannot build or edit a row whose `item` falls outside
/// `ITEM_CHERI_BERRY..=ITEM_ENIGMA_BERRY`, which [`Self::berry_type`] relies on.
///
/// ```compile_fail
/// use assets::berries::BerryTable;
/// use assets::items::ItemId;
///
/// let mut row = *BerryTable::new().by_type(1);
/// row.item = ItemId::NONE; // private field
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BerryData {
    item: ItemId,
    name: &'static str,
    firmness: BerryFirmness,
    size: u16,
    max_yield: u8,
    min_yield: u8,
    description1: &'static str,
    description2: &'static str,
    stage_duration_hours: u8,
    flavors: BerryFlavors,
    smoothness: u8,
}

impl BerryData {
    /// Returns the berry's bag item (`FIRST_BERRY_INDEX` + row).
    #[must_use]
    pub const fn item(&self) -> ItemId {
        self.item
    }

    /// Returns the display name, at most six characters (`BERRY_NAME_LENGTH`),
    /// without the ` BERRY` suffix.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// Returns how firm the berry is.
    #[must_use]
    pub const fn firmness(&self) -> BerryFirmness {
        self.firmness
    }

    /// Returns the size in millimeters (`.size`, as shown on the berry tag
    /// screen).
    #[must_use]
    pub const fn size(&self) -> u16 {
        self.size
    }

    /// Returns the largest number of berries one tree can yield.
    #[must_use]
    pub const fn max_yield(&self) -> u8 {
        self.max_yield
    }

    /// Returns the smallest number of berries one tree yields.
    #[must_use]
    pub const fn min_yield(&self) -> u8 {
        self.min_yield
    }

    /// Returns the first description line, in charmap notation.
    #[must_use]
    pub const fn description1(&self) -> &'static str {
        self.description1
    }

    /// Returns the second description line, in charmap notation.
    #[must_use]
    pub const fn description2(&self) -> &'static str {
        self.description2
    }

    /// Returns the hours each growth stage lasts (`.stageDuration`; upstream's
    /// `GetStageDurationByBerryType` multiplies it by 60 to get minutes,
    /// `pokeemerald/src/berry.c:1250-1253`).
    #[must_use]
    pub const fn stage_duration_hours(&self) -> u8 {
        self.stage_duration_hours
    }

    /// Returns the five flavor values.
    #[must_use]
    pub const fn flavors(&self) -> BerryFlavors {
        self.flavors
    }

    /// Returns the smoothness, used when blending.
    #[must_use]
    pub const fn smoothness(&self) -> u8 {
        self.smoothness
    }

    /// Returns the berry type: the `ITEM_TO_BERRY` index, one-based.
    #[must_use]
    pub const fn berry_type(&self) -> u8 {
        item_to_berry_type(self.item)
    }

    /// Returns the stage duration in minutes, as upstream
    /// `GetStageDurationByBerryType` does.
    #[must_use]
    pub const fn stage_duration_minutes(&self) -> u16 {
        self.stage_duration_hours as u16 * 60
    }
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "callers pass an item inside the 43-row berry range, so the type is at most 43"
)]
const fn item_to_berry_type(item: ItemId) -> u8 {
    (item.index() - FIRST_BERRY_ITEM.index() + 1) as u8
}

#[expect(
    clippy::too_many_arguments,
    reason = "one argument per upstream struct Berry field keeps each table row on one line"
)]
const fn berry(
    item: ItemId,
    name: &'static str,
    firmness: BerryFirmness,
    size: u16,
    max_yield: u8,
    min_yield: u8,
    description1: &'static str,
    description2: &'static str,
    stage_duration_hours: u8,
    flavors: [u8; FLAVOR_COUNT],
    smoothness: u8,
) -> BerryData {
    BerryData {
        item,
        name,
        firmness,
        size,
        max_yield,
        min_yield,
        description1,
        description2,
        stage_duration_hours,
        flavors: BerryFlavors {
            spicy: flavors[0],
            dry: flavors[1],
            sweet: flavors[2],
            bitter: flavors[3],
            sour: flavors[4],
        },
        smoothness,
    }
}

macro_rules! define_berries {
    ($($item:ident => berry($name:literal, $firmness:ident, $($rest:tt)+)),+ $(,)?) => {
        const BERRIES: [BerryData; BERRIES_COUNT] = [
            $(berry(
                ItemId::$item,
                $name,
                BerryFirmness::$firmness,
                $($rest)+
            ),)+
        ];

        const _: () = {
            let items = [$(ItemId::$item),+];
            let mut row = 0;
            while row < BERRIES_COUNT {
                assert!(
                    items[row].index() as usize == FIRST_BERRY_ITEM.index() as usize + row
                );
                row += 1;
            }
        };
    };
}

/// Indexed access to the canonical berry descriptor table.
#[derive(Debug, Clone, Copy)]
pub struct BerryTable {
    berries: &'static [BerryData; BERRIES_COUNT],
}

impl BerryTable {
    /// Creates a berry-table handle.
    #[must_use]
    pub const fn new() -> Self {
        Self { berries: &BERRIES }
    }

    /// Returns the number of berry descriptors.
    #[must_use]
    pub const fn len(&self) -> usize {
        BERRIES_COUNT
    }

    /// Returns `false`; the canonical berry table is never empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        false
    }

    /// Returns the descriptor for `item`, or `None` when `item` is not one of
    /// `ITEM_CHERI_BERRY..=ITEM_ENIGMA_BERRY`.
    ///
    /// Unlike upstream `GetBerryInfo`, this rejects non-berries instead of
    /// substituting the first berry; use [`Self::by_type`] for that fallback.
    #[must_use]
    pub fn get(&self, item: ItemId) -> Option<&BerryData> {
        let row = item.index().checked_sub(FIRST_BERRY_ITEM.index())?;
        self.berries.get(usize::from(row))
    }

    /// Returns the descriptor for a one-based berry type, mirroring the
    /// canonical-table branch of upstream `GetBerryInfo`
    /// (`pokeemerald/src/berry.c:986-990`): `BERRY_NONE` (0) and any type past
    /// the Enigma Berry (43) fall back to the first berry, Cheri.
    #[must_use]
    pub fn by_type(&self, berry_type: u8) -> &BerryData {
        let row = match usize::from(berry_type) {
            0 => 0,
            n if n > BERRIES_COUNT => 0,
            n => n - 1,
        };
        &self.berries[row]
    }

    /// Returns the berry name for a one-based berry type with the
    /// [`Self::by_type`] fallback, as `GetBerryNameByBerryType` copies it
    /// (`pokeemerald/src/berry.c:1171-1174`).
    #[must_use]
    pub fn name_by_type(&self, berry_type: u8) -> &'static str {
        self.by_type(berry_type).name
    }

    /// Iterates in ascending berry-type order.
    pub fn iter(&self) -> impl Iterator<Item = &BerryData> {
        self.berries.iter()
    }
}

impl Default for BerryTable {
    fn default() -> Self {
        Self::new()
    }
}

#[rustfmt::skip]
define_berries! {
    CHERI_BERRY => berry("CHERI", Soft, 20, 3, 2, "Blooms with delicate pretty flowers.", "The bright red BERRY is very spicy.", 3, [10, 0, 0, 0, 0], 25),
    CHESTO_BERRY => berry("CHESTO", SuperHard, 80, 3, 2, "The BERRY's thick skin and fruit are", "very tough. It is dry-tasting all over.", 3, [0, 10, 0, 0, 0], 25),
    PECHA_BERRY => berry("PECHA", VerySoft, 40, 3, 2, "Very sweet and delicious.", "Also very tender - handle with care.", 3, [0, 0, 10, 0, 0], 25),
    RAWST_BERRY => berry("RAWST", Hard, 32, 3, 2, "If the leaves grow long and curly,", "the BERRY seems to grow very bitter.", 3, [0, 0, 0, 10, 0], 25),
    ASPEAR_BERRY => berry("ASPEAR", SuperHard, 50, 3, 2, "The hard BERRY is dense with a rich", "juice. It is quite sour.", 3, [0, 0, 0, 0, 10], 25),
    LEPPA_BERRY => berry("LEPPA", VeryHard, 28, 3, 2, "Grows slower than CHERI and others.", "The smaller the BERRY, the tastier.", 4, [10, 0, 10, 10, 10], 20),
    ORAN_BERRY => berry("ORAN", SuperHard, 35, 3, 2, "A peculiar BERRY with a mix of flavors.", "BERRIES grow in half a day.", 3, [10, 10, 10, 10, 10], 20),
    PERSIM_BERRY => berry("PERSIM", Hard, 47, 3, 2, "Loves sunlight. The BERRY's color", "grows vivid when exposed to the sun.", 3, [10, 10, 10, 10, 10], 20),
    LUM_BERRY => berry("LUM", SuperHard, 34, 2, 1, "Slow to grow. If raised with loving", "care, it may grow two BERRIES.", 12, [10, 10, 10, 10, 10], 20),
    SITRUS_BERRY => berry("SITRUS", VeryHard, 95, 3, 2, "Closely related to ORAN. The large", "BERRY has a well-rounded flavor.", 6, [10, 10, 10, 10, 10], 20),
    FIGY_BERRY => berry("FIGY", Soft, 100, 3, 2, "The BERRY, which looks chewed up,", "brims with spicy substances.", 6, [10, 0, 0, 0, 0], 25),
    WIKI_BERRY => berry("WIKI", Hard, 115, 3, 2, "The BERRY is said to have grown lumpy", "to help POKéMON grip it.", 6, [0, 10, 0, 0, 0], 25),
    MAGO_BERRY => berry("MAGO", Hard, 126, 3, 2, "The BERRY turns curvy as it grows.", "The curvier, the sweeter and tastier.", 6, [0, 0, 10, 0, 0], 25),
    AGUAV_BERRY => berry("AGUAV", SuperHard, 64, 3, 2, "The flower is dainty. It is rare in its", "ability to grow without light.", 6, [0, 0, 0, 10, 0], 25),
    IAPAPA_BERRY => berry("IAPAPA", Soft, 223, 3, 2, "The BERRY is very big and sour.", "It takes at least a day to grow.", 6, [0, 0, 0, 0, 10], 25),
    RAZZ_BERRY => berry("RAZZ", VeryHard, 120, 6, 3, "The red BERRY tastes slightly spicy.", "It grows quickly in just four hours.", 1, [10, 10, 0, 0, 0], 20),
    BLUK_BERRY => berry("BLUK", Soft, 108, 6, 3, "The BERRY is blue on the outside, but", "it blackens the mouth when eaten.", 1, [0, 10, 10, 0, 0], 20),
    NANAB_BERRY => berry("NANAB", VeryHard, 77, 6, 3, "This BERRY was the seventh", "discovered in the world. It is sweet.", 1, [0, 0, 10, 10, 0], 20),
    WEPEAR_BERRY => berry("WEPEAR", SuperHard, 74, 6, 3, "The flower is small and white. It has a", "delicate balance of bitter and sour.", 1, [0, 0, 0, 10, 10], 20),
    PINAP_BERRY => berry("PINAP", Hard, 80, 6, 3, "Weak against wind and cold.", "The fruit is spicy and the skin, sour.", 1, [10, 0, 0, 0, 10], 20),
    POMEG_BERRY => berry("POMEG", VeryHard, 135, 6, 2, "However much it is watered,", "it only grows up to six BERRIES.", 3, [10, 0, 10, 10, 0], 20),
    KELPSY_BERRY => berry("KELPSY", Hard, 150, 6, 2, "A rare variety shaped like a root.", "Grows a very large flower.", 3, [0, 10, 0, 10, 10], 20),
    QUALOT_BERRY => berry("QUALOT", Hard, 110, 6, 2, "Loves water. Grows strong even in", "locations with constant rainfall.", 3, [10, 0, 10, 0, 10], 20),
    HONDEW_BERRY => berry("HONDEW", Hard, 162, 6, 2, "A BERRY that is very valuable and", "rarely seen. It is very delicious.", 3, [10, 10, 0, 10, 0], 20),
    GREPA_BERRY => berry("GREPA", Soft, 149, 6, 2, "Despite its tenderness and round", "shape, the BERRY is unimaginably sour.", 3, [0, 10, 10, 0, 10], 20),
    TAMATO_BERRY => berry("TAMATO", Soft, 200, 4, 2, "The BERRY is lip-bendingly spicy.", "It takes time to grow.", 6, [20, 10, 0, 0, 0], 30),
    CORNN_BERRY => berry("CORNN", Hard, 75, 4, 2, "A BERRY from an ancient era. May not", "grow unless planted in quantity.", 6, [0, 20, 10, 0, 0], 30),
    MAGOST_BERRY => berry("MAGOST", Hard, 140, 4, 2, "A BERRY that is widely said to have", "a finely balanced flavor.", 6, [0, 0, 20, 10, 0], 30),
    RABUTA_BERRY => berry("RABUTA", Soft, 226, 4, 2, "A rare variety that is overgrown with", "hair. It is quite bitter.", 6, [0, 0, 0, 20, 10], 30),
    NOMEL_BERRY => berry("NOMEL", SuperHard, 285, 4, 2, "Quite sour. Just one bite makes it", "impossible to taste for three days.", 6, [10, 0, 0, 0, 20], 30),
    SPELON_BERRY => berry("SPELON", Soft, 133, 2, 1, "The vividly red BERRY is very spicy.", "Its warts secrete a spicy substance.", 18, [40, 10, 0, 0, 0], 70),
    PAMTRE_BERRY => berry("PAMTRE", VerySoft, 244, 2, 1, "Drifts on the sea from somewhere.", "It is thought to grow elsewhere.", 18, [0, 40, 10, 0, 0], 70),
    WATMEL_BERRY => berry("WATMEL", Soft, 250, 2, 1, "A huge BERRY, with some over 20", "inches discovered. Exceedingly sweet.", 18, [0, 0, 40, 10, 0], 70),
    DURIN_BERRY => berry("DURIN", Hard, 280, 2, 1, "Bitter to even look at. It is so", "bitter, no one has ever eaten it as is.", 18, [0, 0, 0, 40, 10], 70),
    BELUE_BERRY => berry("BELUE", VerySoft, 300, 2, 1, "It is glossy and looks delicious, but", "it is awfully sour. Takes time to grow.", 18, [10, 0, 0, 0, 40], 70),
    LIECHI_BERRY => berry("LIECHI", VeryHard, 111, 2, 1, "A mysterious BERRY. It is rumored to", "contain the power of the sea.", 24, [40, 0, 40, 0, 10], 80),
    GANLON_BERRY => berry("GANLON", VeryHard, 33, 2, 1, "A mysterious BERRY. It is rumored to", "contain the power of the land.", 24, [0, 40, 0, 40, 0], 80),
    SALAC_BERRY => berry("SALAC", VeryHard, 95, 2, 1, "A mysterious BERRY. It is rumored to", "contain the power of the sky.", 24, [0, 0, 40, 0, 40], 80),
    PETAYA_BERRY => berry("PETAYA", VeryHard, 237, 2, 1, "A mysterious BERRY. It is rumored to", "contain the power of all living things.", 24, [40, 0, 0, 40, 0], 80),
    APICOT_BERRY => berry("APICOT", Hard, 75, 2, 1, "A very mystifying BERRY. No telling", "what may happen or how it can be used.", 24, [0, 40, 0, 0, 40], 80),
    LANSAT_BERRY => berry("LANSAT", Soft, 97, 2, 1, "Said to be a legendary BERRY.", "Holding it supposedly brings joy.", 24, [10, 10, 10, 10, 10], 30),
    STARF_BERRY => berry("STARF", SuperHard, 153, 2, 1, "So strong, it was abandoned at the", "world's edge. Considered a mirage.", 24, [10, 10, 10, 10, 10], 30),
    ENIGMA_BERRY => berry("ENIGMA", Unknown, 0, 2, 1, "A completely enigmatic BERRY.", "Appears to have the power of stars.", 24, [40, 40, 40, 40, 40], 40),
}

#[cfg(test)]
mod tests {
    use super::{
        BerryData, BerryFirmness, BerryFlavors, BerryTable, BERRIES, BERRIES_COUNT,
        FIRST_BERRY_ITEM, FLAVOR_COUNT, LAST_BERRY_ITEM,
    };
    use crate::items::{ItemId, ItemTable, Pocket, ITEMS_COUNT};

    /// Independently pinned rows transcribed from `pokeemerald/src/berry.c:28-890`:
    /// name, firmness, size, max yield, min yield, description lines, stage
    /// duration in hours, flavors `[spicy, dry, sweet, bitter, sour]`, smoothness.
    type Row = (
        &'static str,
        BerryFirmness,
        u16,
        u8,
        u8,
        &'static str,
        &'static str,
        u8,
        [u8; 5],
        u8,
    );

    #[rustfmt::skip]
    const EXPECTED: [Row; 43] = [
    ("CHERI", BerryFirmness::Soft, 20, 3, 2, "Blooms with delicate pretty flowers.", "The bright red BERRY is very spicy.", 3, [10, 0, 0, 0, 0], 25),
    ("CHESTO", BerryFirmness::SuperHard, 80, 3, 2, "The BERRY's thick skin and fruit are", "very tough. It is dry-tasting all over.", 3, [0, 10, 0, 0, 0], 25),
    ("PECHA", BerryFirmness::VerySoft, 40, 3, 2, "Very sweet and delicious.", "Also very tender - handle with care.", 3, [0, 0, 10, 0, 0], 25),
    ("RAWST", BerryFirmness::Hard, 32, 3, 2, "If the leaves grow long and curly,", "the BERRY seems to grow very bitter.", 3, [0, 0, 0, 10, 0], 25),
    ("ASPEAR", BerryFirmness::SuperHard, 50, 3, 2, "The hard BERRY is dense with a rich", "juice. It is quite sour.", 3, [0, 0, 0, 0, 10], 25),
    ("LEPPA", BerryFirmness::VeryHard, 28, 3, 2, "Grows slower than CHERI and others.", "The smaller the BERRY, the tastier.", 4, [10, 0, 10, 10, 10], 20),
    ("ORAN", BerryFirmness::SuperHard, 35, 3, 2, "A peculiar BERRY with a mix of flavors.", "BERRIES grow in half a day.", 3, [10, 10, 10, 10, 10], 20),
    ("PERSIM", BerryFirmness::Hard, 47, 3, 2, "Loves sunlight. The BERRY's color", "grows vivid when exposed to the sun.", 3, [10, 10, 10, 10, 10], 20),
    ("LUM", BerryFirmness::SuperHard, 34, 2, 1, "Slow to grow. If raised with loving", "care, it may grow two BERRIES.", 12, [10, 10, 10, 10, 10], 20),
    ("SITRUS", BerryFirmness::VeryHard, 95, 3, 2, "Closely related to ORAN. The large", "BERRY has a well-rounded flavor.", 6, [10, 10, 10, 10, 10], 20),
    ("FIGY", BerryFirmness::Soft, 100, 3, 2, "The BERRY, which looks chewed up,", "brims with spicy substances.", 6, [10, 0, 0, 0, 0], 25),
    ("WIKI", BerryFirmness::Hard, 115, 3, 2, "The BERRY is said to have grown lumpy", "to help POKéMON grip it.", 6, [0, 10, 0, 0, 0], 25),
    ("MAGO", BerryFirmness::Hard, 126, 3, 2, "The BERRY turns curvy as it grows.", "The curvier, the sweeter and tastier.", 6, [0, 0, 10, 0, 0], 25),
    ("AGUAV", BerryFirmness::SuperHard, 64, 3, 2, "The flower is dainty. It is rare in its", "ability to grow without light.", 6, [0, 0, 0, 10, 0], 25),
    ("IAPAPA", BerryFirmness::Soft, 223, 3, 2, "The BERRY is very big and sour.", "It takes at least a day to grow.", 6, [0, 0, 0, 0, 10], 25),
    ("RAZZ", BerryFirmness::VeryHard, 120, 6, 3, "The red BERRY tastes slightly spicy.", "It grows quickly in just four hours.", 1, [10, 10, 0, 0, 0], 20),
    ("BLUK", BerryFirmness::Soft, 108, 6, 3, "The BERRY is blue on the outside, but", "it blackens the mouth when eaten.", 1, [0, 10, 10, 0, 0], 20),
    ("NANAB", BerryFirmness::VeryHard, 77, 6, 3, "This BERRY was the seventh", "discovered in the world. It is sweet.", 1, [0, 0, 10, 10, 0], 20),
    ("WEPEAR", BerryFirmness::SuperHard, 74, 6, 3, "The flower is small and white. It has a", "delicate balance of bitter and sour.", 1, [0, 0, 0, 10, 10], 20),
    ("PINAP", BerryFirmness::Hard, 80, 6, 3, "Weak against wind and cold.", "The fruit is spicy and the skin, sour.", 1, [10, 0, 0, 0, 10], 20),
    ("POMEG", BerryFirmness::VeryHard, 135, 6, 2, "However much it is watered,", "it only grows up to six BERRIES.", 3, [10, 0, 10, 10, 0], 20),
    ("KELPSY", BerryFirmness::Hard, 150, 6, 2, "A rare variety shaped like a root.", "Grows a very large flower.", 3, [0, 10, 0, 10, 10], 20),
    ("QUALOT", BerryFirmness::Hard, 110, 6, 2, "Loves water. Grows strong even in", "locations with constant rainfall.", 3, [10, 0, 10, 0, 10], 20),
    ("HONDEW", BerryFirmness::Hard, 162, 6, 2, "A BERRY that is very valuable and", "rarely seen. It is very delicious.", 3, [10, 10, 0, 10, 0], 20),
    ("GREPA", BerryFirmness::Soft, 149, 6, 2, "Despite its tenderness and round", "shape, the BERRY is unimaginably sour.", 3, [0, 10, 10, 0, 10], 20),
    ("TAMATO", BerryFirmness::Soft, 200, 4, 2, "The BERRY is lip-bendingly spicy.", "It takes time to grow.", 6, [20, 10, 0, 0, 0], 30),
    ("CORNN", BerryFirmness::Hard, 75, 4, 2, "A BERRY from an ancient era. May not", "grow unless planted in quantity.", 6, [0, 20, 10, 0, 0], 30),
    ("MAGOST", BerryFirmness::Hard, 140, 4, 2, "A BERRY that is widely said to have", "a finely balanced flavor.", 6, [0, 0, 20, 10, 0], 30),
    ("RABUTA", BerryFirmness::Soft, 226, 4, 2, "A rare variety that is overgrown with", "hair. It is quite bitter.", 6, [0, 0, 0, 20, 10], 30),
    ("NOMEL", BerryFirmness::SuperHard, 285, 4, 2, "Quite sour. Just one bite makes it", "impossible to taste for three days.", 6, [10, 0, 0, 0, 20], 30),
    ("SPELON", BerryFirmness::Soft, 133, 2, 1, "The vividly red BERRY is very spicy.", "Its warts secrete a spicy substance.", 18, [40, 10, 0, 0, 0], 70),
    ("PAMTRE", BerryFirmness::VerySoft, 244, 2, 1, "Drifts on the sea from somewhere.", "It is thought to grow elsewhere.", 18, [0, 40, 10, 0, 0], 70),
    ("WATMEL", BerryFirmness::Soft, 250, 2, 1, "A huge BERRY, with some over 20", "inches discovered. Exceedingly sweet.", 18, [0, 0, 40, 10, 0], 70),
    ("DURIN", BerryFirmness::Hard, 280, 2, 1, "Bitter to even look at. It is so", "bitter, no one has ever eaten it as is.", 18, [0, 0, 0, 40, 10], 70),
    ("BELUE", BerryFirmness::VerySoft, 300, 2, 1, "It is glossy and looks delicious, but", "it is awfully sour. Takes time to grow.", 18, [10, 0, 0, 0, 40], 70),
    ("LIECHI", BerryFirmness::VeryHard, 111, 2, 1, "A mysterious BERRY. It is rumored to", "contain the power of the sea.", 24, [40, 0, 40, 0, 10], 80),
    ("GANLON", BerryFirmness::VeryHard, 33, 2, 1, "A mysterious BERRY. It is rumored to", "contain the power of the land.", 24, [0, 40, 0, 40, 0], 80),
    ("SALAC", BerryFirmness::VeryHard, 95, 2, 1, "A mysterious BERRY. It is rumored to", "contain the power of the sky.", 24, [0, 0, 40, 0, 40], 80),
    ("PETAYA", BerryFirmness::VeryHard, 237, 2, 1, "A mysterious BERRY. It is rumored to", "contain the power of all living things.", 24, [40, 0, 0, 40, 0], 80),
    ("APICOT", BerryFirmness::Hard, 75, 2, 1, "A very mystifying BERRY. No telling", "what may happen or how it can be used.", 24, [0, 40, 0, 0, 40], 80),
    ("LANSAT", BerryFirmness::Soft, 97, 2, 1, "Said to be a legendary BERRY.", "Holding it supposedly brings joy.", 24, [10, 10, 10, 10, 10], 30),
    ("STARF", BerryFirmness::SuperHard, 153, 2, 1, "So strong, it was abandoned at the", "world's edge. Considered a mirage.", 24, [10, 10, 10, 10, 10], 30),
    ("ENIGMA", BerryFirmness::Unknown, 0, 2, 1, "A completely enigmatic BERRY.", "Appears to have the power of stars.", 24, [40, 40, 40, 40, 40], 40),
    ];

    #[test]
    fn all_43_records_match_pinned_fields_and_text() {
        assert_eq!(BERRIES.len(), 43);
        assert_eq!(BERRIES_COUNT, 43);
        for (index, (row, expected)) in BERRIES.iter().zip(EXPECTED).enumerate() {
            let (name, firmness, size, max, min, d1, d2, hours, flavors, smooth) = expected;
            assert_eq!(row.name(), name, "row {index} name");
            assert_eq!(row.firmness(), firmness, "{name} firmness");
            assert_eq!(row.size(), size, "{name} size");
            assert_eq!(row.max_yield(), max, "{name} max yield");
            assert_eq!(row.min_yield(), min, "{name} min yield");
            assert_eq!(row.description1(), d1, "{name} description1");
            assert_eq!(row.description2(), d2, "{name} description2");
            assert_eq!(row.stage_duration_hours(), hours, "{name} stage duration");
            assert_eq!(row.flavors().as_array(), flavors, "{name} flavors");
            assert_eq!(row.smoothness(), smooth, "{name} smoothness");
        }
    }

    #[test]
    fn first_and_last_item_mapping() {
        let table = BerryTable::new();
        assert_eq!(table.len(), 43);
        assert!(!table.is_empty());
        assert_eq!(FIRST_BERRY_ITEM, ItemId(133));
        assert_eq!(LAST_BERRY_ITEM, ItemId(175));
        let first = table.get(ItemId::CHERI_BERRY).expect("Cheri is a berry");
        assert_eq!((first.name(), first.berry_type()), ("CHERI", 1));
        assert_eq!(first.item(), ItemId(133));
        let last = table.get(ItemId::ENIGMA_BERRY).expect("Enigma is a berry");
        assert_eq!((last.name(), last.berry_type()), ("ENIGMA", 43));
        assert_eq!(last.item(), ItemId(175));
        // Item-relative indexing: each row's item is FIRST + its position.
        for (index, row) in table.iter().enumerate() {
            assert_eq!(usize::from(row.item().index()), 133 + index);
            assert_eq!(usize::from(row.berry_type()), index + 1);
            assert_eq!(table.get(row.item()), Some(row));
        }
        // Spot-check the mid-table mapping against the upstream item ids.
        assert_eq!(table.get(ItemId(142)).map(BerryData::name), Some("SITRUS"));
        assert_eq!(table.get(ItemId(153)).map(BerryData::name), Some("POMEG"));
        assert_eq!(table.get(ItemId(174)).map(BerryData::name), Some("STARF"));
    }

    #[test]
    fn berry_rows_align_with_the_berry_pocket_items() {
        let items = ItemTable::new();
        let berries = BerryTable::new();
        for index in 0..ITEMS_COUNT {
            let id = ItemId(u16::try_from(index).expect("item count fits u16"));
            let in_range = (133..=175).contains(&index);
            assert_eq!(berries.get(id).is_some(), in_range, "item {index}");
            if in_range {
                assert_eq!(items.get(id).expect("item").pocket, Pocket::Berries);
            }
        }
    }

    #[test]
    fn non_berry_items_are_rejected() {
        let table = BerryTable::new();
        for id in [
            ItemId::NONE,
            ItemId::BERRY_JUICE,
            ItemId(132),
            ItemId::UNUSED_BERRY_1,
            ItemId(178),
            ItemId::BERRY_POUCH,
            ItemId(u16::MAX),
        ] {
            assert_eq!(table.get(id), None, "{id:?}");
        }
    }

    #[test]
    fn by_type_follows_the_get_berry_info_fallback() {
        let table = BerryTable::new();
        // pokeemerald/src/berry.c:988-989: BERRY_NONE and anything past the
        // last berry (43) fall back to type 1.
        assert_eq!(table.by_type(0).name, "CHERI");
        assert_eq!(table.by_type(1).name, "CHERI");
        assert_eq!(table.by_type(43).name, "ENIGMA");
        assert_eq!(table.by_type(44).name, "CHERI");
        assert_eq!(table.by_type(u8::MAX).name, "CHERI");
        assert_eq!(table.by_type(21).name, "POMEG");
        for berry_type in 1..=43u8 {
            let row = table.by_type(berry_type);
            assert_eq!(row.berry_type(), berry_type);
            assert_eq!(table.name_by_type(berry_type), row.name());
        }
        assert_eq!(table.name_by_type(0), "CHERI");
    }

    #[test]
    fn names_fit_upstream_berry_name_length() {
        // BERRY_NAME_LENGTH is 6 (pokeemerald/include/global.berry.h:4).
        for row in BerryTable::new().iter() {
            assert!((1..=6).contains(&row.name().len()), "{}", row.name());
            assert!(row.name().is_ascii());
        }
    }

    #[test]
    fn flavor_order_is_spicy_dry_sweet_bitter_sour() {
        let flavors = BerryFlavors {
            spicy: 1,
            dry: 2,
            sweet: 3,
            bitter: 4,
            sour: 5,
        };
        assert_eq!(flavors.as_array(), [1, 2, 3, 4, 5]);
        assert_eq!(FLAVOR_COUNT, 5);
        let table = BerryTable::new();
        // Cheri is spicy, Chesto dry, Pecha sweet, Rawst bitter, Aspear sour
        // (pokeemerald/src/berry.c:127-131,145-149,163-167,181-185,199-203).
        for (item, expected) in [
            (ItemId::CHERI_BERRY, [10, 0, 0, 0, 0]),
            (ItemId::CHESTO_BERRY, [0, 10, 0, 0, 0]),
            (ItemId::PECHA_BERRY, [0, 0, 10, 0, 0]),
            (ItemId::RAWST_BERRY, [0, 0, 0, 10, 0]),
            (ItemId::ASPEAR_BERRY, [0, 0, 0, 0, 10]),
        ] {
            assert_eq!(
                table.get(item).expect("berry").flavors().as_array(),
                expected
            );
        }
    }

    #[test]
    fn stage_duration_is_hours_and_converts_to_minutes() {
        let table = BerryTable::new();
        let cheri = table.get(ItemId::CHERI_BERRY).expect("berry");
        assert_eq!(cheri.stage_duration_hours(), 3);
        assert_eq!(cheri.stage_duration_minutes(), 180);
        let enigma = table.get(ItemId::ENIGMA_BERRY).expect("berry");
        assert_eq!(enigma.stage_duration_hours(), 24);
        assert_eq!(enigma.stage_duration_minutes(), 1440);
        // Leppa 4, Lum 12, Sitrus 6 hours (pokeemerald/src/berry.c:216,270,288).
        let hours = |item| table.get(item).expect("berry").stage_duration_hours;
        assert_eq!(hours(ItemId::LEPPA_BERRY), 4);
        assert_eq!(hours(ItemId::LUM_BERRY), 12);
        assert_eq!(hours(ItemId::SITRUS_BERRY), 6);
    }

    #[test]
    fn yield_bounds_are_ordered_and_pinned() {
        for row in BerryTable::new().iter() {
            assert!(row.min_yield() >= 1, "{}", row.name());
            assert!(row.min_yield() <= row.max_yield(), "{}", row.name());
        }
        let table = BerryTable::new();
        let yields = |item| {
            let row = table.get(item).expect("berry");
            (row.min_yield(), row.max_yield())
        };
        assert_eq!(yields(ItemId::CHERI_BERRY), (2, 3));
        assert_eq!(yields(ItemId::ENIGMA_BERRY), (1, 2));
        // Pomeg yields up to six, matching its description (pokeemerald/src/berry.c:69).
        assert_eq!(yields(ItemId::POMEG_BERRY).1, 6);
    }

    #[test]
    fn canonical_enigma_row_is_the_default_not_a_save_override() {
        let enigma: &BerryData = &BERRIES[BERRIES_COUNT - 1];
        // pokeemerald/src/berry.c:873-889.
        assert_eq!(enigma.item(), ItemId::ENIGMA_BERRY);
        assert_eq!(enigma.name(), "ENIGMA");
        assert_eq!(enigma.firmness(), BerryFirmness::Unknown);
        assert_eq!(enigma.firmness().id(), 0);
        assert_eq!(enigma.size(), 0);
        assert_eq!((enigma.max_yield(), enigma.min_yield()), (2, 1));
        assert_eq!(enigma.description1(), "A completely enigmatic BERRY.");
        assert_eq!(enigma.description2(), "Appears to have the power of stars.");
        assert_eq!(enigma.stage_duration_hours(), 24);
        assert_eq!(enigma.flavors().as_array(), [40; 5]);
        assert_eq!(enigma.smoothness(), 40);
        // Only the Enigma Berry carries the unknown firmness.
        let unknown = BerryTable::new()
            .iter()
            .filter(|row| row.firmness() == BerryFirmness::Unknown)
            .count();
        assert_eq!(unknown, 1);
    }

    #[test]
    fn firmness_ids_match_upstream_constants() {
        // pokeemerald/include/constants/berry.h:6-11.
        let ids = [
            (BerryFirmness::Unknown, 0),
            (BerryFirmness::VerySoft, 1),
            (BerryFirmness::Soft, 2),
            (BerryFirmness::Hard, 3),
            (BerryFirmness::VeryHard, 4),
            (BerryFirmness::SuperHard, 5),
        ];
        for (firmness, id) in ids {
            assert_eq!(firmness.id(), id);
        }
    }
}
