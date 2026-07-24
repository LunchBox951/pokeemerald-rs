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
//! player/NPC sprite sheets and the two player palettes. The lower-level
//! [`AssetPack::image`] / [`AssetPack::palette`] / [`AssetPack::raw`]
//! accessors work over any entry by its full id (used directly for e.g.
//! `title/image/*` entries, which have no bundling handle of their own).
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

pub use error::PackError;
pub use format::{EntryKind, FORMAT_VERSION, MAGIC};
pub use handles::{ImageRef, PaletteRef, TilesetHandle};

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
