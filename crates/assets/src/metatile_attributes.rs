//! Typed access to a tileset's borrowed `metatile_attributes.bin` bytes.
//!
//! Each entry is a little-endian `u16` indexed by the local metatile identity
//! carried by [`MetatileCell::metatile_id`](crate::map_layouts::MetatileCell::metatile_id).
//! Upstream `include/global.fieldmap.h` assigns bits 0 through 7 to a terrain
//! behaviour identity, leaves bits 8 through 11 unused, and assigns bits 12
//! through 15 to [`MetatileLayerType`]. Behaviour identities remain raw because
//! the assets crate does not own their movement and interaction semantics. A
//! behaviour belongs to the tileset entry; placement-specific collision belongs
//! to [`MetatileCell::collision`](crate::map_layouts::MetatileCell::collision).

use crate::error::AssetError;
use std::mem::size_of;

const BYTES_PER_METATILE_ATTRIBUTE: usize = size_of::<u16>();
const METATILE_BEHAVIOR_MASK: u16 = 0x00FF;
const METATILE_LAYER_TYPE_MASK: u16 = 0xF000;
const METATILE_LAYER_TYPE_SHIFT: u32 = 12;

/// The pair of background layers used to draw a metatile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetatileLayerType {
    /// Middle and top background layers.
    Normal = 0,
    /// Bottom and middle background layers.
    Covered = 1,
    /// Bottom and top background layers.
    Split = 2,
}

impl MetatileLayerType {
    /// Decodes a four-bit layer-type value.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::UnknownMetatileLayerType`] when `raw` is not one
    /// of the three defined layer types. The four-bit field can represent
    /// other values, so malformed data fails instead of selecting a default.
    pub const fn from_raw(raw: u8) -> Result<Self, AssetError> {
        match raw {
            0 => Ok(Self::Normal),
            1 => Ok(Self::Covered),
            2 => Ok(Self::Split),
            other => Err(AssetError::UnknownMetatileLayerType(other)),
        }
    }
}

/// One decoded metatile attribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MetatileAttribute {
    /// Terrain behaviour identity stored in bits 0 through 7.
    pub behavior: u8,
    /// Background-layer pair stored in bits 12 through 15.
    pub layer_type: MetatileLayerType,
}

impl MetatileAttribute {
    /// Decodes the behaviour and layer type from a packed attribute.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::UnknownMetatileLayerType`] when the packed layer
    /// value is not defined.
    pub const fn from_raw(raw: u16) -> Result<Self, AssetError> {
        let behavior = (raw & METATILE_BEHAVIOR_MASK) as u8;
        let layer_type_raw = ((raw & METATILE_LAYER_TYPE_MASK) >> METATILE_LAYER_TYPE_SHIFT) as u8;
        match MetatileLayerType::from_raw(layer_type_raw) {
            Ok(layer_type) => Ok(Self {
                behavior,
                layer_type,
            }),
            Err(e) => Err(e),
        }
    }

    /// Packs the behaviour and layer type into their 16-bit representation.
    /// Bits 8 through 11 are always zero.
    #[must_use]
    pub const fn pack(self) -> u16 {
        (self.behavior as u16) | ((self.layer_type as u16) << METATILE_LAYER_TYPE_SHIFT)
    }
}

/// A view borrowing encoded attributes for `'a`, indexed by local metatile
/// identity.
#[derive(Debug, Clone, Copy)]
pub struct MetatileAttributeTable<'a> {
    bytes: &'a [u8],
}

impl<'a> MetatileAttributeTable<'a> {
    /// Builds a view over caller-owned bytes without eagerly validating them.
    ///
    /// A trailing partial entry remains inaccessible. Layer types are validated
    /// when their entries are read.
    #[must_use]
    pub const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }

    /// Returns the number of complete attributes.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.bytes.len() / BYTES_PER_METATILE_ATTRIBUTE
    }

    /// Returns whether the table contains no complete attributes.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the decoded attribute for `metatile_id`, or `None` when the
    /// identity is outside the table.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::UnknownMetatileLayerType`] when the entry contains
    /// an undefined layer type.
    #[must_use]
    pub fn attribute_at(&self, metatile_id: u16) -> Option<Result<MetatileAttribute, AssetError>> {
        let offset = usize::from(metatile_id) * BYTES_PER_METATILE_ATTRIBUTE;
        let bytes = self
            .bytes
            .get(offset..offset + BYTES_PER_METATILE_ATTRIBUTE)?;
        Some(MetatileAttribute::from_raw(u16::from_le_bytes([
            bytes[0], bytes[1],
        ])))
    }

    /// Iterates over complete attributes in local metatile identity order.
    pub fn attributes(&self) -> impl Iterator<Item = Result<MetatileAttribute, AssetError>> + 'a {
        self.bytes
            .chunks_exact(BYTES_PER_METATILE_ATTRIBUTE)
            .map(|b| MetatileAttribute::from_raw(u16::from_le_bytes([b[0], b[1]])))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MetatileAttribute, MetatileAttributeTable, MetatileLayerType, BYTES_PER_METATILE_ATTRIBUTE,
        METATILE_BEHAVIOR_MASK, METATILE_LAYER_TYPE_MASK, METATILE_LAYER_TYPE_SHIFT,
    };
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
        for behavior in u8::MIN..=u8::MAX {
            for layer_type in [
                MetatileLayerType::Normal,
                MetatileLayerType::Covered,
                MetatileLayerType::Split,
            ] {
                let attribute = MetatileAttribute {
                    behavior,
                    layer_type,
                };
                assert_eq!(MetatileAttribute::from_raw(attribute.pack()), Ok(attribute));
            }
        }
    }

    #[test]
    fn metatile_attribute_pack_zeroes_the_unused_bits() {
        let unused_bits = !(METATILE_BEHAVIOR_MASK | METATILE_LAYER_TYPE_MASK);
        let raw = unused_bits | 1;
        let attribute = MetatileAttribute::from_raw(raw).unwrap();
        assert_eq!(attribute.pack(), 1);
    }

    #[test]
    fn metatile_attribute_rejects_unknown_layer_type_bits() {
        const UNKNOWN_LAYER_TYPE: u8 = 3;
        let raw = u16::from(UNKNOWN_LAYER_TYPE) << METATILE_LAYER_TYPE_SHIFT;
        assert_eq!(
            MetatileAttribute::from_raw(raw),
            Err(AssetError::UnknownMetatileLayerType(UNKNOWN_LAYER_TYPE))
        );
    }

    #[test]
    fn table_decodes_entries_in_local_metatile_identity_order() {
        let attributes = [
            MetatileAttribute {
                behavior: 1,
                layer_type: MetatileLayerType::Normal,
            },
            MetatileAttribute {
                behavior: 2,
                layer_type: MetatileLayerType::Covered,
            },
            MetatileAttribute {
                behavior: 3,
                layer_type: MetatileLayerType::Split,
            },
        ];
        let mut bytes = Vec::new();
        for attribute in attributes {
            bytes.extend_from_slice(&attribute.pack().to_le_bytes());
        }
        let table = MetatileAttributeTable::new(&bytes);
        assert_eq!(table.len(), attributes.len());
        assert!(!table.is_empty());

        let decoded: Vec<_> = table.attributes().map(Result::unwrap).collect();
        assert_eq!(decoded, attributes);
        assert_eq!(table.attribute_at(0), Some(Ok(attributes[0])));
        assert_eq!(table.attribute_at(2), Some(Ok(attributes[2])));
    }

    #[test]
    fn table_attribute_at_out_of_range_is_none() {
        let bytes = [0u8; BYTES_PER_METATILE_ATTRIBUTE * 2];
        let table = MetatileAttributeTable::new(&bytes);
        assert!(table.attribute_at(2).is_none());
        assert!(table.attribute_at(1000).is_none());
    }

    #[test]
    fn table_attribute_at_bad_layer_type_is_an_error() {
        const UNKNOWN_LAYER_TYPE: u8 = 3;
        let raw = u16::from(UNKNOWN_LAYER_TYPE) << METATILE_LAYER_TYPE_SHIFT;
        let bytes = raw.to_le_bytes();
        let table = MetatileAttributeTable::new(&bytes);
        assert_eq!(
            table.attribute_at(0),
            Some(Err(AssetError::UnknownMetatileLayerType(
                UNKNOWN_LAYER_TYPE
            )))
        );
    }

    #[test]
    fn table_ignores_a_trailing_partial_attribute() {
        let bytes = [0; BYTES_PER_METATILE_ATTRIBUTE + 1];
        let table = MetatileAttributeTable::new(&bytes);
        assert_eq!(table.len(), 1);
        assert_eq!(table.attributes().count(), 1);
        assert_eq!(table.attribute_at(1), None);
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
