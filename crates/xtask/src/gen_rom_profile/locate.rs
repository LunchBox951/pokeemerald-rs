//! Shared plumbing for the per-domain locators: address conversion and
//! uniqueness checks.
//!
//! Two rules hold everywhere. A root is accepted only when exactly one place
//! in the ROM holds its bytes, and an address leaves this module only as a GBA
//! bus address, never as a ROM offset.

use rom_import::{ROM_BASE, ROM_WINDOW_END};

use super::error::GenRomProfileError;

/// The GBA bus address the cartridge maps `offset` to.
pub const fn to_addr(offset: u32) -> u32 {
    ROM_BASE + offset
}

/// The ROM offset of a GBA bus address, or `None` outside the cartridge
/// window `ROM_BASE..ROM_WINDOW_END`.
///
/// The window is half-open: [`ROM_WINDOW_END`] itself is rejected, as is
/// anything below [`ROM_BASE`]. Every pointer check and bus-address read in
/// the locators goes through this bound.
pub fn to_offset(addr: u32) -> Option<usize> {
    (ROM_BASE..ROM_WINDOW_END)
        .contains(&addr)
        .then(|| (addr - ROM_BASE) as usize)
}

/// Convert the ROM offsets a search returned into one bus address, failing
/// unless there is exactly one.
///
/// # Errors
///
/// [`GenRomProfileError::NotFound`] for no hit, [`GenRomProfileError::Ambiguous`]
/// for more than one. An ambiguous root must be resolved through a struct
/// back-reference; this never picks among hits.
pub fn exactly_one(id: &str, hits: &[u32]) -> Result<u32, GenRomProfileError> {
    match hits {
        [] => Err(GenRomProfileError::NotFound { id: id.to_owned() }),
        [only] => Ok(to_addr(*only)),
        many => Err(GenRomProfileError::Ambiguous {
            id: id.to_owned(),
            addrs: many.iter().copied().map(to_addr).collect(),
        }),
    }
}

/// Read a little-endian `u32` at a ROM offset, or `None` if fewer than four
/// bytes remain. `offset + 4` must not overflow `usize`; the bus-address
/// readers guarantee that through [`to_offset`].
pub fn u32_at(rom: &[u8], offset: usize) -> Option<u32> {
    rom.get(offset..offset + 4)
        .map(|bytes| u32::from_le_bytes(bytes.try_into().expect("four bytes")))
}

/// Read a little-endian `u32` at a GBA bus address, or `None` if the address
/// or the read falls outside `rom`.
pub fn u32_at_addr(rom: &[u8], addr: u32) -> Option<u32> {
    u32_at(rom, to_offset(addr)?)
}

/// Read a little-endian `u16` at a GBA bus address, or `None` if the address
/// or the read falls outside `rom`.
pub fn u16_at_addr(rom: &[u8], addr: u32) -> Option<u16> {
    let offset = to_offset(addr)?;
    rom.get(offset..offset + 2)
        .map(|bytes| u16::from_le_bytes(bytes.try_into().expect("two bytes")))
}

/// Read one byte at a GBA bus address, or `None` outside `rom`.
pub fn u8_at_addr(rom: &[u8], addr: u32) -> Option<u8> {
    rom.get(to_offset(addr)?).copied()
}

/// Borrow `len` bytes at a GBA bus address, or `None` if the address or the
/// range falls outside `rom`.
pub fn slice_at_addr(rom: &[u8], addr: u32, len: usize) -> Option<&[u8]> {
    let offset = to_offset(addr)?;
    rom.get(offset..offset.checked_add(len)?)
}

/// Convert a normalized `snake_case` pack name to upstream's `CamelCase`
/// symbol spelling: `brendans_mays_house` becomes `BrendansMaysHouse`.
///
/// Lets the `--map` cross-check derive an upstream symbol name from a pack id
/// without a lookup table.
pub fn camel_case(snake: &str) -> String {
    let mut out = String::with_capacity(snake.len());
    for word in snake.split('_').filter(|word| !word.is_empty()) {
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            out.extend(first.to_uppercase());
            out.push_str(chars.as_str());
        }
    }
    out
}

/// The number of tiles the ROM stores for an image whose full raster is
/// `expected`.
///
/// Upstream trims all-zero trailing tiles and honours `-num_tiles`, so the
/// ROM may legitimately hold a prefix of `expected`. Any other disagreement
/// (a differing byte, a non-zero tail, a partial tile) means the locator
/// matched the wrong bytes.
///
/// # Errors
///
/// [`GenRomProfileError::StructMismatch`] naming what disagreed.
pub fn tile_count_of_prefix(
    id: &str,
    rom_tiles: &[u8],
    expected: &[u8],
    bytes_per_tile: usize,
) -> Result<u32, GenRomProfileError> {
    let mismatch = |reason: String| GenRomProfileError::StructMismatch {
        id: id.to_owned(),
        reason,
    };
    if rom_tiles.len() > expected.len() {
        return Err(mismatch(format!(
            "the ROM holds {} tile bytes, more than the {} the pack raster needs",
            rom_tiles.len(),
            expected.len()
        )));
    }
    if !rom_tiles.len().is_multiple_of(bytes_per_tile) {
        return Err(mismatch(format!(
            "{} tile bytes is not a whole number of {bytes_per_tile}-byte tiles",
            rom_tiles.len()
        )));
    }
    if rom_tiles != &expected[..rom_tiles.len()] {
        return Err(mismatch(
            "the ROM tile data differs from the pack".to_owned(),
        ));
    }
    if expected[rom_tiles.len()..].iter().any(|&byte| byte != 0) {
        return Err(mismatch(
            "the pack raster holds art past the end of the ROM tile data".to_owned(),
        ));
    }
    Ok(u32::try_from(rom_tiles.len() / bytes_per_tile).expect("a tile count fits in u32"))
}

/// The single candidate that satisfies `accept`.
///
/// # Errors
///
/// [`GenRomProfileError::StructMismatch`] unless exactly one candidate
/// satisfies it: a struct-derived choice that is still ambiguous resolves
/// nothing.
pub fn only_one_matching<T>(
    id: &str,
    what: &str,
    candidates: impl IntoIterator<Item = T>,
    accept: impl Fn(&T) -> bool,
) -> Result<T, GenRomProfileError> {
    let mut kept: Vec<T> = candidates.into_iter().filter(|item| accept(item)).collect();
    match kept.len() {
        1 => Ok(kept.remove(0)),
        found => Err(GenRomProfileError::StructMismatch {
            id: id.to_owned(),
            reason: format!("{found} candidates satisfy {what}, expected exactly 1"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        exactly_one, only_one_matching, to_addr, to_offset, u32_at_addr, ROM_BASE, ROM_WINDOW_END,
    };
    use crate::gen_rom_profile::error::GenRomProfileError;

    #[test]
    fn offsets_and_addresses_round_trip() {
        assert_eq!(to_addr(0x1234), 0x0800_1234);
        assert_eq!(to_offset(0x0800_1234), Some(0x1234));
        assert_eq!(to_offset(0x0000_0004), None);
    }

    #[test]
    fn addresses_outside_the_cartridge_window_are_not_offsets() {
        assert_eq!(to_offset(ROM_BASE - 1), None);
        assert_eq!(to_offset(ROM_BASE), Some(0));
        assert_eq!(
            to_offset(ROM_WINDOW_END - 1),
            Some((ROM_WINDOW_END - ROM_BASE - 1) as usize)
        );
        assert_eq!(to_offset(ROM_WINDOW_END), None);
        assert_eq!(to_offset(0xFFFF_FFFF), None);
    }

    #[test]
    fn a_single_hit_becomes_an_address() {
        assert_eq!(exactly_one("x", &[0x10]).unwrap(), 0x0800_0010);
    }

    #[test]
    fn no_hit_and_many_hits_both_fail() {
        assert!(matches!(
            exactly_one("x", &[]),
            Err(GenRomProfileError::NotFound { .. })
        ));
        match exactly_one("x", &[4, 8]) {
            Err(GenRomProfileError::Ambiguous { addrs, .. }) => {
                assert_eq!(addrs, vec![0x0800_0004, 0x0800_0008]);
            }
            other => panic!("expected Ambiguous, got {other:?}"),
        }
    }

    #[test]
    fn reads_are_bounds_checked() {
        let rom = [1u8, 2, 3, 4, 5];
        assert_eq!(u32_at_addr(&rom, 0x0800_0000), Some(0x0403_0201));
        assert_eq!(u32_at_addr(&rom, 0x0800_0002), None);
        assert_eq!(u32_at_addr(&rom, 0x0000_0000), None);
    }

    #[test]
    fn snake_names_become_upstreams_camel_spelling() {
        use super::camel_case;
        assert_eq!(camel_case("general"), "General");
        assert_eq!(camel_case("brendans_mays_house"), "BrendansMaysHouse");
        assert_eq!(camel_case("small_narrow"), "SmallNarrow");
        assert_eq!(camel_case("route101"), "Route101");
        assert_eq!(camel_case(""), "");
        assert_eq!(camel_case("__a__b__"), "AB");
    }

    #[test]
    fn a_short_rom_tile_run_is_a_zero_filled_prefix() {
        use super::tile_count_of_prefix;
        let mut expected = vec![7u8; 64];
        expected.extend_from_slice(&[0u8; 32]);
        assert_eq!(
            tile_count_of_prefix("x", &expected[..64], &expected, 32).unwrap(),
            2
        );
        // A tail that is not zero means the match was wrong.
        let mut art_in_the_tail = expected.clone();
        art_in_the_tail[80] = 1;
        assert!(matches!(
            tile_count_of_prefix("x", &expected[..64], &art_in_the_tail, 32),
            Err(GenRomProfileError::StructMismatch { .. })
        ));
        // A differing byte, a partial tile, and an over-long run all fail.
        assert!(tile_count_of_prefix("x", &[8u8; 32], &expected, 32).is_err());
        assert!(tile_count_of_prefix("x", &expected[..30], &expected, 32).is_err());
        assert!(tile_count_of_prefix("x", &[7u8; 128], &expected, 32).is_err());
    }

    #[test]
    fn narrowing_requires_exactly_one_survivor() {
        assert_eq!(
            only_one_matching("x", "even", [1u32, 2, 3], |v| v % 2 == 0).unwrap(),
            2
        );
        assert!(matches!(
            only_one_matching("x", "even", [1u32, 3], |v| v % 2 == 0),
            Err(GenRomProfileError::StructMismatch { .. })
        ));
        assert!(matches!(
            only_one_matching("x", "even", [2u32, 4], |v| v % 2 == 0),
            Err(GenRomProfileError::StructMismatch { .. })
        ));
    }
}
