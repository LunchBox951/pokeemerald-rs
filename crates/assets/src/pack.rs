//! The local asset pack loader (S-4, F-3, issue #81 / Discussion #71
//! policy A).
//!
//! `cargo xtask extract` (`crates/xtask/src/extract`) reads the developer's
//! local, gitignored `pokeemerald/` checkout and writes a deterministic,
//! versioned pack file — tileset tile graphics, palettes, and player/NPC
//! sprite sheets, keyed by normalized asset ids (see that module's docs for
//! the exact format and id scheme; this module is its read side and
//! intentionally mirrors it rather than sharing code, so the two crates
//! stay decoupled — `xtask` never depends on `assets`, and vice versa).
//!
//! The pack itself is **never committed, never a CI artifact, never
//! embedded in a binary** (owner decision, Discussion #71). It exists only
//! on a developer's disk after they run `./init.sh` then
//! `cargo xtask extract`. Every accessor here that needs the pack's bytes
//! therefore has a real failure mode for "it isn't there yet" —
//! [`PackError::NotFound`] — with a message that says exactly what to run,
//! per the issue's "clear diagnostic" requirement.
//!
//! # A second error type, deliberately
//!
//! [`error::PackError`] is its own enum rather than added variants on
//! [`crate::error::AssetError`] — see [`crate::error`]'s module docs for
//! why (in short: `AssetError` is used inside `const fn` table
//! initializers elsewhere in this crate, which requires every value it can
//! hold to have a `const`-evaluable destructor; `PackError` needs owned
//! `String`/`PathBuf` payloads, which don't qualify).
//!
//! # Typed access
//!
//! [`AssetPack::tileset`] returns a [`TilesetHandle`] bundling a tileset's
//! tile bitmap, its 16 palettes, and its raw metatile tables.
//! [`AssetPack::sprite`] and [`AssetPack::sprite_palette`] reach the
//! player/NPC sprite sheets and the two player palettes.
//! [`AssetPack::layout_map`] / [`AssetPack::layout_border`] reach a map
//! layout's grid/border bytes — deliberately raw byte accessors, not a
//! decoding handle: `crate::map_layouts`'s `LayoutGrid`/`BorderGrid` own the
//! decode, this crate's pack loader stays decoupled from it (same rationale
//! as `xtask::extract`/`crates::assets::pack` staying decoupled from each
//! other — see this module's docs). [`AssetPack::font`] reaches a Latin
//! font's glyph sheet (S-4, issue #114) as a
//! [`FontImageRef`] bound to its [`FontId`], with
//! [`crate::fonts::FontGlyphSheet`] owning the per-glyph decode.
//! [`AssetPack::text_window_frame`] / [`AssetPack::message_box`] bundle a
//! border frame's tile bitmap with its palette (see [`WindowFrameHandle`]);
//! [`AssetPack::text_window_extra_palette`] reaches the four additional
//! textbox colour palettes. The lower-level [`AssetPack::image`] /
//! [`AssetPack::palette`] / [`AssetPack::raw`] accessors work over any entry
//! by its full id (used directly for e.g. `title/image/*` entries, which
//! have no bundling handle of their own).
//!
//! # Format (mirrors `xtask::extract::pack`'s write side — version 1)
//!
//! ```text
//! Header:   magic: [u8; 8] = b"PKMRPACK", format_version: u32, entry_count: u32
//! Directory (entry_count entries, sorted ascending by id):
//!   id_len: u16, id: [u8; id_len], kind: u8, offset: u64, length: u64,
//!   + kind-specific fixed metadata (Image: width/height/bit_depth;
//!     Palette: color_count; Raw: none)
//! Payload region: every entry's payload bytes, concatenated in directory order.
//! ```
//!
//! # Module layout
//!
//! [`error`] (`PackError`), [`format`] (the binary layout + parser), and
//! [`handles`] (the borrowed typed views) are split out one-concept-per-file
//! `(oop-boundaries)`; this file is just [`AssetPack`] itself — loading and
//! the typed accessor methods.

mod error;
mod format;
mod handles;

use std::path::{Path, PathBuf};

use crate::fonts::{FontId, FontImageRef};

pub use error::PackError;
pub use format::{EntryKind, FORMAT_VERSION, MAGIC};
pub use handles::{ImageRef, PaletteRef, TilesetHandle, WindowFrameHandle};

use format::Entry;

/// The pack's location, relative to the repository root — must match
/// `xtask::extract::OUTPUT_RELATIVE_PATH` exactly (duplicated rather than
/// shared, per this module's docs on why the two crates stay decoupled).
const OUTPUT_RELATIVE_PATH: &str = "assets-pack/pokeemerald.pack";

/// A loaded asset pack: the whole file's bytes, plus a parsed, id-sorted
/// directory for lookups.
///
/// Cheap to query (binary search over an in-memory `Vec`, no per-call I/O)
/// once loaded. Not `Clone` — packs are tens of entries to low hundreds of
/// KiB; callers are expected to load once and hold a reference (or an
/// `Rc`/`Arc` of their own choosing — this crate imposes no policy there,
/// `no global mutable state`).
#[derive(Debug)]
pub struct AssetPack {
    bytes: Vec<u8>,
    entries: Vec<Entry>,
}

impl AssetPack {
    /// The pack's default location: `<repo root>/assets-pack/pokeemerald.pack`,
    /// computed from this crate's own manifest directory (robust regardless
    /// of the caller's current working directory — `cargo test` in
    /// particular runs each crate's tests with that crate's own directory
    /// as `cwd`, not the workspace root).
    ///
    /// # Panics
    ///
    /// Never in practice: `crates/assets` (this crate's own manifest
    /// directory, `env!("CARGO_MANIFEST_DIR")`) is always exactly two path
    /// components under the repository root in this workspace's fixed
    /// layout.
    #[must_use]
    pub fn default_path() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("crates/assets is always two levels under the repo root")
            .join(OUTPUT_RELATIVE_PATH)
    }

    /// Load the pack from [`default_path`](Self::default_path).
    ///
    /// # Errors
    ///
    /// See [`load`](Self::load).
    pub fn load_default() -> Result<Self, PackError> {
        Self::load(&Self::default_path())
    }

    /// Load and parse a pack from `path`.
    ///
    /// # Errors
    ///
    /// Returns [`PackError::NotFound`] (the required "missing pack"
    /// diagnostic) if `path` does not exist; [`PackError::ReadFailed`]
    /// for any other I/O failure; [`PackError::BadMagic`],
    /// [`PackError::UnsupportedVersion`], [`PackError::Truncated`],
    /// or [`PackError::BadEntryKind`] if the file exists but is not a
    /// well-formed version-1 pack.
    pub fn load(path: &Path) -> Result<Self, PackError> {
        let bytes = std::fs::read(path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                PackError::NotFound(path.to_path_buf())
            } else {
                PackError::ReadFailed(path.to_path_buf(), e.to_string())
            }
        })?;
        let entries = format::parse_directory(&bytes)?;
        Ok(Self { bytes, entries })
    }

    /// Binary-search the (id-sorted, per the format's determinism
    /// guarantee) directory for `id`.
    fn find(&self, id: &str) -> Result<&Entry, PackError> {
        self.entries
            .binary_search_by(|e| e.id.as_str().cmp(id))
            .map(|i| &self.entries[i])
            .map_err(|_| PackError::UnknownAsset(id.to_owned()))
    }

    fn payload(&self, entry: &Entry) -> &[u8] {
        &self.bytes[entry.offset..entry.offset + entry.length]
    }

    /// Look up any entry by its full normalized id and view it as an image.
    ///
    /// # Errors
    ///
    /// [`PackError::UnknownAsset`] if no entry has this id;
    /// [`PackError::WrongKind`] if it exists but isn't an
    /// [`EntryKind::Image`].
    pub fn image(&self, id: &str) -> Result<ImageRef<'_>, PackError> {
        let entry = self.find(id)?;
        match entry.kind {
            EntryKind::Image {
                width,
                height,
                bit_depth,
            } => Ok(ImageRef {
                width,
                height,
                bit_depth,
                pixels: self.payload(entry),
            }),
            other => Err(wrong_kind(id, "image", other)),
        }
    }

    /// Look up any entry by its full normalized id and view it as a
    /// palette.
    ///
    /// # Errors
    ///
    /// [`PackError::UnknownAsset`] if no entry has this id;
    /// [`PackError::WrongKind`] if it exists but isn't an
    /// [`EntryKind::Palette`].
    pub fn palette(&self, id: &str) -> Result<PaletteRef<'_>, PackError> {
        let entry = self.find(id)?;
        match entry.kind {
            EntryKind::Palette { color_count } => Ok(PaletteRef {
                color_count,
                raw: self.payload(entry),
            }),
            other => Err(wrong_kind(id, "palette", other)),
        }
    }

    /// Look up any entry by its full normalized id and view it as an opaque
    /// raw blob.
    ///
    /// # Errors
    ///
    /// [`PackError::UnknownAsset`] if no entry has this id;
    /// [`PackError::WrongKind`] if it exists but isn't
    /// [`EntryKind::Raw`].
    pub fn raw(&self, id: &str) -> Result<&[u8], PackError> {
        let entry = self.find(id)?;
        match entry.kind {
            EntryKind::Raw => Ok(self.payload(entry)),
            other => Err(wrong_kind(id, "raw blob", other)),
        }
    }

    /// Bundle one tileset's tile bitmap, 16 palettes, and raw metatile
    /// tables. `name` is the tileset's normalized name (e.g. `"general"`,
    /// `"brendans_mays_house"` — see `xtask::extract::mod`'s module docs
    /// for the five tilesets the pack currently ships).
    ///
    /// # Errors
    ///
    /// [`PackError::UnknownAsset`] if `name` isn't a tileset in this pack
    /// (any of its constituent entries missing counts as this); the same
    /// [`PackError::WrongKind`] cases as [`image`](Self::image) /
    /// [`palette`](Self::palette) / [`raw`](Self::raw) if an entry exists
    /// under the expected id but with the wrong kind.
    pub fn tileset(&self, name: &str) -> Result<TilesetHandle<'_>, PackError> {
        let tiles = self.image(&format!("tileset/{name}/tiles"))?;

        let mut palettes: [Option<PaletteRef<'_>>; 16] = [None; 16];
        for (slot, palette) in palettes.iter_mut().enumerate() {
            *palette = Some(self.palette(&format!("tileset/{name}/palette/{slot:02}"))?);
        }
        #[allow(clippy::missing_panics_doc)] // every slot was just filled above
        let palettes = palettes.map(|p| p.expect("every slot filled by the loop above"));

        let metatiles = self.raw(&format!("tileset/{name}/metatiles"))?;
        let metatile_attributes = self.raw(&format!("tileset/{name}/metatile-attributes"))?;

        Ok(TilesetHandle {
            tiles,
            palettes,
            metatiles,
            metatile_attributes,
        })
    }

    /// Look up a player/NPC sprite sheet by its path under `sprite/` (e.g.
    /// `"brendan/walking"`, `"nurse"` — see `xtask::extract::mod`'s module
    /// docs for the full id scheme).
    ///
    /// # Errors
    ///
    /// Same as [`image`](Self::image).
    pub fn sprite(&self, path: &str) -> Result<ImageRef<'_>, PackError> {
        self.image(&format!("sprite/{path}"))
    }

    /// Look up a player character's real in-game palette (`who` is
    /// `"brendan"` or `"may"` — the only two the pack extracts; see
    /// `xtask::extract::mod`'s module docs for why NPC palettes are not
    /// extracted).
    ///
    /// # Errors
    ///
    /// Same as [`palette`](Self::palette).
    pub fn sprite_palette(&self, who: &str) -> Result<PaletteRef<'_>, PackError> {
        self.palette(&format!("sprite/palette/{who}"))
    }

    /// Look up a map layout's grid bytes (`map.bin`) by its normalized pack
    /// name (e.g. `"littleroot_town"` — see `xtask::extract::mod`'s module
    /// docs for the id scheme and which layouts the pack currently ships).
    ///
    /// Hand the returned bytes to
    /// [`MapLayout::grid`](crate::map_layouts::MapLayout::grid) or
    /// [`LayoutGrid::new`](crate::map_layouts::LayoutGrid::new) to decode —
    /// this crate's pack loader and its map-layout decode layer stay
    /// decoupled by design (see this module's docs), so this method only
    /// fetches bytes; it never constructs a `LayoutGrid` itself.
    ///
    /// # Errors
    ///
    /// Same as [`raw`](Self::raw).
    pub fn layout_map(&self, name: &str) -> Result<&[u8], PackError> {
        self.raw(&format!("layout/{name}/map"))
    }

    /// Look up a map layout's border bytes (`border.bin`) by its normalized
    /// pack name. Hand the returned bytes to
    /// [`BorderGrid::new`](crate::map_layouts::BorderGrid::new) to decode
    /// (see [`layout_map`](Self::layout_map)'s docs on why this stays a raw
    /// byte accessor).
    ///
    /// # Errors
    ///
    /// Same as [`raw`](Self::raw).
    pub fn layout_border(&self, name: &str) -> Result<&[u8], PackError> {
        self.raw(&format!("layout/{name}/border"))
    }

    /// Look up a Latin font's glyph sheet by its typed identity. The returned
    /// [`FontImageRef`] is bound to that identity, preventing a caller from
    /// combining one font's pixels with another font's width table. Hand it to
    /// [`FontGlyphSheet::new`](crate::fonts::FontGlyphSheet::new) to decode
    /// individual glyphs — this crate's pack loader and its font decode
    /// layer stay decoupled by design (see this module's docs), so this
    /// method only fetches and identity-binds the raw sheet bitmap.
    ///
    /// # Errors
    ///
    /// Same as [`image`](Self::image).
    pub fn font(&self, font: FontId) -> Result<FontImageRef<'_>, PackError> {
        let image = self.image(&format!("font/{}/glyphs", font.pack_name()))?;
        Ok(FontImageRef::new(font, image))
    }

    /// Bundle one text-window border frame's tile bitmap and palette.
    /// `frame_id` is Emerald's zero-based `sWindowFrames` index (`0..=19`);
    /// it is translated to the one-based source filenames
    /// `1.png`..`20.png`. Like upstream `GetWindowFrameTilesPal`, an id at or
    /// above `WINDOW_FRAMES_COUNT` falls back to frame id `0` (source file
    /// `1.png`). The palette comes from the source PNG's own `PLTE` chunk,
    /// not a sibling `.pal` file — see
    /// `xtask::extract::png::decode_palette`'s docs.
    ///
    /// # Errors
    ///
    /// [`PackError::UnknownAsset`] if the selected frame is absent from the
    /// pack (including when an older pack predates it); the same
    /// [`PackError::WrongKind`] cases as [`image`](Self::image) /
    /// [`palette`](Self::palette);
    /// [`PackError::MalformedTextWindowPalette`] if the palette entry is
    /// not the exact 16-colour/32-byte bank the handle documents;
    /// [`PackError::MalformedTextWindowImage`] if the tile bitmap's
    /// payload length disagrees with its declared `width * height`;
    /// [`PackError::TextWindowPixelOutsidePalette`] if the tile bitmap
    /// holds a pixel index its bundled palette cannot map.
    pub fn text_window_frame(&self, frame_id: u8) -> Result<WindowFrameHandle<'_>, PackError> {
        const WINDOW_FRAMES_COUNT: u8 = 20;
        let source_number = if frame_id < WINDOW_FRAMES_COUNT {
            frame_id + 1
        } else {
            1
        };
        self.window_frame(
            &format!("text-window/image/{source_number}"),
            &format!("text-window/palette/{source_number}"),
        )
    }

    /// Bundle the default message-box tile bitmap and palette (upstream
    /// `gMessageBox_Gfx`/`gMessageBox_Pal`, `pokeemerald/src/graphics.c`) —
    /// the frame every standard overworld/battle text box uses, distinct
    /// from the 20 selectable [`text_window_frame`](Self::text_window_frame)
    /// options menu frames.
    ///
    /// # Errors
    ///
    /// Same as [`text_window_frame`](Self::text_window_frame).
    pub fn message_box(&self) -> Result<WindowFrameHandle<'_>, PackError> {
        self.window_frame(
            "text-window/image/message_box",
            "text-window/palette/message_box",
        )
    }

    /// Look up one of the four additional textbox colour palettes (`n` is
    /// `1..=4`, upstream `text_pal1.pal`..`text_pal4.pal`,
    /// `sTextWindowPalettes[1..=4]` — slot `0` is
    /// [`message_box`](Self::message_box)'s own palette, not reachable
    /// through this accessor).
    ///
    /// # Errors
    ///
    /// Same as [`text_window_frame`](Self::text_window_frame)'s palette
    /// cases.
    pub fn text_window_extra_palette(&self, n: u8) -> Result<PaletteRef<'_>, PackError> {
        self.text_window_palette(&format!("text-window/palette/text_pal{n}"))
    }

    /// Look up a text-window palette and enforce the exact one-GBA-bank
    /// shape (16 declared colours, 32 payload bytes) every typed
    /// text-window accessor documents. Extraction validates this on write
    /// (`xtask::extract`'s text-window pairing checks), but the read side
    /// must not trust pack metadata: a corrupt or hand-built pack could
    /// otherwise hand out a [`WindowFrameHandle`] whose 16-colour
    /// invariant is false.
    fn text_window_palette(&self, id: &str) -> Result<PaletteRef<'_>, PackError> {
        const TEXT_WINDOW_PALETTE_COLORS: u16 = 16;
        const TEXT_WINDOW_PALETTE_BYTES: usize = 32;
        let palette = self.palette(id)?;
        if palette.color_count != TEXT_WINDOW_PALETTE_COLORS
            || palette.raw.len() != TEXT_WINDOW_PALETTE_BYTES
        {
            return Err(PackError::MalformedTextWindowPalette {
                id: id.to_owned(),
                color_count: palette.color_count,
                byte_len: palette.raw.len(),
            });
        }
        Ok(palette)
    }

    /// Bundle a text-window frame's tile bitmap and palette, validating
    /// the pair on read like [`text_window_palette`](Self::text_window_palette)
    /// does for the palette alone: the read side must not trust pack
    /// contents. A tile bitmap whose payload length disagrees with its own
    /// declared `width * height` (violating [`ImageRef`]'s documented
    /// pixel-count invariant), or holding a pixel its 16-colour palette
    /// cannot map (possible in a corrupt or hand-built pack carrying an
    /// 8-bit-indexed image), is rejected here rather than handed to a
    /// renderer to index out of bounds. Extraction enforces the same pair
    /// rule on write (`xtask::extract`'s text-window pairing checks).
    fn window_frame(
        &self,
        image_id: &str,
        palette_id: &str,
    ) -> Result<WindowFrameHandle<'_>, PackError> {
        let tiles = self.image(image_id)?;
        let declared = u64::from(tiles.width) * u64::from(tiles.height);
        if !u64::try_from(tiles.pixels.len()).is_ok_and(|len| len == declared) {
            return Err(PackError::MalformedTextWindowImage {
                id: image_id.to_owned(),
                width: tiles.width,
                height: tiles.height,
                byte_len: tiles.pixels.len(),
            });
        }
        let palette = self.text_window_palette(palette_id)?;
        if let Some(&pixel) = tiles
            .pixels
            .iter()
            .find(|&&pixel| u16::from(pixel) >= palette.color_count)
        {
            return Err(PackError::TextWindowPixelOutsidePalette {
                id: image_id.to_owned(),
                pixel,
                palette_len: palette.color_count,
            });
        }
        Ok(WindowFrameHandle { tiles, palette })
    }
}

fn wrong_kind(id: &str, expected: &'static str, actual: EntryKind) -> PackError {
    PackError::WrongKind {
        id: id.to_owned(),
        expected,
        actual: actual.label(),
    }
}

#[cfg(test)]
mod tests;
