//! Read side of the local asset pack. [`AssetPack::load`] parses the
//! directory [`pack_format`] writes (which owns the on-disk layout, the id
//! scheme, and the writer `cargo xtask extract` drives); this module's
//! methods look up and decode individual entries.
//!
//! The pack file is never committed, embedded in a binary, or produced by
//! CI — it only exists after a developer runs `cargo xtask extract`. Every
//! accessor that needs its bytes surfaces that absence as
//! [`PackError::NotFound`], whose message names the command to run.
//!
//! [`PackError`] is a separate enum from [`crate::error::AssetError`] —
//! see [`crate::error`] for why.

mod error;
mod format;
mod handles;

use std::path::{Path, PathBuf};

use crate::audio::{Sample, SampleId, Song, VoiceGroup, VoiceGroupId};
use crate::fonts::{FontId, FontImageRef};

pub use error::PackError;
pub use format::{DirectoryEntry, EntryKind, FORMAT_VERSION, MAGIC};
pub use handles::{ImageRef, PaletteRef, TilesetHandle, WindowFrameHandle};

use format::kind_label;

/// A border frame's 3x3 grid of 8x8 tiles (upstream `sWindowFrames`,
/// `graphics/text_window/1.png`..`20.png`).
const FRAME_WIDTH: u32 = 24;
const FRAME_HEIGHT: u32 = 24;

/// The default message-box strip (upstream `gMessageBox_Gfx`,
/// `graphics/text_window/message_box.png`), a different shape from the
/// selectable border frames above.
const MESSAGE_BOX_WIDTH: u32 = 56;
const MESSAGE_BOX_HEIGHT: u32 = 16;

/// Every tileset carries this many palette banks.
const TILESET_PALETTE_COUNT: usize = 16;

/// A loaded asset pack: its bytes plus an id-sorted directory for lookups.
///
/// Cheap to query once loaded (an in-memory binary search, no per-call
/// I/O). Not `Clone` — hold a reference, or your own `Rc`/`Arc`, rather
/// than reloading.
#[derive(Debug)]
pub struct AssetPack {
    bytes: Vec<u8>,
    entries: Vec<DirectoryEntry>,
}

impl AssetPack {
    /// The pack's default location: see [`pack_format::default_pack_path`]
    /// for the full resolution order (env var, then OS user-data directory,
    /// then the executable's own directory, then this checkout's bundled
    /// pack — last, since a shipped binary resolves through the earlier
    /// rungs and only a developer checkout needs it).
    ///
    /// Never fails: the last rung always yields a path; a path that does
    /// not exist surfaces as [`PackError::NotFound`] from
    /// [`load`](Self::load).
    #[must_use]
    pub fn default_path() -> PathBuf {
        pack_format::default_pack_path()
    }

    /// The checkout's own pack: `<repo root>/assets-pack/pokeemerald.pack`,
    /// where `cargo xtask extract` writes — [`default_path`](Self::default_path)'s
    /// last rung, named directly so a checkout-validation gate reads this
    /// pack rather than whichever one the resolver's earlier rungs find
    /// installed.
    #[must_use]
    pub fn repo_pack_path() -> PathBuf {
        pack_format::repo_pack_path()
    }

    /// Load the pack from [`default_path`](Self::default_path).
    ///
    /// # Errors
    ///
    /// See [`load`](Self::load).
    pub fn load_default() -> Result<Self, PackError> {
        Self::load(&Self::default_path())
    }

    /// Load the pack `cargo xtask extract` writes into *this checkout*
    /// ([`repo_pack_path`](Self::repo_pack_path)), not whichever pack
    /// [`default_path`](Self::default_path) resolves to.
    ///
    /// For gates that validate the checkout rather than play the game:
    /// `default_path`'s earlier rungs can resolve to an already-installed
    /// pack, letting an extractor regression pass against a stale one, or
    /// a stale user pack fail a checkout that is otherwise fine
    /// `(test-ratchet)`.
    ///
    /// # Errors
    ///
    /// See [`load`](Self::load). [`PackError::NotFound`] here means "run
    /// `cargo xtask extract` first".
    pub fn load_repo() -> Result<Self, PackError> {
        Self::load(&Self::repo_pack_path())
    }

    /// Load and parse a pack from `path`.
    ///
    /// # Errors
    ///
    /// [`PackError::NotFound`] if `path` does not exist; [`PackError::ReadFailed`]
    /// for any other I/O failure; [`PackError::BadMagic`],
    /// [`PackError::UnsupportedVersion`], [`PackError::Truncated`], or
    /// [`PackError::BadEntryKind`] if the file exists but is not a
    /// well-formed pack at [`FORMAT_VERSION`].
    pub fn load(path: &Path) -> Result<Self, PackError> {
        let bytes = std::fs::read(path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                PackError::NotFound(path.to_path_buf())
            } else {
                PackError::ReadFailed(path.to_path_buf(), e.to_string())
            }
        })?;
        let entries = pack_format::parse_directory(&bytes)?;
        Ok(Self { bytes, entries })
    }

    /// The exact byte buffer retained when this pack was loaded — the same
    /// allocation every typed accessor decodes.
    ///
    /// Lets a caller hash the bytes actually used for decoding without
    /// reopening the source path and racing a replacement of that file.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Walk the directory in id-sorted order: every entry's id, kind
    /// metadata, and the `offset`/`length` its payload occupies in
    /// [`bytes`](Self::bytes).
    ///
    /// For callers that need to know what a pack *contains*, not one
    /// known asset by id.
    pub fn entries(&self) -> impl Iterator<Item = &DirectoryEntry> {
        self.entries.iter()
    }

    /// Relies on [`pack_format::parse_directory`]'s guarantee that entry
    /// ids are strictly ascending and unique.
    fn find(&self, id: &str) -> Result<&DirectoryEntry, PackError> {
        self.entries
            .binary_search_by(|e| e.id.as_str().cmp(id))
            .map(|i| &self.entries[i])
            .map_err(|_| PackError::UnknownAsset(id.to_owned()))
    }

    fn payload(&self, entry: &DirectoryEntry) -> &[u8] {
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

    /// Look up an `audio/song/*` entry by its normalized pack name (e.g.
    /// `"mus_title"` — see `xtask::extract::midi` for the id scheme) and
    /// decode it through [`crate::audio::Song::decode`].
    ///
    /// # Errors
    ///
    /// [`PackError::UnknownAsset`] if no song with this name is in the
    /// pack; [`PackError::WrongKind`] if an entry exists under that id but
    /// isn't [`EntryKind::Raw`]; [`PackError::AudioDecode`] if the bytes
    /// are not a well-formed [`crate::audio::Song`] — see [`Song::decode`]
    /// for what is (and is not) validated.
    pub fn song(&self, name: &str) -> Result<Song, PackError> {
        let id = format!("audio/song/{name}");
        let bytes = self.raw(&id)?;
        Song::decode(bytes).map_err(|source| PackError::AudioDecode { id, source })
    }

    /// Look up an `audio/voicegroup/*` entry by its stable pack id (e.g. a
    /// [`Song::voicegroup`] reference — see `xtask::extract::voicegroups`
    /// for the id scheme) and decode it through
    /// [`crate::audio::VoiceGroup::decode`]. `id`'s string is already the
    /// full pack id, unlike [`song`](Self::song)'s bare name.
    ///
    /// # Errors
    ///
    /// Same as [`song`](Self::song), with [`VoiceGroup`] in place of
    /// [`Song`]. A slot's own child/sample reference is not resolved
    /// here — walk the returned [`VoiceGroup`]'s slots and call this
    /// method (or [`sample`](Self::sample)) again for each reference a
    /// caller needs to follow.
    pub fn voicegroup(&self, id: &VoiceGroupId) -> Result<VoiceGroup, PackError> {
        let bytes = self.raw(&id.0)?;
        VoiceGroup::decode(bytes).map_err(|source| PackError::AudioDecode {
            id: id.0.clone(),
            source,
        })
    }

    /// Look up an `audio/sample/*` entry by its stable pack id (see
    /// `xtask::extract::audio_samples` for the id scheme) and decode it
    /// through [`crate::audio::Sample::decode`]. `id`'s string is already
    /// the full pack id, same as [`voicegroup`](Self::voicegroup).
    ///
    /// # Errors
    ///
    /// Same as [`song`](Self::song), with [`Sample`] in place of [`Song`].
    pub fn sample(&self, id: &SampleId) -> Result<Sample, PackError> {
        let bytes = self.raw(&id.0)?;
        Sample::decode(bytes).map_err(|source| PackError::AudioDecode {
            id: id.0.clone(),
            source,
        })
    }

    /// Bundle one tileset's tile bitmap, 16 palettes, and raw metatile
    /// tables. `name` is the tileset's normalized name (e.g. `"general"`
    /// — see `xtask::extract::mod` for the id scheme).
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

        let mut palettes: [Option<PaletteRef<'_>>; TILESET_PALETTE_COUNT] =
            [None; TILESET_PALETTE_COUNT];
        for (slot, palette) in palettes.iter_mut().enumerate() {
            *palette = Some(self.palette(&format!("tileset/{name}/palette/{slot:02}"))?);
        }
        #[expect(
            clippy::missing_panics_doc,
            reason = "every slot is populated by the loop above"
        )]
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

    /// Look up a sprite palette by name: `"brendan"`/`"may"` (the two
    /// player characters' own in-game palettes) or `"npc_1"`..`"npc_4"`
    /// (the four generic NPC palette banks object-event rendering draws
    /// from) — see `xtask::extract::mod` for exactly which palettes the
    /// pack extracts.
    ///
    /// # Errors
    ///
    /// Same as [`palette`](Self::palette).
    pub fn sprite_palette(&self, who: &str) -> Result<PaletteRef<'_>, PackError> {
        self.palette(&format!("sprite/palette/{who}"))
    }

    /// Look up a map layout's grid bytes (`map.bin`) by its normalized
    /// pack name (e.g. `"littleroot_town"` — see `xtask::extract::mod`
    /// for the id scheme).
    ///
    /// Hand the returned bytes to
    /// [`MapLayout::grid`](crate::map_layouts::MapLayout::grid) or
    /// [`LayoutGrid::new`](crate::map_layouts::LayoutGrid::new) to
    /// decode; this method only fetches bytes, it never decodes them
    /// itself.
    ///
    /// # Errors
    ///
    /// Same as [`raw`](Self::raw).
    pub fn layout_map(&self, name: &str) -> Result<&[u8], PackError> {
        self.raw(&format!("layout/{name}/map"))
    }

    /// Look up a map layout's border bytes (`border.bin`) by its
    /// normalized pack name. Hand the returned bytes to
    /// [`BorderGrid::new`](crate::map_layouts::BorderGrid::new) to decode.
    ///
    /// # Errors
    ///
    /// Same as [`raw`](Self::raw).
    pub fn layout_border(&self, name: &str) -> Result<&[u8], PackError> {
        self.raw(&format!("layout/{name}/border"))
    }

    /// Look up a Latin font's glyph sheet by its typed identity. The
    /// returned [`FontImageRef`] is bound to that identity, preventing a
    /// caller from combining one font's pixels with another font's width
    /// table. Hand it to
    /// [`FontGlyphSheet::new`](crate::fonts::FontGlyphSheet::new) to
    /// decode individual glyphs; this method only fetches and
    /// identity-binds the raw sheet bitmap.
    ///
    /// # Errors
    ///
    /// Same as [`image`](Self::image).
    pub fn font(&self, font: FontId) -> Result<FontImageRef<'_>, PackError> {
        let image = self.image(&format!("font/{}/glyphs", font.pack_name()))?;
        Ok(FontImageRef::new(font, image))
    }

    /// Bundle one text-window border frame's tile bitmap and palette.
    /// `frame_id` is Emerald's zero-based `sWindowFrames` index (`0..=19`),
    /// translated to the one-based source filenames `1.png`..`20.png`.
    /// Like upstream `GetWindowFrameTilesPal`, an id at or above
    /// `WINDOW_FRAMES_COUNT` falls back to frame id `0`. The palette
    /// comes from the source PNG's own `PLTE` chunk, not a sibling `.pal`
    /// file — see `xtask::extract::png::decode_palette`.
    ///
    /// # Errors
    ///
    /// [`PackError::UnknownAsset`] if the selected frame is absent from
    /// the pack; the same [`PackError::WrongKind`] cases as
    /// [`image`](Self::image) / [`palette`](Self::palette);
    /// [`PackError::MalformedTextWindowPalette`] if the palette entry
    /// isn't the exact bank [`WindowFrameHandle`] documents;
    /// [`PackError::TextWindowImageWrongDimensions`] if the tile bitmap
    /// isn't 24x24; [`PackError::MalformedTextWindowImage`] if its
    /// payload length disagrees with its declared `width * height`;
    /// [`PackError::TextWindowPixelOutsidePalette`] if it holds a pixel
    /// index its bundled palette cannot map.
    pub fn text_window_frame(&self, frame_id: u8) -> Result<WindowFrameHandle<'_>, PackError> {
        const WINDOW_FRAMES_COUNT: u8 = 20;
        const FIRST_SOURCE_NUMBER: u8 = 1;
        let source_number = if frame_id < WINDOW_FRAMES_COUNT {
            frame_id + FIRST_SOURCE_NUMBER
        } else {
            FIRST_SOURCE_NUMBER
        };
        self.window_frame(
            &format!("text-window/image/{source_number}"),
            &format!("text-window/palette/{source_number}"),
            FRAME_WIDTH,
            FRAME_HEIGHT,
        )
    }

    /// Bundle the default message-box tile bitmap and palette (upstream
    /// `gMessageBox_Gfx`/`gMessageBox_Pal`) — the frame every standard
    /// overworld/battle text box uses, distinct from the 20 selectable
    /// [`text_window_frame`](Self::text_window_frame) options.
    ///
    /// # Errors
    ///
    /// Same as [`text_window_frame`](Self::text_window_frame).
    pub fn message_box(&self) -> Result<WindowFrameHandle<'_>, PackError> {
        self.window_frame(
            "text-window/image/message_box",
            "text-window/palette/message_box",
            MESSAGE_BOX_WIDTH,
            MESSAGE_BOX_HEIGHT,
        )
    }

    /// Look up textbox colour palette `n` (upstream
    /// `sTextWindowPalettes[n]`). `n` is passed straight through to the
    /// pack id; the pack only extracts `text_pal1`..`text_pal4` (upstream's
    /// four "extra" banks), so `n = 0` misses the pack rather than
    /// returning [`message_box`](Self::message_box)'s own palette, which
    /// is stored under a different id.
    ///
    /// # Errors
    ///
    /// Same as [`text_window_frame`](Self::text_window_frame)'s palette
    /// cases.
    pub fn text_window_extra_palette(&self, n: u8) -> Result<PaletteRef<'_>, PackError> {
        self.text_window_palette(&format!("text-window/palette/text_pal{n}"))
    }

    /// Enforce the one-GBA-bank shape every text-window accessor's
    /// palette must have. Extraction validates this on write, but the
    /// read side must not trust pack metadata — a corrupt or hand-built
    /// pack could otherwise defeat [`WindowFrameHandle`]'s invariant.
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

    /// Bundle a text-window frame's tile bitmap and palette, re-checking
    /// the pair on read (dimensions, declared pixel count, and every
    /// pixel index against the palette) so a corrupt or hand-built pack
    /// can't reach a renderer and index out of bounds, even though
    /// extraction enforces the same pairing on write.
    fn window_frame(
        &self,
        image_id: &str,
        palette_id: &str,
        expected_width: u32,
        expected_height: u32,
    ) -> Result<WindowFrameHandle<'_>, PackError> {
        let tiles = self.image(image_id)?;
        if tiles.width != expected_width || tiles.height != expected_height {
            return Err(PackError::TextWindowImageWrongDimensions {
                id: image_id.to_owned(),
                width: tiles.width,
                height: tiles.height,
                expected_width,
                expected_height,
            });
        }
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
        actual: kind_label(actual),
    }
}

#[cfg(test)]
mod tests;
