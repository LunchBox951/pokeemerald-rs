//! The owned mirror of `rom_import::Roots` the generator builds, plus the
//! per-root report it prints.
//!
//! `Roots` and everything under it is `&'static`, which a running generator
//! cannot build. These types hold the same facts with owned strings and
//! vectors; [`super::emit`] turns them into the `const` the profile module
//! ships.

use rom_import::Encoding;

/// How a located root was pinned down, for the report and for the
/// `--map` cross-check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution {
    /// Exactly one place in the ROM held the expected bytes.
    UniqueSignature,
    /// The bytes appeared more than once, and a struct field or an
    /// adjacent, already-unique root chose between them.
    StructDerived,
    /// The address came from a pointer inside an already-located struct,
    /// never from a search.
    PointerWalk,
    /// Several addresses held the root's bytes, every one of them held the
    /// *same* bytes, and nothing in the ROM distinguishes them. The choice
    /// between them is arbitrary and is recorded as such.
    ///
    /// Sound only because it changes nothing downstream: the importer reads
    /// bytes, and every candidate holds identical ones, so the pack entry
    /// is the same whichever was picked. Never use it where the copies
    /// differ, or where a table or an adjacency could decide.
    ArbitraryAmongIdentical,
}

impl Resolution {
    /// The short word the report prints.
    pub const fn label(self) -> &'static str {
        match self {
            Self::UniqueSignature => "unique",
            Self::StructDerived => "struct",
            Self::PointerWalk => "pointer",
            Self::ArbitraryAmongIdentical => "identical",
        }
    }
}

/// What a linker map should say about a root's address.
///
/// The `--map` cross-check is only as strong as what it can assert. Where
/// upstream's symbol name follows from the pack id, the check is exact.
/// Where it does not (a map layout's symbol is generated at build time and
/// exists nowhere in the checkout), the check only asserts that *some*
/// symbol starts there. Where the address is deliberately inside another
/// symbol, there is nothing to assert at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymbolExpectation {
    /// The address is interior to another symbol. Not checkable.
    Interior,
    /// A symbol must start here, but its name is not derivable.
    Unnamed,
    /// A symbol with exactly this name must start here.
    Exact(String),
    /// A symbol whose name contains every one of these fragments must
    /// start here. For the conventions upstream follows loosely, where a
    /// `static` and a global spelling of the same asset differ only in
    /// their prefix.
    Contains(Vec<String>),
}

/// One line of the generator's report: a root, where it landed, and how.
#[derive(Debug, Clone)]
pub struct ReportLine {
    /// The pack id, or the upstream symbol's role for a root with no pack
    /// entry of its own (`tileset/general` names its `struct Tileset`).
    pub id: String,
    /// The GBA bus address.
    pub addr: u32,
    /// The root's length in bytes, decompressed.
    pub len: u32,
    /// How the address was settled.
    pub resolution: Resolution,
    /// What a linker map should say about this address.
    pub symbol: SymbolExpectation,
    /// Anything a reader of the report needs to know: a duplicate that was
    /// resolved, or a pack entry the ROM disagrees with.
    pub note: Option<String>,
}

impl ReportLine {
    /// A line for a root pinned by a unique byte signature.
    pub fn unique(id: impl Into<String>, addr: u32, len: u32) -> Self {
        Self {
            id: id.into(),
            addr,
            len,
            resolution: Resolution::UniqueSignature,
            symbol: SymbolExpectation::Unnamed,
            note: None,
        }
    }

    /// Set how the address was settled.
    #[must_use]
    pub const fn with(mut self, resolution: Resolution) -> Self {
        self.resolution = resolution;
        self
    }

    /// Mark the address as interior to another symbol.
    #[must_use]
    pub fn interior(mut self) -> Self {
        self.symbol = SymbolExpectation::Interior;
        self
    }

    /// Require a linker map to name exactly this symbol at the address.
    #[must_use]
    pub fn symbol(mut self, name: impl Into<String>) -> Self {
        self.symbol = SymbolExpectation::Exact(name.into());
        self
    }

    /// Require a linker map to name a symbol containing every fragment.
    #[must_use]
    pub fn symbol_contains<I, S>(mut self, fragments: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.symbol = SymbolExpectation::Contains(fragments.into_iter().map(Into::into).collect());
        self
    }

    /// Attach an explanatory note.
    #[must_use]
    pub fn note(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }
}

/// An image root, owned.
#[derive(Debug, Clone)]
pub struct ImagePlan {
    /// The pack id this root produces.
    pub id: String,
    /// The GBA bus address.
    pub addr: u32,
    /// How the bytes are stored.
    pub encoding: Encoding,
    /// Bits per pixel as the ROM packs the tiles.
    pub rom_bit_depth: u8,
    /// Bits per pixel the pack entry records.
    pub pack_bit_depth: u8,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `gbagfx`'s `-mwidth`, in tiles.
    pub metatile_width: u32,
    /// `gbagfx`'s `-mheight`, in tiles.
    pub metatile_height: u32,
    /// How many 8x8 tiles the ROM stores.
    pub tile_count: u32,
}

/// A palette root, owned.
#[derive(Debug, Clone)]
pub struct PalettePlan {
    /// The pack id this root produces.
    pub id: String,
    /// The GBA bus address.
    pub addr: u32,
    /// How many colours to read.
    pub color_count: u16,
}

/// A blob root, owned.
#[derive(Debug, Clone)]
pub struct BlobPlan {
    /// The pack id this root produces.
    pub id: String,
    /// The GBA bus address.
    pub addr: u32,
    /// How the bytes are stored.
    pub encoding: Encoding,
    /// Length in bytes, after decompression.
    pub len: u32,
}

/// A tileset root, owned.
#[derive(Debug, Clone)]
pub struct TilesetPlan {
    /// The normalized pack name.
    pub name: String,
    /// Where the upstream struct itself sits.
    pub struct_addr: u32,
    /// The struct's `isCompressed` field.
    pub is_compressed: bool,
    /// The struct's `isSecondary` field.
    pub is_secondary: bool,
    /// The tile sheet the struct points at.
    pub tiles: ImagePlan,
    /// The palette banks this root owns.
    pub palettes: Vec<PalettePlan>,
    /// The metatile table.
    pub metatiles: BlobPlan,
    /// The metatile attribute table.
    pub metatile_attributes: BlobPlan,
    /// The struct's `callback` field.
    pub callback: u32,
    /// The animations this tileset drives.
    pub anims: Vec<TileAnimPlan>,
}

/// A tileset animation, owned.
#[derive(Debug, Clone)]
pub struct TileAnimPlan {
    /// The normalized pack name.
    pub name: String,
    /// The frames, in pack-id order.
    pub frames: Vec<ImagePlan>,
}

/// The title screen's roots, owned.
#[derive(Debug, Clone)]
pub struct TitleScreenPlan {
    /// Every image root in this domain.
    pub images: Vec<ImagePlan>,
    /// Every tilemap blob in this domain.
    pub tilemaps: Vec<BlobPlan>,
    /// The palette banks this root owns.
    pub palettes: Vec<PalettePlan>,
    /// `gTitleScreenBgPalettes`.
    pub bg_palettes: u32,
}

/// The object-event sprite roots, owned.
#[derive(Debug, Clone)]
pub struct SpritePlan {
    /// `sObjectEventSpritePalettes`.
    pub palette_table: u32,
    /// Every object-event sprite sheet.
    pub sheets: Vec<ImagePlan>,
    /// The palette banks this root owns.
    pub palettes: Vec<PalettePlan>,
}

/// A map layout root, owned.
#[derive(Debug, Clone)]
pub struct MapLayoutPlan {
    /// The normalized pack name.
    pub name: String,
    /// Where the upstream struct itself sits.
    pub struct_addr: u32,
    /// Width in metatiles.
    pub width: u32,
    /// Height in metatiles.
    pub height: u32,
    /// The `map.bin` grid.
    pub map: BlobPlan,
    /// The `border.bin` block.
    pub border: BlobPlan,
    /// The struct's `primaryTileset` field.
    pub primary_tileset: u32,
    /// The struct's `secondaryTileset` field.
    pub secondary_tileset: u32,
}

/// A Latin glyph sheet root, owned.
#[derive(Debug, Clone)]
pub struct FontPlan {
    /// The pack id this root produces.
    pub id: String,
    /// The GBA bus address.
    pub addr: u32,
    /// Length in bytes, after decompression.
    pub len: u32,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Bits per pixel.
    pub bit_depth: u8,
}

/// The text-window roots, owned.
#[derive(Debug, Clone)]
pub struct TextWindowPlan {
    /// Every image root in this domain.
    pub images: Vec<ImagePlan>,
    /// The palette banks this root owns.
    pub palettes: Vec<PalettePlan>,
    /// `sTextWindowPalettes`.
    pub window_palettes: u32,
}

/// One song's roots, owned.
#[derive(Debug, Clone)]
pub struct SongPlan {
    /// The pack id this root produces.
    pub id: String,
    /// The `MUS_*` index into `gSongTable`.
    pub index: u16,
    /// The `SongHeader` the table entry points at.
    pub header: u32,
    /// The header's `trackCount`.
    pub track_count: u8,
    /// The voicegroup the header plays through.
    pub voicegroup: u32,
}

/// One voicegroup's roots, owned.
#[derive(Debug, Clone)]
pub struct VoicegroupPlan {
    /// The pack id this root produces.
    pub id: String,
    /// The upstream symbol label, minus its prefix.
    pub label: String,
    /// The GBA bus address.
    pub addr: u32,
    /// The declared `starting_note` bias.
    pub starting_note: u8,
    /// How many slots the upstream `.inc` declares.
    pub declared_slots: u16,
}

/// One key-split table's roots, owned.
#[derive(Debug, Clone)]
pub struct KeysplitPlan {
    /// The upstream symbol label, minus its prefix.
    pub label: String,
    /// The GBA bus address.
    pub addr: u32,
    /// The declared `starting_note` bias.
    pub starting_note: u8,
    pub len: u16,
}

/// One sample's roots, owned.
#[derive(Debug, Clone)]
pub struct SamplePlan {
    /// The pack id this root produces.
    pub id: String,
    /// The GBA bus address.
    pub addr: u32,
    /// The `WaveData` header length, `0` if none.
    pub header_len: u32,
    /// The PCM payload length in bytes.
    pub data_len: u32,
}

/// The audio roots, owned.
#[derive(Debug, Clone)]
pub struct AudioPlan {
    /// `gSongTable`.
    pub song_table: u32,
    /// Every song with a pack entry.
    pub songs: Vec<SongPlan>,
    /// Every voicegroup reached from those songs.
    pub voicegroups: Vec<VoicegroupPlan>,
    /// Every key-split table those voicegroups use.
    pub keysplits: Vec<KeysplitPlan>,
    /// Every `DirectSound` sample.
    pub direct_sound: Vec<SamplePlan>,
    /// Every CGB programmable-wave table.
    pub programmable_wave: Vec<SamplePlan>,
}

/// Everything one generator run derived.
#[derive(Debug, Clone)]
pub struct ProfilePlan {
    /// The five bundled tilesets.
    pub tilesets: Vec<TilesetPlan>,
    /// The title screen.
    pub title_screen: TitleScreenPlan,
    /// Object-event sprites and palettes.
    pub sprites: SpritePlan,
    /// The bundled map layouts.
    pub layouts: Vec<MapLayoutPlan>,
    /// The five Latin glyph sheets.
    pub fonts: Vec<FontPlan>,
    /// Text-window frames and palettes.
    pub text_window: TextWindowPlan,
    /// Interface palettes.
    pub interface: Vec<PalettePlan>,
    /// `MUS_TITLE` and everything it plays through.
    pub audio: AudioPlan,
}

#[cfg(test)]
mod tests {
    use super::{ReportLine, Resolution, SymbolExpectation};

    #[test]
    fn a_report_line_records_how_its_address_was_settled() {
        let line = ReportLine::unique("tileset/general/tiles", 0x0800_0010, 32);
        assert_eq!(line.resolution, Resolution::UniqueSignature);
        assert_eq!(line.symbol, SymbolExpectation::Unnamed);
        assert!(line.note.is_none());

        let derived = ReportLine::unique("sprite/palette/brendan", 0x0800_0020, 32)
            .with(Resolution::StructDerived)
            .interior()
            .note("three identical banks");
        assert_eq!(derived.resolution, Resolution::StructDerived);
        assert_eq!(derived.symbol, SymbolExpectation::Interior);
        assert_eq!(derived.note.as_deref(), Some("three identical banks"));
    }

    #[test]
    fn a_report_line_carries_the_symbol_a_map_should_name() {
        let exact = ReportLine::unique("x", 0x0800_0000, 4).symbol("gTileset_General");
        assert_eq!(
            exact.symbol,
            SymbolExpectation::Exact("gTileset_General".to_owned())
        );
        let loose = ReportLine::unique("x", 0x0800_0000, 4).symbol_contains([
            "TitleScreen",
            "Rayquaza",
            "Gfx",
        ]);
        assert_eq!(
            loose.symbol,
            SymbolExpectation::Contains(vec![
                "TitleScreen".to_owned(),
                "Rayquaza".to_owned(),
                "Gfx".to_owned(),
            ])
        );
    }

    #[test]
    fn every_resolution_has_a_short_label() {
        assert_eq!(Resolution::UniqueSignature.label(), "unique");
        assert_eq!(Resolution::StructDerived.label(), "struct");
        assert_eq!(Resolution::PointerWalk.label(), "pointer");
        assert_eq!(Resolution::ArbitraryAmongIdentical.label(), "identical");
    }
}
