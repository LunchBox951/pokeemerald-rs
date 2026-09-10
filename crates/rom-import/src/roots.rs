//! The authoritative ROM addresses one revision profile carries.
//!
//! Every field here is a fact about one build of the game: where a symbol
//! sits, how long its data is, what shape its art has. Nothing in this
//! module is discovered at run time. The shipped importer never scans a
//! ROM; it reads this table `(behavioral-fidelity)`.
//!
//! The table is generated, not hand-written. `cargo xtask gen-rom-profile`
//! locates every root by signature against a ROM whose whole-file SHA-1
//! already matched, asserts each root is unique (or resolves a duplicate
//! through a struct back-reference), checks the bytes it would yield
//! against the pack `cargo xtask extract` produces, and emits
//! [`crate::profiles::bpee_rev0`]. Regenerating is how this table changes.
//!
//! # Shape
//!
//! [`Roots`] groups its fields by asset domain, one per domain a later
//! slice will write a reader for. Three record types recur across domains,
//! because the ROM stores art, colours, and opaque blobs the same way
//! wherever they live:
//!
//! - [`ImageRoot`]: tile data plus everything needed to unpack it into the
//!   raster [`pack_format::image_entry_from_tiles`] expects.
//! - [`PaletteRoot`]: a run of GBA-native BGR555 colours.
//! - [`BlobRoot`]: bytes a reader copies through without interpreting.
//!
//! Each carries the pack id it produces, so a reader walks the table rather
//! than restating the id scheme.

use crate::reader::GbaPtr;

/// How a root's bytes sit in the ROM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    /// Stored verbatim. Read `len` bytes and use them.
    Raw,
    /// Stored as a GBA BIOS LZ77 type `0x10` stream
    /// ([`crate::lz77_decompress`]).
    Lz77,
}

/// One image asset: where its GBA tile data lives, and how to unpack it.
///
/// `rom_bit_depth` is how the ROM packs the tiles; `pack_bit_depth` is the
/// depth the pack entry records, which is the *source PNG's* depth and can
/// differ. The title screen's press-start banner is the case that forces
/// the split: the ROM holds 4bpp tiles, the pack entry says 8.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageRoot {
    /// The pack id this image produces.
    pub id: &'static str,
    /// Where the tile data starts.
    pub addr: GbaPtr,
    /// How the tile data is stored.
    pub encoding: Encoding,
    /// Bits per pixel as the ROM packs the tiles: 4 or 8.
    pub rom_bit_depth: u8,
    /// Bits per pixel the pack entry records (the source PNG's depth).
    pub pack_bit_depth: u8,
    /// The raster's width in pixels.
    pub width: u32,
    /// The raster's height in pixels.
    pub height: u32,
    /// `gbagfx`'s `-mwidth`, in tiles. `1` for the plain row-major order.
    pub metatile_width: u32,
    /// `gbagfx`'s `-mheight`, in tiles. `1` for the plain row-major order.
    pub metatile_height: u32,
    /// How many 8x8 tiles the ROM actually stores.
    ///
    /// Often fewer than the raster needs: upstream's `-num_tiles` cut and
    /// its all-zero-trailing-tile trim both shorten the stored art, and
    /// [`pack_format::image_entry_from_tiles`] zero-fills the rest.
    pub tile_count: u32,
}

impl ImageRoot {
    /// How many bytes of tile data the ROM holds, once decompressed.
    #[must_use]
    pub const fn tile_data_len(&self) -> u32 {
        self.tile_count * if self.rom_bit_depth == 4 { 32 } else { 64 }
    }

    /// The metatile shape, in tiles. `(1, 1)` is the plain row-major order.
    ///
    /// [`pack_format::image_entry_from_tiles`] takes it wrapped in
    /// `Some`; wrapping here would only hide that `None` is unreachable.
    #[must_use]
    pub const fn metatile(&self) -> (u32, u32) {
        (self.metatile_width, self.metatile_height)
    }
}

/// One run of GBA-native BGR555 colours.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaletteRoot {
    /// The pack id this palette produces.
    pub id: &'static str,
    /// Where the colours start.
    pub addr: GbaPtr,
    /// How many colours to read. Each is one little-endian `u16`.
    pub color_count: u16,
}

/// Bytes a reader copies through without interpreting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlobRoot {
    /// The pack id this blob produces, or `""` for a blob that only exists
    /// to be pointed at (a tileset's metatile table, say).
    pub id: &'static str,
    /// Where the bytes start.
    pub addr: GbaPtr,
    /// How the bytes are stored.
    pub encoding: Encoding,
    /// The length in bytes, after decompression for [`Encoding::Lz77`].
    pub len: u32,
}

/// One `struct Tileset` and everything it points at.
///
/// The six fields taken from the struct appear in
/// `pokeemerald/include/global.fieldmap.h`'s own declaration order, so a
/// reader validating the struct checks them in the order it reads them.
/// `name` and `anims` are this port's own, and bracket them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TilesetRoot {
    /// The tileset's normalized pack name, e.g. `"general"`.
    pub name: &'static str,
    /// The address of the `struct Tileset` itself.
    pub struct_addr: GbaPtr,
    /// The struct's `isCompressed` field.
    pub is_compressed: bool,
    /// The struct's `isSecondary` field.
    pub is_secondary: bool,
    /// The tile sheet the struct's `tiles` field points at.
    pub tiles: ImageRoot,
    /// The 16 palette banks the struct's `palettes` field points at.
    pub palettes: &'static [PaletteRoot],
    /// The metatile table the struct's `metatiles` field points at.
    pub metatiles: BlobRoot,
    /// The attribute table the struct's `metatileAttributes` field points
    /// at.
    pub metatile_attributes: BlobRoot,
    /// The struct's `callback` field, `0` when the tileset animates
    /// nothing. Not a [`GbaPtr`]: it addresses code in the ROM's text
    /// section with the Thumb bit set, and is recorded only so a reader can
    /// corroborate the struct.
    pub callback: u32,
    /// The animations `src/tileset_anims.c` drives for this tileset.
    pub anims: &'static [TileAnimRoot],
}

/// One tileset animation: a named set of frames.
///
/// Frames are stored uncompressed and are *not* contiguous in ROM, nor laid
/// out in frame order, so each carries its own address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TileAnimRoot {
    /// The animation's normalized pack name, e.g. `"flower"`.
    pub name: &'static str,
    /// The frames, in pack-id order (`0`, `1`, ...).
    pub frames: &'static [ImageRoot],
}

/// The title screen's graphics, tilemaps, and palettes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TitleScreenRoots {
    /// Every `title/image/*` sheet.
    pub images: &'static [ImageRoot],
    /// Every `title/raw/*` tilemap.
    pub tilemaps: &'static [BlobRoot],
    /// Every `title/palette/*` bank.
    pub palettes: &'static [PaletteRoot],
    /// `gTitleScreenBgPalettes`: the logo palette and the
    /// rayquaza/clouds palette, concatenated and loaded as one block.
    /// Both appear in [`Self::palettes`] under their own ids.
    pub bg_palettes: GbaPtr,
}

/// Object-event sprite sheets and their palettes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpriteRoots {
    /// `sObjectEventSpritePalettes`, the `{data, tag}` table that resolves
    /// which palette a graphics id draws from. The generator uses it to
    /// pick the right address for a palette whose bytes repeat in the ROM.
    pub palette_table: GbaPtr,
    /// Every `sprite/*` sheet.
    pub sheets: &'static [ImageRoot],
    /// Every `sprite/palette/*` bank.
    pub palettes: &'static [PaletteRoot],
}

/// One `struct MapLayout` and the two grids it points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapLayoutRoot {
    /// The layout's normalized pack name, e.g. `"littleroot_town"`.
    pub name: &'static str,
    /// The address of the `struct MapLayout` itself.
    pub struct_addr: GbaPtr,
    /// The struct's `width` field, in metatiles.
    pub width: u32,
    /// The struct's `height` field, in metatiles.
    pub height: u32,
    /// The `map.bin` grid. Its length is what upstream's own file holds,
    /// which for a few layouts is longer than `width * height * 2`.
    pub map: BlobRoot,
    /// The `border.bin` block, always four `u16` cells.
    pub border: BlobRoot,
    /// The struct's `primaryTileset` field.
    pub primary_tileset: GbaPtr,
    /// The struct's `secondaryTileset` field.
    pub secondary_tileset: GbaPtr,
}

/// One Latin glyph sheet, stored in `gbagfx`'s `.latfont` layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FontRoot {
    /// The pack id this sheet produces.
    pub id: &'static str,
    /// Where the glyph data starts.
    pub addr: GbaPtr,
    /// The stored length in bytes.
    pub len: u32,
    /// The unpacked sheet's width in pixels.
    pub width: u32,
    /// The unpacked sheet's height in pixels.
    pub height: u32,
    /// Bits per pixel: `2` for every Latin sheet.
    pub bit_depth: u8,
}

/// Text-window frames, the message box, and their palettes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextWindowRoots {
    /// Every `text-window/image/*` sheet.
    pub images: &'static [ImageRoot],
    /// Every `text-window/palette/*` bank.
    pub palettes: &'static [PaletteRoot],
    /// `sTextWindowPalettes`: the message-box bank followed by the four
    /// extra textbox banks. The four appear in [`Self::palettes`] under
    /// their own ids.
    pub window_palettes: GbaPtr,
}

/// Interface palettes the no-save menus need.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InterfaceRoots {
    /// Every `interface/palette/*` bank.
    pub palettes: &'static [PaletteRoot],
}

/// One entry of `gSongTable` and the song header it points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SongRoot {
    /// The pack id this song produces.
    pub id: &'static str,
    /// The song's `MUS_*` index into `gSongTable`.
    pub index: u16,
    /// The `SongHeader` the table entry points at.
    pub header: GbaPtr,
    /// The header's `trackCount`.
    pub track_count: u8,
    /// The voicegroup the header's `tone` field points at. It is one of
    /// [`AudioRoots::voicegroups`].
    pub voicegroup: GbaPtr,
}

/// One voicegroup: a run of 12-byte `ToneData` slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VoicegroupRoot {
    /// The pack id this voicegroup produces.
    pub id: &'static str,
    /// The upstream `voicegroup_*` label, minus its prefix.
    pub label: &'static str,
    /// The address the ROM's own pointers carry. A group declared with a
    /// `starting_note` bias is addressed *before* its first slot, exactly
    /// as the mixer indexes it.
    pub addr: GbaPtr,
    /// The `starting_note` bias, `0` for an unbiased group.
    pub starting_note: u8,
    /// How many slots the upstream `.inc` declares.
    pub declared_slots: u16,
    /// How many slots a reader addresses. Always 128: the mixer's
    /// `voicegroup + voice * 12` fetch is unchecked, so a song may select a
    /// slot past the declared tail.
    pub addressable_slots: u16,
}

/// One key-split table: one child-slot index per note.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeysplitRoot {
    /// The upstream `keysplit_*` label, minus its prefix.
    pub label: &'static str,
    /// The address the ROM's own pointers carry, biased back by
    /// `starting_note` so `addr[note]` indexes directly.
    pub addr: GbaPtr,
    /// The first note the table maps.
    pub starting_note: u8,
    /// How many notes the table maps, starting at
    /// [`Self::starting_note`].
    pub len: u16,
}

/// One instrument sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SampleRoot {
    /// The pack id this sample produces.
    pub id: &'static str,
    /// The address of the sample's `WaveData` header for a `DirectSound`
    /// sample, or of the 16-byte table itself for a programmable wave.
    pub addr: GbaPtr,
    /// The `WaveData` header length in bytes, `0` for a programmable wave
    /// (which has no header).
    pub header_len: u32,
    /// The PCM payload's length in bytes: sample count for `DirectSound`,
    /// 16 for a programmable wave.
    pub data_len: u32,
}

/// The audio roots `MUS_TITLE` needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioRoots {
    /// `gSongTable`, an array of 8-byte `{header, ms, me}` entries.
    pub song_table: GbaPtr,
    /// Every song with a pack entry.
    pub songs: &'static [SongRoot],
    /// Every voicegroup reachable from those songs.
    pub voicegroups: &'static [VoicegroupRoot],
    /// Every key-split table those voicegroups reference.
    pub keysplits: &'static [KeysplitRoot],
    /// Every `DirectSound` sample those voicegroups reference.
    pub direct_sound: &'static [SampleRoot],
    /// Every CGB programmable-wave table those voicegroups reference.
    pub programmable_wave: &'static [SampleRoot],
}

/// The authoritative ROM addresses a profile carries, grouped by domain.
///
/// `#[non_exhaustive]` so adding a domain stays a non-breaking change for
/// anything outside this crate. Construct one only in a generated profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct Roots {
    /// The five tilesets Littleroot Town's layout family references.
    pub tilesets: &'static [TilesetRoot],
    /// The title screen.
    pub title_screen: TitleScreenRoots,
    /// Object-event sprite sheets and palettes.
    pub sprites: SpriteRoots,
    /// The map layouts the early game walks.
    pub layouts: &'static [MapLayoutRoot],
    /// The five Latin glyph sheets.
    pub fonts: &'static [FontRoot],
    /// Text-window frames and palettes.
    pub text_window: TextWindowRoots,
    /// Interface palettes.
    pub interface: InterfaceRoots,
    /// `MUS_TITLE` and everything it plays through.
    pub audio: AudioRoots,
}

impl Roots {
    /// A profile with no roots recorded.
    ///
    /// Not a stand-in for a real build: an importer handed this finds
    /// nothing and fails closed. It exists so a test fixture's profile can
    /// exercise selection without pretending to know a ROM's layout.
    pub const NONE: Self = Self {
        tilesets: &[],
        title_screen: TitleScreenRoots {
            images: &[],
            tilemaps: &[],
            palettes: &[],
            bg_palettes: GbaPtr::AT_BASE,
        },
        sprites: SpriteRoots {
            palette_table: GbaPtr::AT_BASE,
            sheets: &[],
            palettes: &[],
        },
        layouts: &[],
        fonts: &[],
        text_window: TextWindowRoots {
            images: &[],
            palettes: &[],
            window_palettes: GbaPtr::AT_BASE,
        },
        interface: InterfaceRoots { palettes: &[] },
        audio: AudioRoots {
            song_table: GbaPtr::AT_BASE,
            songs: &[],
            voicegroups: &[],
            keysplits: &[],
            direct_sound: &[],
            programmable_wave: &[],
        },
    };

    /// Every image root in the table, across every domain.
    ///
    /// A reader that wants "every `title/image/*`" walks the domain it
    /// belongs to; this is for whole-table checks, like asserting no two
    /// roots claim the same pack id.
    pub fn images(&self) -> impl Iterator<Item = &ImageRoot> {
        self.tilesets
            .iter()
            .flat_map(|tileset| {
                std::iter::once(&tileset.tiles)
                    .chain(tileset.anims.iter().flat_map(|anim| anim.frames.iter()))
            })
            .chain(self.title_screen.images.iter())
            .chain(self.sprites.sheets.iter())
            .chain(self.text_window.images.iter())
    }

    /// Every palette root in the table, across every domain.
    pub fn palettes(&self) -> impl Iterator<Item = &PaletteRoot> {
        self.tilesets
            .iter()
            .flat_map(|tileset| tileset.palettes.iter())
            .chain(self.title_screen.palettes.iter())
            .chain(self.sprites.palettes.iter())
            .chain(self.text_window.palettes.iter())
            .chain(self.interface.palettes.iter())
    }
}

impl Default for Roots {
    fn default() -> Self {
        Self::NONE
    }
}

#[cfg(test)]
mod tests {
    use super::{Encoding, ImageRoot, Roots};
    use crate::reader::GbaPtr;

    fn image(id: &'static str, depth: u8, tiles: u32) -> ImageRoot {
        ImageRoot {
            id,
            addr: GbaPtr::AT_BASE,
            encoding: Encoding::Raw,
            rom_bit_depth: depth,
            pack_bit_depth: depth,
            width: 8,
            height: 8,
            metatile_width: 1,
            metatile_height: 1,
            tile_count: tiles,
        }
    }

    #[test]
    fn an_empty_table_yields_no_roots() {
        let roots = Roots::NONE;
        assert_eq!(roots.images().count(), 0);
        assert_eq!(roots.palettes().count(), 0);
        assert_eq!(Roots::default(), Roots::NONE);
    }

    #[test]
    fn tile_data_length_follows_the_rom_depth() {
        assert_eq!(image("a", 4, 3).tile_data_len(), 96);
        assert_eq!(image("a", 8, 3).tile_data_len(), 192);
    }

    #[test]
    fn a_plain_metatile_shape_is_one_by_one() {
        assert_eq!(image("a", 4, 1).metatile(), (1, 1));
    }
}
