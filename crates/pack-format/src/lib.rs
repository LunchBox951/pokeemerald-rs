//! The asset-pack container format (S-4, F-3, issue #81 / Discussion #71
//! policy A): the writer-side owner of the layout `cargo xtask extract`
//! writes. `crates/assets` still mirrors the reader side; its migration is next.
//!
//! # Format (version 6, wire layout unchanged since 1)
//!
//! All multi-byte integers are little-endian. Layout, in order:
//!
//! ```text
//! Header:
//!   magic:        [u8; 8]   = b"PKMRPACK"
//!   format_version: u32
//!   entry_count:  u32
//!
//! Directory (entry_count entries, sorted ascending by `id` as raw bytes):
//!   for each entry:
//!     id_len:  u16
//!     id:      [u8; id_len]      UTF-8, normalized asset id (see below)
//!     kind:    u8                0 = Image, 1 = Palette, 2 = Raw
//!     offset:  u64                absolute byte offset of the payload
//!     length:  u64                payload length in bytes
//!     -- kind-specific fixed metadata --
//!     Image:   width: u32, height: u32, bit_depth: u8
//!     Palette: color_count: u16
//!     Raw:     (none)
//!
//! Payload region:
//!   the concatenation of every entry's payload bytes, in directory order,
//!   starting immediately after the last directory entry (so the first
//!   entry's `offset` equals the header+directory size).
//! ```
//!
//! Payload shapes:
//! - **Image**: `width * height` bytes, one palette-index byte per pixel,
//!   row-major (see `xtask::extract::png`).
//! - **Palette**: `color_count * 2` bytes, one GBA-native packed BGR555
//!   `u16` (little-endian) per colour (see `xtask::extract::jasc_pal`).
//! - **Raw**: opaque bytes, copied verbatim from the upstream source file
//!   (used for content the extract pipeline doesn't decode, e.g.
//!   `metatiles.bin` — see `xtask::extract`'s module docs for the exact
//!   list).
//!
//! # Determinism
//!
//! Byte-for-byte reproducibility across runs (on the same upstream ref) is
//! a hard requirement (issue #81's Definition of Done). This format and its
//! writer avoid every common source of nondeterminism:
//! - **No timestamps, no host/filesystem metadata** anywhere in the format.
//! - **Directory entries are sorted by id** ([`PackWriter::finish`]) rather
//!   than written in insertion order, so callers walking directories with
//!   `std::fs::read_dir` (whose iteration order is *not* guaranteed by std)
//!   can't perturb the output — see `xtask::extract`'s directory-walk
//!   helpers, which additionally sort every `read_dir` listing before use
//!   as defence in depth.
//! - **No hashmap iteration**: the writer collects entries into a `Vec` and
//!   sorts it; nothing here is ever iterated from a `HashMap`.
//!
//! # Asset ids: normalized, not decomp-shaped
//!
//! Per the owner's Discussion #71 decision, ids name *what the asset is*
//! (e.g. `"tileset/general/tiles"`, `"sprite/brendan/walking"`), not
//! upstream's linker symbols (`gTilesetTiles_General`) or raw source paths
//! (`data/tilesets/primary/general/tiles.png`). The intent (spelled out in
//! the discussion) is that a future ROM-backed extractor can produce the
//! exact same ids from a completely different input, so `crates/assets`'s
//! pack consumer never has to change when policy A (the decomp-checkout
//! backend) is eventually joined or replaced by policy C (a ROM-backed
//! `--import-rom` backend). See `xtask::extract`'s module docs for the
//! concrete id scheme that pipeline uses.
//!
//! # Status
//!
//! This crate holds the format constants ([`MAGIC`], [`FORMAT_VERSION`],
//! [`OUTPUT_RELATIVE_PATH`], [`EntryKind`]) and the write side
//! ([`PackEntry`], [`PackWriter`], [`PackWriteError`]). `xtask::extract` is
//! its only consumer so far.
//!
//! Next: `crates/assets`'s reader moves here too, so the two sides stop
//! mirroring one layout in two places. After that, entry constructors
//! (typed helpers replacing hand-built [`PackEntry`] literals at every call
//! site) and the default pack path (`AssetPack::default_path`'s repo-root
//! join, currently derived independently on each side).

mod layout;
mod writer;

pub use layout::{EntryKind, FORMAT_VERSION, MAGIC, OUTPUT_RELATIVE_PATH};
pub use writer::{PackEntry, PackWriteError, PackWriter};
