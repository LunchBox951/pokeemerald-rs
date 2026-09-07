//! The asset-pack container format (S-4, F-3, issue #81 / Discussion #71
//! policies A and C): the one owner of the layout `cargo xtask extract` and
//! `pokeemerald-rs --import-rom` write and `crates/assets` reads.
//!
//! # Format (version 7)
//!
//! All multi-byte integers are little-endian. Layout, in order:
//!
//! ```text
//! Header:
//!   magic:        [u8; 8]   = b"PKMRPACK"
//!   format_version: u32
//!   entry_count:  u32
//!
//! Directory (entry_count entries, strictly ascending and unique by `id` as
//! raw bytes):
//!   for each entry:
//!     id_len:  u16
//!     id:      [u8; id_len]      UTF-8, normalized asset id (see below)
//!     kind:    u8                0 = Image, 1 = Palette, 2 = Raw
//!     offset:  u64                absolute byte offset of the payload
//!     length:  u64                payload length in bytes
//!     -- kind-specific fixed metadata --
//!     Image:   width: u32, height: u32, bit_depth: u8 (2, 4, or 8)
//!     Palette: color_count: u16
//!     Raw:     (none)
//!
//! Payload region:
//!   the concatenation of every entry's payload bytes, in directory order,
//!   starting immediately after the last directory entry (so the first
//!   entry's `offset` equals the header+directory size) and ending at the
//!   end of the file. No padding, no gaps, no trailing bytes.
//! ```
//!
//! That arrangement is a structural contract rather than a writer
//! convention: [`parse_directory`] rejects a pack whose payloads sit
//! anywhere else, so slicing an entry's `offset`/`length` yields that
//! entry's bytes and not whatever a corrupt offset pointed at.
//!
//! Payload shapes, which [`parse_directory`] enforces for the same reason
//! and so a consumer may read a payload by the metadata beside it:
//! - **Image**: `width * height` bytes, one palette-index byte per pixel,
//!   row-major (see `xtask::extract::png`).
//! - **Palette**: `color_count * 2` bytes, one GBA-native packed BGR555
//!   `u16` (little-endian) per colour (see `xtask::extract::jasc_pal`).
//! - **Raw**: opaque *to this container*. The writer validates nothing and
//!   [`parse_directory`]'s reader hands the bytes back unread, so the schema
//!   a payload must hold is not defined here — it belongs to the asset type
//!   in `crates/assets` that decodes that id family, and a backend owes that
//!   encoding rather than whatever bytes its own source happened to store.
//!   `audio/song/*`, `audio/sample/*`, and `audio/voicegroup/*` carry
//!   `assets::audio`'s `Song`/`Sample`/`VoiceGroup` `encode` output, read
//!   back through the matching `AssetPack` accessor's `decode`;
//!   `tileset/*/metatiles`, the `map`/`border` layout grids, and
//!   `metatile_attributes` carry the little-endian cell arrays
//!   `assets::map_layouts` and `assets::metatile_attributes` define, which
//!   upstream happens to ship as flat files of the same bytes — which is
//!   why the decomp backend can copy those through and the ROM backend
//!   cannot. See `xtask::extract`'s module docs for the id families that
//!   pipeline writes.
//!
//! # Determinism
//!
//! Byte-for-byte reproducibility across runs (on the same upstream ref) is
//! a hard requirement. This format and its writer avoid every common
//! source of nondeterminism:
//! - **No timestamps, no host/filesystem metadata** anywhere in the format.
//! - **Directory entries are sorted by id** ([`PackWriter::finish`]) rather
//!   than written in insertion order, so callers walking directories with
//!   `std::fs::read_dir` (whose iteration order is *not* guaranteed by std)
//!   can't perturb the output — see `xtask::extract`'s directory-walk
//!   helpers, which additionally sort every `read_dir` listing before use
//!   as defence in depth. This is also a structural contract, not only a
//!   determinism aid: ids must be strictly ascending and unique, and
//!   [`parse_directory`] rejects any pack whose directory is not, so a
//!   consumer may binary-search the result without re-checking it.
//! - **No hashmap iteration**: the writer collects entries into a `Vec` and
//!   sorts it; nothing here is ever iterated from a `HashMap`.
//!
//! # Asset ids: normalized, not decomp-shaped
//!
//! Ids name *what the asset is* (e.g. `"tileset/general/tiles"`,
//! `"sprite/brendan/walking"`), not upstream's linker symbols
//! (`gTilesetTiles_General`) or raw source paths
//! (`data/tilesets/primary/general/tiles.png`) — so a consumer never has to
//! change when the extractor backend producing them does. See
//! `xtask::extract`'s module docs for the concrete id scheme that pipeline
//! uses.
//!
//! # Status
//!
//! This crate holds the format constants ([`MAGIC`], [`FORMAT_VERSION`],
//! [`OUTPUT_RELATIVE_PATH`], [`EntryKind`]), the write side ([`PackEntry`],
//! [`PackWriter`], [`PackWriteError`]), the entry constructors
//! ([`palette_entry`], [`image_entry`], [`image_entry_from_tiles`],
//! [`tiles_from_image`], [`raw_entry`], [`EntryShapeError`]), the read side
//! ([`parse_directory`], [`DirectoryEntry`], [`PackReadError`]), and runtime
//! pack-path resolution
//! ([`default_pack_path`], [`repo_pack_path`], [`user_data_dir`],
//! [`user_pack_path`], [`PACK_PATH_ENV`]). `xtask::extract` writes through
//! it; `crates/assets`'s `AssetPack` reads through it.
//!
//! One crate owns the layout so its writers (`xtask::extract`, the ROM
//! importer) and its reader (`crates/assets`) cannot drift: a format bump
//! touches one file, and `xtask` and `assets` never depend on each other.
//!
//! The entry constructors are what makes two backends produce one pack. A
//! hand-built [`PackEntry`] literal can promise a `color_count` or a
//! `width`/`height` its payload does not deliver, and two backends writing
//! their own literals drift. Both now shape entries here, so the same
//! normalized input yields the same bytes whichever backend read it. See
//! the `entry` module docs.
//!
//! [`default_pack_path`] resolves at runtime, first match wins:
//! 1. `$POKEEMERALD_PACK`, if set and non-empty.
//! 2. The OS user-data directory's `pokeemerald-rs/pokeemerald.pack`, if it
//!    exists; the shipped ROM importer writes there ([`user_pack_path`]).
//! 3. `<directory of the running executable>/`[`OUTPUT_RELATIVE_PATH`], if
//!    it exists, for portable installs.
//! 4. [`repo_pack_path`], the compile-time repo path, so a developer
//!    checkout keeps working with nothing configured.
//!
//! Rungs 2 and 3 advance only when the candidate is *known* absent. One
//! that cannot be examined — an unsearchable directory component — stops
//! resolution and is handed back, so the loader reports the permission
//! failure at the pack the player installed rather than a missing-file
//! error naming some other path.
//!
//! That order is right for a *running game* and wrong for a gate that means
//! to validate this checkout: rungs 1 and 2 are the very destinations
//! `--import-rom` writes to, so a checkout gate resolving through
//! [`default_pack_path`] would read whichever pack the developer has
//! installed rather than the one `cargo xtask extract` just wrote. Such
//! gates call [`repo_pack_path`] by name instead `(test-ratchet)`.

mod entry;
mod layout;
mod path;
mod reader;
mod writer;

pub use entry::{
    image_entry, image_entry_from_tiles, palette_entry, raw_entry, tiles_from_image,
    EntryShapeError,
};
pub use layout::{EntryKind, FORMAT_VERSION, MAGIC, OUTPUT_RELATIVE_PATH};
pub use path::{default_pack_path, repo_pack_path, user_data_dir, user_pack_path, PACK_PATH_ENV};
pub use reader::{parse_directory, DirectoryEntry, PackReadError};
pub use writer::{PackEntry, PackWriteError, PackWriter};
