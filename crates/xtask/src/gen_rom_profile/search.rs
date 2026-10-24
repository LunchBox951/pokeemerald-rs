//! Finding known bytes inside a 16 MiB ROM.
//!
//! Every locator asks where an exact byte string lives and whether it lives
//! anywhere else. Each search type answers a whole batch of needles in one
//! pass over the image, never one pass per needle: a domain can ask about
//! several thousand candidates at once (a sprite sheet is searched under
//! every metatile shape that divides its tile grid), which a per-needle scan
//! would make minutes of debug-build work.
//!
//! [`RawSearch`] anchors each needle at an eight-byte window chosen to be
//! distinctive: sprite art usually starts with a run of transparent pixels,
//! so anchoring on the head would put thousands of needles in one bucket and
//! make the pass quadratic. [`Lz77Search`] walks only the offsets that could
//! start a stream of a wanted decompressed size.
//!
//! Both return ROM offsets, not bus addresses, and cap each needle at
//! [`MATCH_LIMIT`] hits.

use std::collections::HashMap;

use rom_import::{lz77_decompress, LZ77_TYPE, ROM_BASE, ROM_WINDOW_END};

/// The anchor window's length in bytes: one `u64` key, and short enough that
/// every root in the pack is at least this long.
const ANCHOR: usize = 8;

/// The most hits a search records per needle.
///
/// A locator needs only "one" or "more than one", plus enough addresses to
/// name the duplicates in an error. The cap bounds memory for a short blob
/// that repeats hundreds of times.
pub const MATCH_LIMIT: usize = 16;

/// The fewest distinct byte values an eight-byte window needs to serve as an
/// anchor.
const ANCHOR_DISTINCT_BYTES: usize = 5;

/// The offset within `needle` of the first [`ANCHOR`]-byte window with at
/// least [`ANCHOR_DISTINCT_BYTES`] distinct byte values, or `0` if none has.
fn anchor_offset(needle: &[u8]) -> usize {
    if needle.len() <= ANCHOR {
        return 0;
    }
    for start in 0..=needle.len() - ANCHOR {
        let window = &needle[start..start + ANCHOR];
        let mut seen = [false; 256];
        let mut distinct = 0usize;
        for &byte in window {
            if !seen[usize::from(byte)] {
                seen[usize::from(byte)] = true;
                distinct += 1;
            }
        }
        if distinct >= ANCHOR_DISTINCT_BYTES {
            return start;
        }
    }
    0
}

/// The little-endian `u64` key at `off`, or `None` if it runs past the end.
fn key_at(bytes: &[u8], off: usize) -> Option<u64> {
    bytes
        .get(off..off + ANCHOR)
        .map(|window| u64::from_le_bytes(window.try_into().expect("eight bytes")))
}

/// A batch search for uncompressed byte strings.
pub struct RawSearch<'a> {
    rom: &'a [u8],
}

impl<'a> RawSearch<'a> {
    /// Search `rom`.
    pub const fn new(rom: &'a [u8]) -> Self {
        Self { rom }
    }

    /// Find every occurrence of each needle, in one pass.
    ///
    /// Returns one ROM-offset list per needle, in the order given, each
    /// capped at [`MATCH_LIMIT`] and sorted ascending. A needle shorter than
    /// [`ANCHOR`] bytes, or longer than the ROM, gets an empty list. Use
    /// [`PointerIndex`] to find a four-byte pointer.
    pub fn find_all(&self, needles: &[Vec<u8>]) -> Vec<Vec<u32>> {
        let mut anchors = Vec::with_capacity(needles.len());
        // Bitmap over the anchor's first two bytes: a one-index filter that
        // spares most ROM offsets the hash lookup below.
        let mut coarse = vec![false; 1 << 16];
        let mut buckets: HashMap<u64, Vec<usize>> = HashMap::new();

        for (index, needle) in needles.iter().enumerate() {
            let start = anchor_offset(needle);
            anchors.push(start);
            let Some(key) = key_at(needle, start) else {
                continue;
            };
            if needle.len() > self.rom.len() {
                continue;
            }
            coarse[usize::from(u16::from_le_bytes([needle[start], needle[start + 1]]))] = true;
            buckets.entry(key).or_default().push(index);
        }

        let mut hits: Vec<Vec<u32>> = vec![Vec::new(); needles.len()];
        if buckets.is_empty() || self.rom.len() < ANCHOR {
            return hits;
        }

        for offset in 0..=self.rom.len() - ANCHOR {
            if !coarse[usize::from(u16::from_le_bytes([self.rom[offset], self.rom[offset + 1]]))] {
                continue;
            }
            let key = u64::from_le_bytes(
                self.rom[offset..offset + ANCHOR]
                    .try_into()
                    .expect("eight bytes"),
            );
            let Some(candidates) = buckets.get(&key) else {
                continue;
            };
            for &index in candidates {
                let needle = &needles[index];
                let Some(start) = offset.checked_sub(anchors[index]) else {
                    continue;
                };
                if hits[index].len() >= MATCH_LIMIT {
                    continue;
                }
                if self.rom.get(start..start + needle.len()) == Some(needle.as_slice()) {
                    let truncated =
                        u32::try_from(start).expect("a ROM offset fits in u32 at 16 MiB");
                    hits[index].push(truncated);
                }
            }
        }
        for list in &mut hits {
            list.sort_unstable();
            list.dedup();
        }
        hits
    }
}

/// Every 4-byte-aligned word in the ROM that falls in the cartridge window,
/// indexed by the address it holds.
///
/// Struct-derived resolution repeatedly asks what points at a root: a
/// tileset through its metatile table, a map layout through its grid, a song
/// header through its voicegroup. One index build replaces a pass per query.
///
/// Compiler-emitted pointers are word-aligned, so unaligned words are
/// skipped on purpose: the generator wants real symbol references, not
/// coincidental byte patterns.
pub struct PointerIndex {
    by_target: HashMap<u32, Vec<u32>>,
}

impl PointerIndex {
    /// Index every aligned cartridge-window word in `rom`.
    pub fn build(rom: &[u8]) -> Self {
        let mut by_target: HashMap<u32, Vec<u32>> = HashMap::new();
        let mut offset = 0usize;
        while offset + 4 <= rom.len() {
            let word = u32::from_le_bytes(rom[offset..offset + 4].try_into().expect("four bytes"));
            if (ROM_BASE..ROM_WINDOW_END).contains(&word) {
                let at = u32::try_from(offset).expect("a ROM offset fits in u32 at 16 MiB");
                by_target.entry(word).or_default().push(ROM_BASE + at);
            }
            offset += 4;
        }
        Self { by_target }
    }

    /// Every GBA bus address whose word holds `target`, ascending.
    pub fn refs_to(&self, target: u32) -> &[u32] {
        self.by_target.get(&target).map_or(&[], Vec::as_slice)
    }
}

/// A batch search for LZ77-compressed payloads.
///
/// A compressed root cannot be matched by its stored bytes, so this
/// decompresses each offset that starts a `LZ77_TYPE` stream whose declared
/// size equals some needle's length, once, and compares the result.
pub struct Lz77Search<'a> {
    rom: &'a [u8],
}

impl<'a> Lz77Search<'a> {
    /// Search `rom`.
    pub const fn new(rom: &'a [u8]) -> Self {
        Self { rom }
    }

    /// Find every stream that decompresses to each needle, in one pass.
    ///
    /// Returns one ROM-offset list per needle, in the order given, each
    /// capped at [`MATCH_LIMIT`] and sorted ascending.
    pub fn find_all(&self, needles: &[Vec<u8>]) -> Vec<Vec<u32>> {
        let mut by_size: HashMap<usize, Vec<usize>> = HashMap::new();
        for (index, needle) in needles.iter().enumerate() {
            by_size.entry(needle.len()).or_default().push(index);
        }
        let mut hits: Vec<Vec<u32>> = vec![Vec::new(); needles.len()];
        if self.rom.len() < 4 {
            return hits;
        }

        for offset in 0..self.rom.len() - 4 {
            if self.rom[offset] != LZ77_TYPE {
                continue;
            }
            let declared = usize::from(self.rom[offset + 1])
                | usize::from(self.rom[offset + 2]) << 8
                | usize::from(self.rom[offset + 3]) << 16;
            let Some(candidates) = by_size.get(&declared) else {
                continue;
            };
            let Ok(payload) = lz77_decompress(&self.rom[offset..], Some(declared)) else {
                continue;
            };
            for &index in candidates {
                if hits[index].len() < MATCH_LIMIT && payload == needles[index] {
                    let truncated =
                        u32::try_from(offset).expect("a ROM offset fits in u32 at 16 MiB");
                    hits[index].push(truncated);
                }
            }
        }
        hits
    }
}

#[cfg(test)]
mod tests {
    use super::{anchor_offset, Lz77Search, RawSearch, ANCHOR};

    /// Find one needle, the shape most of these tests want.
    fn find_one(rom: &[u8], needle: &[u8]) -> Vec<u32> {
        RawSearch::new(rom)
            .find_all(&[needle.to_vec()])
            .pop()
            .unwrap_or_default()
    }

    #[test]
    fn an_anchor_skips_a_uniform_run() {
        let mut needle = vec![0u8; 16];
        needle.extend_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        // The window at 12 is `0,0,0,0,1,2,3,4`: five distinct values, so
        // it is already distinctive enough to anchor on.
        assert_eq!(anchor_offset(&needle), 12);
    }

    #[test]
    fn a_needle_with_no_distinct_window_anchors_at_its_head() {
        assert_eq!(anchor_offset(&[7u8; 32]), 0);
        assert_eq!(anchor_offset(&[1, 2, 3]), 0);
    }

    #[test]
    fn a_unique_needle_is_found_once() {
        let mut rom = vec![0u8; 4096];
        rom[1000..1008].copy_from_slice(&[9, 8, 7, 6, 5, 4, 3, 2]);
        let hits = find_one(&rom, &[9, 8, 7, 6, 5, 4, 3, 2]);
        assert_eq!(hits, vec![1000]);
    }

    #[test]
    fn a_repeated_needle_reports_every_occurrence() {
        let mut rom = vec![0u8; 4096];
        for at in [100usize, 2000, 3000] {
            rom[at..at + ANCHOR].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        }
        let hits = find_one(&rom, &[1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(hits, vec![100, 2000, 3000]);
    }

    #[test]
    fn a_batch_keeps_per_needle_results_apart() {
        let mut rom = vec![0u8; 8192];
        rom[10..18].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        rom[500..508].copy_from_slice(&[8, 7, 6, 5, 4, 3, 2, 1]);
        let needles = vec![
            vec![1, 2, 3, 4, 5, 6, 7, 8],
            vec![8, 7, 6, 5, 4, 3, 2, 1],
            vec![0xAA; 12],
        ];
        let hits = RawSearch::new(&rom).find_all(&needles);
        assert_eq!(hits[0], vec![10]);
        assert_eq!(hits[1], vec![500]);
        assert!(hits[2].is_empty());
    }

    #[test]
    fn a_needle_anchored_past_its_head_reports_its_own_start() {
        // The leading zeros force the anchor forward; the reported offset
        // must still be the needle's first byte.
        let mut needle = vec![0u8; 12];
        needle.extend_from_slice(&[3, 1, 4, 1, 5, 9, 2, 6]);
        let mut rom = vec![0u8; 4096];
        rom[600..600 + needle.len()].copy_from_slice(&needle);
        assert_eq!(find_one(&rom, &needle), vec![600]);
    }

    #[test]
    fn the_pointer_index_finds_aligned_references() {
        use super::PointerIndex;
        let mut rom = vec![0u8; 64];
        rom[8..12].copy_from_slice(&0x0800_1234u32.to_le_bytes());
        rom[32..36].copy_from_slice(&0x0800_1234u32.to_le_bytes());
        // Unaligned, and so deliberately invisible.
        rom[45..49].copy_from_slice(&0x0800_1234u32.to_le_bytes());
        // Not a cartridge address.
        rom[52..56].copy_from_slice(&0x0300_0000u32.to_le_bytes());
        let index = PointerIndex::build(&rom);
        assert_eq!(index.refs_to(0x0800_1234), [0x0800_0008, 0x0800_0020]);
        assert!(index.refs_to(0x0300_0000).is_empty());
        assert!(index.refs_to(0x0900_0000).is_empty());
    }

    #[test]
    fn a_needle_longer_than_the_rom_matches_nothing() {
        let rom = vec![0u8; 16];
        assert!(find_one(&rom, &[1u8; 64]).is_empty());
    }

    /// Compress `data` as a type `0x10` stream of literals only. Valid
    /// input for the decompressor, which is all a search test needs.
    fn lz77_literals(data: &[u8]) -> Vec<u8> {
        let mut out = vec![
            0x10,
            u8::try_from(data.len() & 0xFF).unwrap(),
            u8::try_from((data.len() >> 8) & 0xFF).unwrap(),
            u8::try_from((data.len() >> 16) & 0xFF).unwrap(),
        ];
        for chunk in data.chunks(8) {
            out.push(0);
            out.extend_from_slice(chunk);
        }
        out
    }

    #[test]
    fn a_compressed_payload_is_found_by_its_decompressed_bytes() {
        let payload: Vec<u8> = (0..64u8).collect();
        let stream = lz77_literals(&payload);
        let mut rom = vec![0xFFu8; 8192];
        rom[2048..2048 + stream.len()].copy_from_slice(&stream);
        let hits = Lz77Search::new(&rom).find_all(&[payload, vec![7u8; 64]]);
        assert_eq!(hits[0], vec![2048]);
        assert!(hits[1].is_empty());
    }
}
