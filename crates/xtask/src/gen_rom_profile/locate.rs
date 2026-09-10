//! Shared plumbing for the per-domain locators: addresses, uniqueness, and
//! pointer back-references.
//!
//! Two rules hold everywhere. A root is only accepted when exactly one
//! place in the ROM holds its bytes, and an address is only ever a GBA bus
//! address once it leaves this module, so nothing downstream has to
//! remember whether it is holding an offset or a pointer.

use rom_import::ROM_BASE;

use super::error::GenRomProfileError;

/// Turn a ROM offset into the GBA bus address the cartridge is mapped at.
pub const fn to_addr(offset: u32) -> u32 {
    ROM_BASE + offset
}

/// Turn a GBA bus address into a ROM offset, or `None` if it is not a
/// cartridge address at all.
pub fn to_offset(addr: u32) -> Option<usize> {
    addr.checked_sub(ROM_BASE).map(|off| off as usize)
}

/// Accept a search result only when it found exactly one place.
///
/// # Errors
///
/// [`GenRomProfileError::NotFound`] for no match,
/// [`GenRomProfileError::Ambiguous`] for more than one. An ambiguous root
/// needs a struct back-reference, not a coin toss.
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

/// Read a little-endian `u32` at a ROM offset.
pub fn u32_at(rom: &[u8], offset: usize) -> Option<u32> {
    rom.get(offset..offset + 4)
        .map(|bytes| u32::from_le_bytes(bytes.try_into().expect("four bytes")))
}

/// Read a little-endian `u32` at a GBA bus address.
pub fn u32_at_addr(rom: &[u8], addr: u32) -> Option<u32> {
    u32_at(rom, to_offset(addr)?)
}

/// Read a little-endian `u16` at a GBA bus address.
pub fn u16_at_addr(rom: &[u8], addr: u32) -> Option<u16> {
    let offset = to_offset(addr)?;
    rom.get(offset..offset + 2)
        .map(|bytes| u16::from_le_bytes(bytes.try_into().expect("two bytes")))
}

/// Read one byte at a GBA bus address.
pub fn u8_at_addr(rom: &[u8], addr: u32) -> Option<u8> {
    rom.get(to_offset(addr)?).copied()
}

/// Borrow `len` bytes at a GBA bus address.
pub fn slice_at_addr(rom: &[u8], addr: u32, len: usize) -> Option<&[u8]> {
    let offset = to_offset(addr)?;
    rom.get(offset..offset.checked_add(len)?)
}

/// Turn a normalized `snake_case` pack name into upstream's `CamelCase`
/// spelling of the same thing: `brendans_mays_house` becomes
/// `BrendansMaysHouse`.
///
/// Upstream names its symbols after the same assets the pack ids name, in
/// the other convention, so this is what lets a `--map` cross-check assert
/// a symbol name without a table of them.
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

/// How many tiles the ROM stores for an image whose full raster needs
/// `expected`.
///
/// Upstream trims all-zero trailing tiles and honours `-num_tiles`, so the
/// ROM legitimately holds a prefix. Anything else -- a differing byte, a
/// non-zero tail, a partial tile -- means the locator matched the wrong
/// thing and must not be trusted.
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

/// Narrow a list of candidate addresses down to the one that satisfies
/// `accept`.
///
/// # Errors
///
/// [`GenRomProfileError::StructMismatch`] if no candidate or more than one
/// candidate satisfies it. A struct-derived resolution that is itself
/// ambiguous is no better than the signature it was meant to disambiguate.
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
    use super::{exactly_one, only_one_matching, to_addr, to_offset, u32_at_addr};
    use crate::gen_rom_profile::error::GenRomProfileError;

    #[test]
    fn offsets_and_addresses_round_trip() {
        assert_eq!(to_addr(0x1234), 0x0800_1234);
        assert_eq!(to_offset(0x0800_1234), Some(0x1234));
        assert_eq!(to_offset(0x0000_0004), None);
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
