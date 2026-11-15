//! The Secret Base / player-room decoration catalog: identity, name,
//! placement rules, price, and description for every `gDecorations` row.
//!
//! Source: `src/data/decoration/header.h` (the `gDecorations` records),
//! `src/data/decoration/description.h` (description text), and
//! `include/constants/decorations.h` (IDs 0 through 120). The per-decoration
//! `.tiles` payload and the icon, palette, and tilemap graphics are not part
//! of this table.

use std::error::Error;
use std::fmt;

/// The number of addressable decoration IDs, including [`DecorationId::NONE`]
/// (`NUM_DECORATIONS` plus the unnamed zero row).
pub const DECORATIONS_COUNT: usize = 121;

/// The number of decoration categories (`DECORCAT_COUNT`).
pub const DECORATION_CATEGORY_COUNT: usize = 8;

/// A failed decoding of a decoration ID or one of its record fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecorationError {
    /// The value is not a `DECOR_*` ID below [`DECORATIONS_COUNT`].
    UnknownDecoration(u8),
    /// The value is not a `DecorationPermission`.
    UnknownPermission(u8),
    /// The value is not a `DecorationShape`.
    UnknownShape(u8),
    /// The value is not a `DecorationCategory`.
    UnknownCategory(u8),
}

impl fmt::Display for DecorationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownDecoration(id) => write!(f, "unknown decoration id `{id}`"),
            Self::UnknownPermission(id) => write!(f, "unknown decoration permission `{id}`"),
            Self::UnknownShape(id) => write!(f, "unknown decoration shape `{id}`"),
            Self::UnknownCategory(id) => write!(f, "unknown decoration category `{id}`"),
        }
    }
}

impl Error for DecorationError {}

/// A decoration identifier (`DECOR_*`). The named values are the upstream IDs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DecorationId(u8);

impl DecorationId {
    /// Validates a raw decoration ID.
    ///
    /// # Errors
    ///
    /// Returns [`DecorationError::UnknownDecoration`] when `id` is
    /// [`DECORATIONS_COUNT`] or more.
    pub const fn new(id: u8) -> Result<Self, DecorationError> {
        if (id as usize) < DECORATIONS_COUNT {
            Ok(Self(id))
        } else {
            Err(DecorationError::UnknownDecoration(id))
        }
    }

    /// Returns the raw upstream ID.
    #[must_use]
    pub const fn index(self) -> u8 {
        self.0
    }
}

/// Which floor and wall tiles a decoration may stand on (`DecorationPermission`).
/// The name gives collision, then placement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecorationPermission {
    /// `DECORPERM_SOLID_FLOOR`.
    SolidFloor,
    /// `DECORPERM_PASS_FLOOR`.
    PassFloor,
    /// `DECORPERM_BEHIND_FLOOR`.
    BehindFloor,
    /// `DECORPERM_NA_WALL`.
    NaWall,
    /// `DECORPERM_SPRITE`.
    Sprite,
}

impl DecorationPermission {
    /// Decodes an upstream `DECORPERM_*` value.
    ///
    /// # Errors
    ///
    /// Returns [`DecorationError::UnknownPermission`] for any other value.
    pub const fn from_id(id: u8) -> Result<Self, DecorationError> {
        match id {
            0 => Ok(Self::SolidFloor),
            1 => Ok(Self::PassFloor),
            2 => Ok(Self::BehindFloor),
            3 => Ok(Self::NaWall),
            4 => Ok(Self::Sprite),
            _ => Err(DecorationError::UnknownPermission(id)),
        }
    }

    /// Returns the upstream `DECORPERM_*` value.
    #[must_use]
    pub const fn id(self) -> u8 {
        self as u8
    }
}

/// A decoration's footprint in metatiles (`DecorationShape`), named
/// `W<width>H<height>` after `DECORSHAPE_<width>x<height>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecorationShape {
    /// `DECORSHAPE_1x1`.
    W1H1,
    /// `DECORSHAPE_2x1`.
    W2H1,
    /// `DECORSHAPE_3x1`, unused upstream.
    W3H1,
    /// `DECORSHAPE_4x2`.
    W4H2,
    /// `DECORSHAPE_2x2`.
    W2H2,
    /// `DECORSHAPE_1x2`.
    W1H2,
    /// `DECORSHAPE_1x3`, unused upstream.
    W1H3,
    /// `DECORSHAPE_2x4`.
    W2H4,
    /// `DECORSHAPE_3x3`.
    W3H3,
    /// `DECORSHAPE_3x2`.
    W3H2,
}

impl DecorationShape {
    /// Decodes an upstream `DECORSHAPE_*` value.
    ///
    /// # Errors
    ///
    /// Returns [`DecorationError::UnknownShape`] for any other value.
    pub const fn from_id(id: u8) -> Result<Self, DecorationError> {
        match id {
            0 => Ok(Self::W1H1),
            1 => Ok(Self::W2H1),
            2 => Ok(Self::W3H1),
            3 => Ok(Self::W4H2),
            4 => Ok(Self::W2H2),
            5 => Ok(Self::W1H2),
            6 => Ok(Self::W1H3),
            7 => Ok(Self::W2H4),
            8 => Ok(Self::W3H3),
            9 => Ok(Self::W3H2),
            _ => Err(DecorationError::UnknownShape(id)),
        }
    }

    /// Returns the upstream `DECORSHAPE_*` value.
    #[must_use]
    pub const fn id(self) -> u8 {
        self as u8
    }

    /// Returns the footprint as `(width, height)` in metatiles, the
    /// dimensions `ShowDecorationOnMap` and `SetUpDecorationShape` use.
    #[must_use]
    pub const fn dimensions(self) -> (u8, u8) {
        match self {
            Self::W1H1 => (1, 1),
            Self::W2H1 => (2, 1),
            Self::W3H1 => (3, 1),
            Self::W4H2 => (4, 2),
            Self::W2H2 => (2, 2),
            Self::W1H2 => (1, 2),
            Self::W1H3 => (1, 3),
            Self::W2H4 => (2, 4),
            Self::W3H3 => (3, 3),
            Self::W3H2 => (3, 2),
        }
    }
}

/// The menu category a decoration is filed under (`DecorationCategory`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecorationCategory {
    /// `DECORCAT_DESK`.
    Desk,
    /// `DECORCAT_CHAIR`.
    Chair,
    /// `DECORCAT_PLANT`.
    Plant,
    /// `DECORCAT_ORNAMENT`.
    Ornament,
    /// `DECORCAT_MAT`.
    Mat,
    /// `DECORCAT_POSTER`.
    Poster,
    /// `DECORCAT_DOLL`.
    Doll,
    /// `DECORCAT_CUSHION`.
    Cushion,
}

impl DecorationCategory {
    /// Decodes an upstream `DECORCAT_*` value.
    ///
    /// # Errors
    ///
    /// Returns [`DecorationError::UnknownCategory`] for any other value,
    /// including `DECORCAT_COUNT`.
    pub const fn from_id(id: u8) -> Result<Self, DecorationError> {
        match id {
            0 => Ok(Self::Desk),
            1 => Ok(Self::Chair),
            2 => Ok(Self::Plant),
            3 => Ok(Self::Ornament),
            4 => Ok(Self::Mat),
            5 => Ok(Self::Poster),
            6 => Ok(Self::Doll),
            7 => Ok(Self::Cushion),
            _ => Err(DecorationError::UnknownCategory(id)),
        }
    }

    /// Returns the upstream `DECORCAT_*` value.
    #[must_use]
    pub const fn id(self) -> u8 {
        self as u8
    }
}

/// One `gDecorations` row, without its `.tiles` graphics. Text is in the
/// game's charmap notation: `\n` in a description is a real line break.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Decoration {
    /// The row's `.id`, equal to its table position.
    pub id: DecorationId,
    /// The menu name (`.name`, at most 15 characters). [`DecorationId::NONE`]
    /// carries upstream's `"SMALL DESK"`.
    pub name: &'static str,
    /// Where the decoration may be placed (`.permission`).
    pub permission: DecorationPermission,
    /// The footprint (`.shape`).
    pub shape: DecorationShape,
    /// The menu category (`.category`).
    pub category: DecorationCategory,
    /// The Poké Mart price (`.price`); zero for [`DecorationId::NONE`].
    pub price: u16,
    /// The summary shown for the selected decoration (`.description`).
    pub description: &'static str,
}

/// Lookup over the decoration catalog.
#[derive(Debug, Clone, Copy)]
pub struct DecorationTable {
    rows: &'static [Decoration; DECORATIONS_COUNT],
}

impl DecorationTable {
    /// Creates a handle on the canonical catalog.
    #[must_use]
    pub const fn new() -> Self {
        Self { rows: &DECORATIONS }
    }

    /// Returns the number of rows.
    #[must_use]
    pub const fn len(&self) -> usize {
        DECORATIONS_COUNT
    }

    /// Returns `false`; the catalog is never empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        false
    }

    /// Returns the row for `id`.
    #[must_use]
    pub const fn get(&self, id: DecorationId) -> &Decoration {
        &self.rows[id.0 as usize]
    }

    /// Iterates in ascending [`DecorationId`] order.
    pub fn iter(&self) -> impl Iterator<Item = &Decoration> {
        self.rows.iter()
    }
}

impl Default for DecorationTable {
    fn default() -> Self {
        Self::new()
    }
}

macro_rules! define_decorations {
    ($($name:ident = $id:literal => (
        $text:literal, $permission:ident, $shape:ident, $category:ident, $price:literal, $description:literal
    )),+ $(,)?) => {
        impl DecorationId {
            $(
                #[doc = concat!("`DECOR_", stringify!($name), "`.")]
                pub const $name: Self = Self($id);
            )+
        }

        const DECORATIONS: [Decoration; DECORATIONS_COUNT] = [
            $(Decoration {
                id: DecorationId::$name,
                name: $text,
                permission: DecorationPermission::$permission,
                shape: DecorationShape::$shape,
                category: DecorationCategory::$category,
                price: $price,
                description: $description,
            }),+
        ];

        const _: () = {
            let ids: [usize; DECORATIONS_COUNT] = [$($id),+];
            let mut index = 0;
            while index < DECORATIONS_COUNT {
                assert!(ids[index] == index);
                index += 1;
            }
        };
    };
}

#[rustfmt::skip]
define_decorations! {
    NONE = 0 => ("SMALL DESK", SolidFloor, W1H1, Desk, 0, "A small desk built\nfor one."),
    SMALL_DESK = 1 => ("SMALL DESK", SolidFloor, W1H1, Desk, 3000, "A small desk built\nfor one."),
    POKEMON_DESK = 2 => ("POKéMON DESK", SolidFloor, W1H1, Desk, 3000, "A small desk built in\nthe shape of a POKé\nBALL."),
    HEAVY_DESK = 3 => ("HEAVY DESK", SolidFloor, W3H2, Desk, 6000, "A large desk made\nof steel. Put some\ndecorations on it."),
    RAGGED_DESK = 4 => ("RAGGED DESK", SolidFloor, W3H2, Desk, 6000, "A large desk made\nof wood. Put some\ndecorations on it."),
    COMFORT_DESK = 5 => ("COMFORT DESK", SolidFloor, W3H2, Desk, 6000, "A large desk made\nof leaves. Put some\ndecorations on it."),
    PRETTY_DESK = 6 => ("PRETTY DESK", SolidFloor, W3H3, Desk, 9000, "A huge desk made\nof glass. Holds lots\nof decorations."),
    BRICK_DESK = 7 => ("BRICK DESK", SolidFloor, W3H3, Desk, 9000, "A huge desk made\nof brick. Holds lots\nof decorations."),
    CAMP_DESK = 8 => ("CAMP DESK", SolidFloor, W3H3, Desk, 9000, "A huge desk made\nof logs. Put lots of\ndecorations on it."),
    HARD_DESK = 9 => ("HARD DESK", SolidFloor, W3H3, Desk, 9000, "A huge desk made\nof rocks. Holds\nmany decorations."),
    SMALL_CHAIR = 10 => ("SMALL CHAIR", PassFloor, W1H1, Chair, 2000, "A small chair made\nfor one."),
    POKEMON_CHAIR = 11 => ("POKéMON CHAIR", PassFloor, W1H1, Chair, 2000, "A small chair built\nin the shape of a\nPOKé BALL."),
    HEAVY_CHAIR = 12 => ("HEAVY CHAIR", PassFloor, W1H1, Chair, 2000, "A small chair made\nof steel."),
    PRETTY_CHAIR = 13 => ("PRETTY CHAIR", PassFloor, W1H1, Chair, 2000, "A small chair made\nof glass."),
    COMFORT_CHAIR = 14 => ("COMFORT CHAIR", PassFloor, W1H1, Chair, 2000, "A small chair made\nof leaves."),
    RAGGED_CHAIR = 15 => ("RAGGED CHAIR", PassFloor, W1H1, Chair, 2000, "A small chair made\nof wood."),
    BRICK_CHAIR = 16 => ("BRICK CHAIR", PassFloor, W1H1, Chair, 2000, "A small chair made\nof brick."),
    CAMP_CHAIR = 17 => ("CAMP CHAIR", PassFloor, W1H1, Chair, 2000, "A small chair made\nof logs."),
    HARD_CHAIR = 18 => ("HARD CHAIR", PassFloor, W1H1, Chair, 2000, "A small chair made\nof rock."),
    RED_PLANT = 19 => ("RED PLANT", BehindFloor, W1H2, Plant, 3000, "A vivid red potted\nplant."),
    TROPICAL_PLANT = 20 => ("TROPICAL PLANT", BehindFloor, W1H2, Plant, 3000, "A flowering tropical\nplant in a pot."),
    PRETTY_FLOWERS = 21 => ("PRETTY FLOWERS", BehindFloor, W1H2, Plant, 3000, "A pot of cute\nflowers."),
    COLORFUL_PLANT = 22 => ("COLORFUL PLANT", BehindFloor, W2H2, Plant, 5000, "A large pot with\nmany colorful\nflowers."),
    BIG_PLANT = 23 => ("BIG PLANT", BehindFloor, W2H2, Plant, 5000, "A large, umbrella-\nshaped plant in a\nbig pot."),
    GORGEOUS_PLANT = 24 => ("GORGEOUS PLANT", BehindFloor, W2H2, Plant, 5000, "A large, impressive\nplant in a big pot."),
    RED_BRICK = 25 => ("RED BRICK", SolidFloor, W1H2, Ornament, 500, "A red-colored brick.\nDecorations can be\nplaced on top."),
    YELLOW_BRICK = 26 => ("YELLOW BRICK", SolidFloor, W1H2, Ornament, 500, "A yellow-colored\nbrick. Put some\ndecorations on top."),
    BLUE_BRICK = 27 => ("BLUE BRICK", SolidFloor, W1H2, Ornament, 500, "A blue-colored\nbrick. Put some\ndecorations on top."),
    RED_BALLOON = 28 => ("RED BALLOON", PassFloor, W1H1, Ornament, 500, "A red balloon filled\nwith water. Bursts\nif stepped on."),
    BLUE_BALLOON = 29 => ("BLUE BALLOON", PassFloor, W1H1, Ornament, 500, "A blue balloon filled\nwith water. Bursts\nif stepped on."),
    YELLOW_BALLOON = 30 => ("YELLOW BALLOON", PassFloor, W1H1, Ornament, 500, "A yellow balloon\nfilled with water.\nPops if stepped on."),
    RED_TENT = 31 => ("RED TENT", PassFloor, W3H3, Ornament, 10000, "A large red tent.\nYou can hide inside\nit."),
    BLUE_TENT = 32 => ("BLUE TENT", PassFloor, W3H3, Ornament, 10000, "A large blue tent.\nYou can hide inside\nit."),
    SOLID_BOARD = 33 => ("SOLID BOARD", PassFloor, W1H2, Ornament, 3000, "Place over a hole to\ncross to the other\nside."),
    SLIDE = 34 => ("SLIDE", PassFloor, W2H4, Ornament, 8000, "Use to slide down\nfrom the platform."),
    FENCE_LENGTH = 35 => ("FENCE LENGTH", SolidFloor, W1H1, Ornament, 500, "A small fence that\nblocks passage."),
    FENCE_WIDTH = 36 => ("FENCE WIDTH", SolidFloor, W1H1, Ornament, 500, "A small fence that\nblocks passage."),
    TIRE = 37 => ("TIRE", SolidFloor, W2H2, Ornament, 800, "An old large tire.\nDecorations can be\nplaced on top."),
    STAND = 38 => ("STAND", PassFloor, W4H2, Ornament, 7000, "A large pedestal\nwith steps."),
    MUD_BALL = 39 => ("MUD BALL", PassFloor, W1H1, Ornament, 200, "A large ball of mud.\nCrumbles if stepped\non."),
    BREAKABLE_DOOR = 40 => ("BREAKABLE DOOR", PassFloor, W1H2, Ornament, 3000, "A weird door that\npeople can walk\nright through."),
    SAND_ORNAMENT = 41 => ("SAND ORNAMENT", BehindFloor, W1H2, Ornament, 3000, "An ornament made\nof sand. Crumbles if\ntouched."),
    SILVER_SHIELD = 42 => ("SILVER SHIELD", BehindFloor, W1H2, Ornament, 0, "Awarded for 50\nstraight wins at\nthe BATTLE TOWER."),
    GOLD_SHIELD = 43 => ("GOLD SHIELD", BehindFloor, W1H2, Ornament, 0, "Awarded for 100\nstraight wins at\nthe BATTLE TOWER."),
    GLASS_ORNAMENT = 44 => ("GLASS ORNAMENT", BehindFloor, W1H2, Ornament, 0, "A glass replica of\na famous sculpture\nat the ART MUSEUM."),
    TV = 45 => ("TV", SolidFloor, W1H1, Ornament, 3000, "A small, gray-\ncolored toy TV."),
    ROUND_TV = 46 => ("ROUND TV", SolidFloor, W1H1, Ornament, 4000, "A toy TV modeled\nin the image of a\nSEEDOT."),
    CUTE_TV = 47 => ("CUTE TV", SolidFloor, W1H1, Ornament, 4000, "A toy TV modeled\nin the image of a\nSKITTY."),
    GLITTER_MAT = 48 => ("GLITTER MAT", PassFloor, W1H1, Mat, 2000, "An odd mat that\nglitters if stepped\non."),
    JUMP_MAT = 49 => ("JUMP MAT", PassFloor, W1H1, Mat, 2000, "A trick mat that\njumps when it is\nstepped on."),
    SPIN_MAT = 50 => ("SPIN MAT", PassFloor, W1H1, Mat, 2000, "A trick mat that\nspins around when\nstepped on."),
    C_LOW_NOTE_MAT = 51 => ("C Low NOTE MAT", PassFloor, W1H1, Mat, 500, "A mat that plays\na low C note when\nstepped on."),
    D_NOTE_MAT = 52 => ("D NOTE MAT", PassFloor, W1H1, Mat, 500, "A mat that plays\na D note when\nstepped on."),
    E_NOTE_MAT = 53 => ("E NOTE MAT", PassFloor, W1H1, Mat, 500, "A mat that plays\nan E note when\nstepped on."),
    F_NOTE_MAT = 54 => ("F NOTE MAT", PassFloor, W1H1, Mat, 500, "A mat that plays\nan F note when\nstepped on."),
    G_NOTE_MAT = 55 => ("G NOTE MAT", PassFloor, W1H1, Mat, 500, "A mat that plays\na G note when\nstepped on."),
    A_NOTE_MAT = 56 => ("A NOTE MAT", PassFloor, W1H1, Mat, 500, "A mat that plays\nan A note when\nstepped on."),
    B_NOTE_MAT = 57 => ("B NOTE MAT", PassFloor, W1H1, Mat, 500, "A mat that plays\na B note when\nstepped on."),
    C_HIGH_NOTE_MAT = 58 => ("C High NOTE MAT", PassFloor, W1H1, Mat, 500, "A mat that plays\na high C note when\nstepped on."),
    SURF_MAT = 59 => ("SURF MAT", PassFloor, W3H3, Mat, 4000, "A mat designed with\na SURF image.\nPut items on top."),
    THUNDER_MAT = 60 => ("THUNDER MAT", PassFloor, W3H3, Mat, 4000, "A mat designed with\na THUNDER image.\nPut items on top."),
    FIRE_BLAST_MAT = 61 => ("FIRE BLAST MAT", PassFloor, W3H3, Mat, 4000, "A mat designed with\na FIRE BLAST image.\nPut items on top."),
    POWDER_SNOW_MAT = 62 => ("POWDER SNOW MAT", PassFloor, W3H3, Mat, 4000, "A mat with a POWDER\nSNOW image design.\nPut items on top."),
    ATTRACT_MAT = 63 => ("ATTRACT MAT", PassFloor, W3H3, Mat, 4000, "A mat designed with\nan ATTRACT image.\nPut items on top."),
    FISSURE_MAT = 64 => ("FISSURE MAT", PassFloor, W3H3, Mat, 4000, "A mat designed with\na FISSURE image.\nPut items on top."),
    SPIKES_MAT = 65 => ("SPIKES MAT", PassFloor, W3H3, Mat, 4000, "A mat designed with\na SPIKES image.\nPut items on top."),
    BALL_POSTER = 66 => ("BALL POSTER", NaWall, W1H1, Poster, 1000, "A small poster\nprinted with POKé\nBALLS."),
    GREEN_POSTER = 67 => ("GREEN POSTER", NaWall, W1H1, Poster, 1000, "A small poster with\na TREECKO print."),
    RED_POSTER = 68 => ("RED POSTER", NaWall, W1H1, Poster, 1000, "A small poster with\na TORCHIC print."),
    BLUE_POSTER = 69 => ("BLUE POSTER", NaWall, W1H1, Poster, 1000, "A small poster with\na MUDKIP print."),
    CUTE_POSTER = 70 => ("CUTE POSTER", NaWall, W1H1, Poster, 1000, "A small poster with\nan AZURILL print."),
    PIKA_POSTER = 71 => ("PIKA POSTER", NaWall, W2H1, Poster, 1500, "A large poster with\na PIKACHU and\nPICHU print."),
    LONG_POSTER = 72 => ("LONG POSTER", NaWall, W2H1, Poster, 1500, "A large poster with\na SEVIPER print."),
    SEA_POSTER = 73 => ("SEA POSTER", NaWall, W2H1, Poster, 1500, "A large poster with\na RELICANTH print."),
    SKY_POSTER = 74 => ("SKY POSTER", NaWall, W2H1, Poster, 1500, "A large poster with\na WINGULL print."),
    KISS_POSTER = 75 => ("KISS POSTER", NaWall, W2H1, Poster, 1500, "A large poster with\na SMOOCHUM print."),
    PICHU_DOLL = 76 => ("PICHU DOLL", Sprite, W1H1, Doll, 3000, "A PICHU doll.\nPlace it on a mat\nor a desk."),
    PIKACHU_DOLL = 77 => ("PIKACHU DOLL", Sprite, W1H1, Doll, 3000, "A PIKACHU doll.\nPlace it on a mat\nor a desk."),
    MARILL_DOLL = 78 => ("MARILL DOLL", Sprite, W1H1, Doll, 3000, "A MARILL doll.\nPlace it on a mat\nor a desk."),
    TOGEPI_DOLL = 79 => ("TOGEPI DOLL", Sprite, W1H1, Doll, 3000, "A TOGEPI doll.\nPlace it on a mat\nor a desk."),
    CYNDAQUIL_DOLL = 80 => ("CYNDAQUIL DOLL", Sprite, W1H1, Doll, 3000, "A CYNDAQUIL doll.\nPlace it on a mat\nor a desk."),
    CHIKORITA_DOLL = 81 => ("CHIKORITA DOLL", Sprite, W1H1, Doll, 3000, "A CHIKORITA doll.\nPlace it on a mat\nor a desk."),
    TOTODILE_DOLL = 82 => ("TOTODILE DOLL", Sprite, W1H1, Doll, 3000, "A TOTODILE doll.\nPlace it on a mat\nor a desk."),
    JIGGLYPUFF_DOLL = 83 => ("JIGGLYPUFF DOLL", Sprite, W1H1, Doll, 3000, "A JIGGLYPUFF doll.\nPlace it on a mat\nor a desk."),
    MEOWTH_DOLL = 84 => ("MEOWTH DOLL", Sprite, W1H1, Doll, 3000, "A MEOWTH doll.\nPlace it on a mat\nor a desk."),
    CLEFAIRY_DOLL = 85 => ("CLEFAIRY DOLL", Sprite, W1H1, Doll, 3000, "A CLEFAIRY doll.\nPlace it on a mat\nor a desk."),
    DITTO_DOLL = 86 => ("DITTO DOLL", Sprite, W1H1, Doll, 3000, "A DITTO doll.\nPlace it on a mat\nor a desk."),
    SMOOCHUM_DOLL = 87 => ("SMOOCHUM DOLL", Sprite, W1H1, Doll, 3000, "A SMOOCHUM doll.\nPlace it on a mat\nor a desk."),
    TREECKO_DOLL = 88 => ("TREECKO DOLL", Sprite, W1H1, Doll, 3000, "A TREECKO doll.\nPlace it on a mat\nor a desk."),
    TORCHIC_DOLL = 89 => ("TORCHIC DOLL", Sprite, W1H1, Doll, 3000, "A TORCHIC doll.\nPlace it on a mat\nor a desk."),
    MUDKIP_DOLL = 90 => ("MUDKIP DOLL", Sprite, W1H1, Doll, 3000, "A MUDKIP doll.\nPlace it on a mat\nor a desk."),
    DUSKULL_DOLL = 91 => ("DUSKULL DOLL", Sprite, W1H1, Doll, 3000, "A DUSKULL doll.\nPlace it on a mat\nor a desk."),
    WYNAUT_DOLL = 92 => ("WYNAUT DOLL", Sprite, W1H1, Doll, 3000, "A WYNAUT doll.\nPlace it on a mat\nor a desk."),
    BALTOY_DOLL = 93 => ("BALTOY DOLL", Sprite, W1H1, Doll, 3000, "A BALTOY doll.\nPlace it on a mat\nor a desk."),
    KECLEON_DOLL = 94 => ("KECLEON DOLL", Sprite, W1H1, Doll, 3000, "A KECLEON doll.\nPlace it on a mat\nor a desk."),
    AZURILL_DOLL = 95 => ("AZURILL DOLL", Sprite, W1H1, Doll, 3000, "An AZURILL doll.\nPlace it on a mat\nor a desk."),
    SKITTY_DOLL = 96 => ("SKITTY DOLL", Sprite, W1H1, Doll, 3000, "A SKITTY doll.\nPlace it on a mat\nor a desk."),
    SWABLU_DOLL = 97 => ("SWABLU DOLL", Sprite, W1H1, Doll, 3000, "A SWABLU doll.\nPlace it on a mat\nor a desk."),
    GULPIN_DOLL = 98 => ("GULPIN DOLL", Sprite, W1H1, Doll, 3000, "A GULPIN doll.\nPlace it on a mat\nor a desk."),
    LOTAD_DOLL = 99 => ("LOTAD DOLL", Sprite, W1H1, Doll, 3000, "A LOTAD doll.\nPlace it on a mat\nor a desk."),
    SEEDOT_DOLL = 100 => ("SEEDOT DOLL", Sprite, W1H1, Doll, 3000, "A SEEDOT doll.\nPlace it on a mat\nor a desk."),
    PIKA_CUSHION = 101 => ("PIKA CUSHION", Sprite, W1H1, Cushion, 2000, "A PIKACHU cushion.\nPlace it on a mat\nor a desk."),
    ROUND_CUSHION = 102 => ("ROUND CUSHION", Sprite, W1H1, Cushion, 2000, "A MARILL cushion.\nPlace it on a mat\nor a desk."),
    KISS_CUSHION = 103 => ("KISS CUSHION", Sprite, W1H1, Cushion, 2000, "A SMOOCHUM\ncushion. Place it on\na mat or a desk."),
    ZIGZAG_CUSHION = 104 => ("ZIGZAG CUSHION", Sprite, W1H1, Cushion, 2000, "A ZIGZAGOON\ncushion. Place it on\na mat or a desk."),
    SPIN_CUSHION = 105 => ("SPIN CUSHION", Sprite, W1H1, Cushion, 2000, "A SPINDA cushion.\nPlace it on a mat\nor a desk."),
    DIAMOND_CUSHION = 106 => ("DIAMOND CUSHION", Sprite, W1H1, Cushion, 2000, "A SABLEYE cushion.\nPlace it on a mat\nor a desk."),
    BALL_CUSHION = 107 => ("BALL CUSHION", Sprite, W1H1, Cushion, 2000, "A BALL cushion.\nPlace it on a mat\nor a desk."),
    GRASS_CUSHION = 108 => ("GRASS CUSHION", Sprite, W1H1, Cushion, 2000, "A grass-mark\ncushion. Place it on\na mat or a desk."),
    FIRE_CUSHION = 109 => ("FIRE CUSHION", Sprite, W1H1, Cushion, 2000, "A fire-mark\ncushion. Place it on\na mat or a desk."),
    WATER_CUSHION = 110 => ("WATER CUSHION", Sprite, W1H1, Cushion, 2000, "A water-mark\ncushion. Place it on\na mat or a desk."),
    SNORLAX_DOLL = 111 => ("SNORLAX DOLL", Sprite, W1H2, Doll, 10000, "A large doll.\nPlace it on a mat\nor a desk."),
    RHYDON_DOLL = 112 => ("RHYDON DOLL", Sprite, W1H2, Doll, 10000, "A large doll.\nPlace it on a mat\nor a desk."),
    LAPRAS_DOLL = 113 => ("LAPRAS DOLL", Sprite, W1H2, Doll, 10000, "A large doll.\nPlace it on a mat\nor a desk."),
    VENUSAUR_DOLL = 114 => ("VENUSAUR DOLL", Sprite, W1H2, Doll, 10000, "A large doll.\nPlace it on a mat\nor a desk."),
    CHARIZARD_DOLL = 115 => ("CHARIZARD DOLL", Sprite, W1H2, Doll, 10000, "A large doll.\nPlace it on a mat\nor a desk."),
    BLASTOISE_DOLL = 116 => ("BLASTOISE DOLL", Sprite, W1H2, Doll, 10000, "A large doll.\nPlace it on a mat\nor a desk."),
    WAILMER_DOLL = 117 => ("WAILMER DOLL", Sprite, W1H2, Doll, 10000, "A large doll.\nPlace it on a mat\nor a desk."),
    REGIROCK_DOLL = 118 => ("REGIROCK DOLL", Sprite, W1H2, Doll, 10000, "A large doll.\nPlace it on a mat\nor a desk."),
    REGICE_DOLL = 119 => ("REGICE DOLL", Sprite, W1H2, Doll, 10000, "A large doll.\nPlace it on a mat\nor a desk."),
    REGISTEEL_DOLL = 120 => ("REGISTEEL DOLL", Sprite, W1H2, Doll, 10000, "A large doll.\nPlace it on a mat\nor a desk."),
}

#[cfg(test)]
mod tests {
    use super::{
        Decoration, DecorationCategory, DecorationError, DecorationId, DecorationPermission,
        DecorationShape, DecorationTable, DECORATIONS_COUNT, DECORATION_CATEGORY_COUNT,
    };

    fn row(id: DecorationId) -> Decoration {
        *DecorationTable::new().get(id)
    }

    fn count(matches: impl Fn(&Decoration) -> bool) -> usize {
        DecorationTable::new().iter().filter(|d| matches(d)).count()
    }

    #[test]
    fn every_row_sits_at_its_declared_id() {
        let table = DecorationTable::new();
        assert_eq!(table.len(), DECORATIONS_COUNT);
        assert_eq!(table.iter().count(), DECORATIONS_COUNT);
        assert!(!table.is_empty());
        for (position, decoration) in table.iter().enumerate() {
            assert_eq!(usize::from(decoration.id.index()), position);
            assert_eq!(table.get(decoration.id), decoration);
        }
    }

    #[test]
    fn id_bounds_match_upstream_constants() {
        // include/constants/decorations.h: DECOR_NONE 0 .. DECOR_REGISTEEL_DOLL 120.
        assert_eq!(DecorationId::NONE.index(), 0);
        assert_eq!(DecorationId::SMALL_DESK.index(), 1);
        assert_eq!(DecorationId::REGISTEEL_DOLL.index(), 120);
        assert_eq!(DecorationId::new(0), Ok(DecorationId::NONE));
        assert_eq!(DecorationId::new(120), Ok(DecorationId::REGISTEEL_DOLL));
        for raw in [121, 200, u8::MAX] {
            assert_eq!(
                DecorationId::new(raw),
                Err(DecorationError::UnknownDecoration(raw))
            );
        }
    }

    #[test]
    fn permission_decodes_the_upstream_enum() {
        use DecorationPermission::*;
        let expected = [SolidFloor, PassFloor, BehindFloor, NaWall, Sprite];
        for (raw, permission) in (0u8..).zip(expected) {
            assert_eq!(DecorationPermission::from_id(raw), Ok(permission));
            assert_eq!(permission.id(), raw);
        }
        assert_eq!(
            DecorationPermission::from_id(5),
            Err(DecorationError::UnknownPermission(5))
        );
    }

    #[test]
    fn shape_decodes_the_upstream_enum_and_dimensions() {
        use DecorationShape::*;
        // include/decoration.h:17-28 order; src/decoration.c:1247-1278 sizes.
        let expected = [
            (W1H1, (1, 1)),
            (W2H1, (2, 1)),
            (W3H1, (3, 1)),
            (W4H2, (4, 2)),
            (W2H2, (2, 2)),
            (W1H2, (1, 2)),
            (W1H3, (1, 3)),
            (W2H4, (2, 4)),
            (W3H3, (3, 3)),
            (W3H2, (3, 2)),
        ];
        for (raw, (shape, dimensions)) in (0u8..).zip(expected) {
            assert_eq!(DecorationShape::from_id(raw), Ok(shape));
            assert_eq!(shape.id(), raw);
            assert_eq!(shape.dimensions(), dimensions);
        }
        assert_eq!(
            DecorationShape::from_id(10),
            Err(DecorationError::UnknownShape(10))
        );
    }

    #[test]
    fn category_decodes_the_upstream_enum() {
        use DecorationCategory::*;
        let expected = [Desk, Chair, Plant, Ornament, Mat, Poster, Doll, Cushion];
        assert_eq!(expected.len(), DECORATION_CATEGORY_COUNT);
        for (raw, category) in (0u8..).zip(expected) {
            assert_eq!(DecorationCategory::from_id(raw), Ok(category));
            assert_eq!(category.id(), raw);
        }
        assert_eq!(
            DecorationCategory::from_id(8),
            Err(DecorationError::UnknownCategory(8))
        );
    }

    #[test]
    fn rows_per_category_match_upstream_counts() {
        use DecorationCategory::*;
        // DECOR_NONE is filed under DECORCAT_DESK, as header.h:9 has it.
        let expected = [
            (Desk, 10),
            (Chair, 9),
            (Plant, 6),
            (Ornament, 23),
            (Mat, 18),
            (Poster, 10),
            (Doll, 35),
            (Cushion, 10),
        ];
        for (category, rows) in expected {
            assert_eq!(count(|d| d.category == category), rows, "{category:?}");
        }
        assert_eq!(expected.iter().map(|(_, rows)| rows).sum::<usize>(), 121);
    }

    #[test]
    fn rows_per_permission_and_shape_match_upstream_counts() {
        use DecorationPermission::*;
        use DecorationShape::*;
        for (permission, rows) in [
            (SolidFloor, 19),
            (PassFloor, 37),
            (BehindFloor, 10),
            (NaWall, 10),
            (Sprite, 45),
        ] {
            assert_eq!(
                count(|d| d.permission == permission),
                rows,
                "{permission:?}"
            );
        }
        for (shape, rows) in [
            (W1H1, 72),
            (W2H1, 5),
            (W3H1, 0),
            (W4H2, 1),
            (W2H2, 4),
            (W1H2, 22),
            (W1H3, 0),
            (W2H4, 1),
            (W3H3, 13),
            (W3H2, 3),
        ] {
            assert_eq!(count(|d| d.shape == shape), rows, "{shape:?}");
        }
    }

    #[test]
    fn none_row_carries_the_small_desk_text_and_no_price() {
        let none = row(DecorationId::NONE);
        let small_desk = row(DecorationId::SMALL_DESK);
        assert_eq!(none.name, "SMALL DESK");
        assert!(!none.name.is_empty());
        assert_eq!(none.price, 0);
        assert_eq!(none.description, small_desk.description);
        assert_eq!(
            (none.permission, none.shape, none.category),
            (small_desk.permission, small_desk.shape, small_desk.category)
        );
        assert_eq!(small_desk.price, 3000);
    }

    #[test]
    fn prices_match_upstream_values() {
        let table = DecorationTable::new();
        let mut distinct: Vec<u16> = table.iter().map(|d| d.price).collect();
        distinct.sort_unstable();
        distinct.dedup();
        assert_eq!(
            distinct,
            [0, 200, 500, 800, 1000, 1500, 2000, 3000, 4000, 5000, 6000, 7000, 8000, 9000, 10000]
        );
        // The BATTLE TOWER shields and the ART MUSEUM glass ornament are awarded,
        // not sold (header.h price 0).
        let free: Vec<_> = table
            .iter()
            .filter(|d| d.price == 0)
            .map(|d| d.id.index())
            .collect();
        assert_eq!(free, [0, 42, 43, 44]);
    }

    #[test]
    fn name_and_description_text_fit_the_game_box() {
        for decoration in DecorationTable::new().iter() {
            // `u8 name[16]` leaves 15 characters before the terminator.
            assert!((1..=15).contains(&decoration.name.chars().count()));
            let lines = decoration.description.split('\n').count();
            assert!((2..=3).contains(&lines), "{:?}", decoration.id);
            assert!(!decoration.description.contains('\\'));
        }
    }

    type Pin = (
        DecorationId,
        &'static str,
        DecorationPermission,
        DecorationShape,
        DecorationCategory,
        u16,
        &'static str,
    );

    const PINS: [Pin; 9] = [
        (
            DecorationId::POKEMON_DESK,
            "POKéMON DESK",
            DecorationPermission::SolidFloor,
            DecorationShape::W1H1,
            DecorationCategory::Desk,
            3000,
            "A small desk built in\nthe shape of a POKé\nBALL.",
        ),
        (
            DecorationId::SLIDE,
            "SLIDE",
            DecorationPermission::PassFloor,
            DecorationShape::W2H4,
            DecorationCategory::Ornament,
            8000,
            "Use to slide down\nfrom the platform.",
        ),
        (
            DecorationId::STAND,
            "STAND",
            DecorationPermission::PassFloor,
            DecorationShape::W4H2,
            DecorationCategory::Ornament,
            7000,
            "A large pedestal\nwith steps.",
        ),
        (
            DecorationId::BIG_PLANT,
            "BIG PLANT",
            DecorationPermission::BehindFloor,
            DecorationShape::W2H2,
            DecorationCategory::Plant,
            5000,
            "A large, umbrella-\nshaped plant in a\nbig pot.",
        ),
        (
            DecorationId::SILVER_SHIELD,
            "SILVER SHIELD",
            DecorationPermission::BehindFloor,
            DecorationShape::W1H2,
            DecorationCategory::Ornament,
            0,
            "Awarded for 50\nstraight wins at\nthe BATTLE TOWER.",
        ),
        (
            DecorationId::FIRE_BLAST_MAT,
            "FIRE BLAST MAT",
            DecorationPermission::PassFloor,
            DecorationShape::W3H3,
            DecorationCategory::Mat,
            4000,
            "A mat designed with\na FIRE BLAST image.\nPut items on top.",
        ),
        (
            DecorationId::BALL_POSTER,
            "BALL POSTER",
            DecorationPermission::NaWall,
            DecorationShape::W1H1,
            DecorationCategory::Poster,
            1000,
            "A small poster\nprinted with POKé\nBALLS.",
        ),
        (
            DecorationId::SNORLAX_DOLL,
            "SNORLAX DOLL",
            DecorationPermission::Sprite,
            DecorationShape::W1H2,
            DecorationCategory::Doll,
            10000,
            "A large doll.\nPlace it on a mat\nor a desk.",
        ),
        (
            DecorationId::REGISTEEL_DOLL,
            "REGISTEEL DOLL",
            DecorationPermission::Sprite,
            DecorationShape::W1H2,
            DecorationCategory::Doll,
            10000,
            "A large doll.\nPlace it on a mat\nor a desk.",
        ),
    ];

    #[test]
    fn hand_pinned_rows_match_upstream() {
        for (id, name, permission, shape, category, price, description) in PINS {
            let decoration = row(id);
            assert_eq!(decoration.id, id);
            assert_eq!(decoration.name, name, "{id:?}");
            assert_eq!(decoration.permission, permission, "{id:?}");
            assert_eq!(decoration.shape, shape, "{id:?}");
            assert_eq!(decoration.category, category, "{id:?}");
            assert_eq!(decoration.price, price, "{id:?}");
            assert_eq!(decoration.description, description, "{id:?}");
        }
    }

    fn fnv1a64(bytes: &[u8]) -> u64 {
        bytes.iter().fold(0xcbf2_9ce4_8422_2325_u64, |hash, &byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
        })
    }

    /// Digest of upstream `gDecorations` and `DecorDesc_*`, serialized per row
    /// as id, name and NUL, permission, shape, category, little-endian price,
    /// description and NUL.
    #[test]
    fn every_row_matches_the_canonical_digest() {
        let mut out = Vec::new();
        for decoration in DecorationTable::new().iter() {
            out.push(decoration.id.index());
            out.extend_from_slice(decoration.name.as_bytes());
            out.push(0);
            out.push(decoration.permission.id());
            out.push(decoration.shape.id());
            out.push(decoration.category.id());
            out.extend_from_slice(&decoration.price.to_le_bytes());
            out.extend_from_slice(decoration.description.as_bytes());
            out.push(0);
        }
        assert_eq!(out.len(), 7545);
        assert_eq!(
            format!("{:016x}", fnv1a64(&out)),
            "e83bdf4fefb196aa",
            "decoration catalog diverges from the canonical upstream rows",
        );
    }
}
