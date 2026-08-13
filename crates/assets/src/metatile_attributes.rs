//! Metatile attribute decode (S-4): typed access over a tileset's
//! `metatile_attributes.bin` bytes.
//!
//! Each tileset's `metatile_attributes.bin` (bundled raw, undecoded, as
//! [`TilesetHandle::metatile_attributes`](crate::pack::TilesetHandle::metatile_attributes)
//! — extraction of the file itself is `cargo xtask extract`'s job, per
//! Discussion #71 policy A; see `crate::pack`'s and
//! `crate::map_layouts`'s module docs for the identical situation with
//! `metatiles.bin`/`map.bin`) is a flat array of little-endian `u16`, one
//! per metatile, indexed by that tileset's local metatile id — the same id
//! [`MetatileCell::metatile_id`](crate::map_layouts::MetatileCell::metatile_id)
//! carries.
//!
//! Bit layout (upstream `include/global.fieldmap.h`,
//! `METATILE_ATTR_*_MASK`/`_SHIFT`):
//! - bits 0-7: **behavior** (`METATILE_ATTR_BEHAVIOR_MASK`, `0x00FF`) — the
//!   upstream `MB_*` behavior id (`constants/metatile_behaviors.h` defines
//!   around 200 of them, e.g. tall grass, ice, a warp). Kept as a raw `u8`
//!   rather than modelled as an enum of every `MB_*` value — matching how
//!   [`MetatileCell::collision`](crate::map_layouts::MetatileCell::collision)
//!   / `elevation` also stay raw bit-fields rather than closed enums;
//!   decoding the full behavior table is future movement/interaction-system
//!   work, out of scope here.
//! - bits 8-11: unused.
//! - bits 12-15: **layer type** (`METATILE_ATTR_LAYER_MASK`, `0xF000`) —
//!   which two of the three background layers this metatile draws into
//!   ([`MetatileLayerType`]).

use crate::error::AssetError;

/// The three upstream `METATILE_LAYER_TYPE_*` values
/// (`pokeemerald/include/global.fieldmap.h`): which two of the three
/// background layers a metatile draws into.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetatileLayerType {
    /// `METATILE_LAYER_TYPE_NORMAL` (0): middle and top bg layers.
    Normal = 0,
    /// `METATILE_LAYER_TYPE_COVERED` (1): bottom and middle bg layers.
    Covered = 1,
    /// `METATILE_LAYER_TYPE_SPLIT` (2): bottom and top bg layers.
    Split = 2,
}

impl MetatileLayerType {
    /// Decode a raw 4-bit layer-type value (already shifted down to
    /// `0..=15`).
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::UnknownMetatileLayerType`] if `raw` is not
    /// `0..=2` — upstream never emits another value, though nothing in the
    /// file format itself rules one out, so this stays a real (if
    /// practically unreachable against real upstream data) failure mode
    /// rather than a silent default.
    pub const fn from_raw(raw: u8) -> Result<Self, AssetError> {
        match raw {
            0 => Ok(Self::Normal),
            1 => Ok(Self::Covered),
            2 => Ok(Self::Split),
            other => Err(AssetError::UnknownMetatileLayerType(other)),
        }
    }
}

/// One decoded metatile-attribute entry — the unpacked form of a raw `u16`
/// in a tileset's `metatile_attributes.bin` (upstream `UNPACK_BEHAVIOR` /
/// `UNPACK_LAYER_TYPE`, `include/global.fieldmap.h`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MetatileAttribute {
    /// The metatile's behavior id (bits 0-7, `METATILE_ATTR_BEHAVIOR_MASK`):
    /// the raw upstream `MB_*` value, not decoded further (see the module
    /// docs).
    pub behavior: u8,
    /// Which background layers this metatile draws into (bits 12-15,
    /// `METATILE_ATTR_LAYER_MASK`).
    pub layer_type: MetatileLayerType,
}

impl MetatileAttribute {
    /// Decode a raw packed `u16` attribute entry.
    ///
    /// Only the modeled fields — behavior (bits 0-7) and layer type (bits
    /// 12-15) — are extracted; the unused bits 8-11 are ignored.
    ///
    /// # Errors
    ///
    /// See [`MetatileLayerType::from_raw`].
    // The `as u8` cast is lossless: masking to `0x00FF` leaves at most 8
    // significant bits, always `<= u8::MAX`.
    #[allow(clippy::cast_possible_truncation)]
    pub const fn from_raw(raw: u16) -> Result<Self, AssetError> {
        let behavior = (raw & 0x00FF) as u8;
        let layer_type_raw = ((raw & 0xF000) >> 12) as u8;
        match MetatileLayerType::from_raw(layer_type_raw) {
            Ok(layer_type) => Ok(Self {
                behavior,
                layer_type,
            }),
            Err(e) => Err(e),
        }
    }

    /// Repack the modeled fields — behavior (bits 0-7) and layer type
    /// (bits 12-15) — into a `u16`. The unused bits 8-11 are always zero,
    /// so this does not reconstruct the exact original raw value if those
    /// bits were nonzero.
    // `as u16` widens u8 -> u16 (lossless).
    #[allow(clippy::cast_lossless)]
    #[must_use]
    pub const fn pack(self) -> u16 {
        (self.behavior as u16) | ((self.layer_type as u16) << 12)
    }
}

/// A borrowed, validated view over a tileset's decoded
/// `metatile_attributes.bin` bytes, indexed by local metatile id.
///
/// Wraps caller-supplied bytes — this module ships no attribute bytes of
/// its own; the bytes come from the local, gitignored asset pack (`cargo
/// xtask extract`; see the module docs). Build one from
/// [`TilesetHandle::metatile_attributes`](crate::pack::TilesetHandle::metatile_attributes)'s
/// raw bytes.
#[derive(Debug, Clone, Copy)]
pub struct MetatileAttributeTable<'a> {
    bytes: &'a [u8],
}

impl<'a> MetatileAttributeTable<'a> {
    /// Build a view over `bytes` (a `metatile_attributes.bin`-shaped
    /// buffer). Never fails at construction time — an odd-length trailing
    /// byte (which never occurs in real upstream files) simply isn't
    /// reachable by any whole-cell index; decode errors (an out-of-range
    /// layer-type value) only surface once a specific entry is read.
    #[must_use]
    pub const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }

    /// The number of whole `u16` entries available.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.bytes.len() / 2
    }

    /// Whether this table has no entries.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The decoded attribute entry for `metatile_id`, or `None` if that id
    /// is out of range for this table.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::UnknownMetatileLayerType`] if the entry exists
    /// but its layer-type bits don't decode (see
    /// [`MetatileAttribute::from_raw`]).
    #[must_use]
    pub fn attribute_at(&self, metatile_id: u16) -> Option<Result<MetatileAttribute, AssetError>> {
        let offset = usize::from(metatile_id) * 2;
        let bytes = self.bytes.get(offset..offset + 2)?;
        Some(MetatileAttribute::from_raw(u16::from_le_bytes([
            bytes[0], bytes[1],
        ])))
    }

    /// Every decoded attribute entry, in upstream storage order (index 0 is
    /// metatile id 0, and so on).
    pub fn attributes(&self) -> impl Iterator<Item = Result<MetatileAttribute, AssetError>> + 'a {
        self.bytes
            .chunks_exact(2)
            .map(|b| MetatileAttribute::from_raw(u16::from_le_bytes([b[0], b[1]])))
    }
}

#[cfg(test)]
mod tests {
    use super::{MetatileAttribute, MetatileAttributeTable, MetatileLayerType};
    use crate::error::AssetError;

    #[test]
    fn layer_type_decodes_known_values() {
        assert_eq!(
            MetatileLayerType::from_raw(0),
            Ok(MetatileLayerType::Normal)
        );
        assert_eq!(
            MetatileLayerType::from_raw(1),
            Ok(MetatileLayerType::Covered)
        );
        assert_eq!(MetatileLayerType::from_raw(2), Ok(MetatileLayerType::Split));
    }

    #[test]
    fn layer_type_rejects_unknown_values() {
        for raw in [3u8, 4, 15] {
            assert_eq!(
                MetatileLayerType::from_raw(raw),
                Err(AssetError::UnknownMetatileLayerType(raw))
            );
        }
    }

    #[test]
    fn metatile_attribute_round_trips_through_pack() {
        for raw in [0x0000u16, 0x00FF, 0x1000, 0x2001, 0x00AB] {
            let attr = MetatileAttribute::from_raw(raw).unwrap();
            assert_eq!(attr.behavior, (raw & 0x00FF) as u8);
            assert_eq!(attr.pack(), raw);
        }
    }

    #[test]
    fn metatile_attribute_rejects_unknown_layer_type_bits() {
        // Layer type bits (12-15) = 3, an upstream-unused value.
        let raw = 0x3000u16;
        assert_eq!(
            MetatileAttribute::from_raw(raw),
            Err(AssetError::UnknownMetatileLayerType(3))
        );
    }

    #[test]
    fn table_decodes_row_major_entries() {
        let raws: [u16; 3] = [0x0001, 0x1002, 0x2003];
        let mut bytes = Vec::new();
        for raw in raws {
            bytes.extend_from_slice(&raw.to_le_bytes());
        }
        let table = MetatileAttributeTable::new(&bytes);
        assert_eq!(table.len(), 3);
        assert!(!table.is_empty());

        let decoded: Vec<_> = table.attributes().map(Result::unwrap).collect();
        assert_eq!(decoded.len(), 3);
        for (i, raw) in raws.iter().enumerate() {
            assert_eq!(decoded[i], MetatileAttribute::from_raw(*raw).unwrap());
        }

        assert_eq!(
            table.attribute_at(0).unwrap().unwrap(),
            MetatileAttribute::from_raw(raws[0]).unwrap()
        );
        assert_eq!(
            table.attribute_at(2).unwrap().unwrap(),
            MetatileAttribute::from_raw(raws[2]).unwrap()
        );
    }

    #[test]
    fn table_attribute_at_out_of_range_is_none() {
        let bytes = [0u8; 4]; // 2 entries
        let table = MetatileAttributeTable::new(&bytes);
        assert!(table.attribute_at(2).is_none());
        assert!(table.attribute_at(1000).is_none());
    }

    #[test]
    fn table_attribute_at_bad_layer_type_is_an_error() {
        let raw = 0x3000u16; // layer type bits = 3
        let bytes = raw.to_le_bytes();
        let table = MetatileAttributeTable::new(&bytes);
        assert_eq!(
            table.attribute_at(0),
            Some(Err(AssetError::UnknownMetatileLayerType(3)))
        );
    }

    #[test]
    fn empty_table_has_no_entries() {
        let table = MetatileAttributeTable::new(&[]);
        assert_eq!(table.len(), 0);
        assert!(table.is_empty());
        assert!(table.attribute_at(0).is_none());
        assert_eq!(table.attributes().count(), 0);
    }
}
