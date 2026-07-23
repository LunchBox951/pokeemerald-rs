//! Sector checksum (S-5).
//!
//! Behavioural re-implementation `(behavioral-fidelity)` of the static
//! `CalculateChecksum` in `pokeemerald/src/save.c`.

/// Compute the checksum upstream's sector footer stores, mirroring the
/// static `CalculateChecksum(void *data, u16 size)` in `src/save.c`.
///
/// Upstream reads `size / 4` little-endian `u32` words starting at `data`
/// (the GBA is little-endian) and sums them into a 32-bit accumulator, then
/// folds that sum to 16 bits — `(checksum >> 16) + checksum` — with the
/// final truncation to `u16` coming from the C function's return type. Both
/// the running sum and the fold use wrapping (unsigned, C-defined-behaviour)
/// arithmetic. A trailing partial word — `data.len()` not a multiple of 4 —
/// is never read, exactly as the integer-truncating `size / 4` loop bound
/// leaves it out.
// The final `as u16` mirrors `CalculateChecksum`'s C return type, which
// truncates the folded 32-bit sum to 16 bits by design — not a bug this
// port needs to guard against.
#[allow(clippy::cast_possible_truncation)]
#[must_use]
pub fn checksum(data: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    for word in data.chunks_exact(4) {
        // `chunks_exact(4)` guarantees exactly 4 bytes per `word`, so this
        // indexing never panics.
        let w = u32::from_le_bytes([word[0], word[1], word[2], word[3]]);
        sum = sum.wrapping_add(w);
    }
    sum.wrapping_shr(16).wrapping_add(sum) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    // Vectors computed independently (by hand / a standalone script), not
    // derived from `checksum`'s own code, so they pin the algorithm rather
    // than restate it. Each is: sum the data as little-endian u32 words,
    // fold `(sum >> 16) + sum`, keep the low 16 bits.

    #[test]
    fn empty_data_checksums_to_zero() {
        assert_eq!(checksum(&[]), 0x0000);
    }

    #[test]
    fn two_small_words_sum_directly() {
        // words: 1, 2 -> sum 3, fold (0 + 3) = 3.
        let data = [1, 0, 0, 0, 2, 0, 0, 0];
        assert_eq!(checksum(&data), 0x0003);
    }

    #[test]
    fn single_word_requires_the_fold() {
        // word: 0xFFFFFFFF -> sum 0xFFFFFFFF, fold 0xFFFF + 0xFFFFFFFF =
        // 0x1_0000_FFFE, wraps (u32) to 0x0000FFFE, low 16 bits 0xFFFE.
        let data = [0xFF, 0xFF, 0xFF, 0xFF];
        assert_eq!(checksum(&data), 0xFFFE);
    }

    #[test]
    fn two_max_words_wrap_the_running_sum() {
        // words: 0xFFFFFFFF, 0xFFFFFFFF -> sum wraps to 0xFFFFFFFE,
        // fold 0xFFFF + 0xFFFFFFFE = 0x1_0000_FFFD -> wraps to 0xFFFD.
        let data = [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
        assert_eq!(checksum(&data), 0xFFFD);
    }

    #[test]
    fn trailing_partial_word_is_ignored() {
        // Only the first word (bytes 1,2,3,4 -> 0x04030201) is read; bytes
        // 5,6 don't make a full word and are dropped, matching `size / 4`.
        // sum = 0x04030201, fold = 0x0403 + 0x04030201 = 0x04030604.
        let with_trailer = [1u8, 2, 3, 4, 5, 6];
        let without_trailer = [1u8, 2, 3, 4];
        assert_eq!(checksum(&with_trailer), 0x0604);
        assert_eq!(checksum(&with_trailer), checksum(&without_trailer));
    }

    #[test]
    fn known_word_folds_correctly() {
        // Single word 0x12345678 (little-endian bytes 78 56 34 12).
        // fold = 0x1234 + 0x12345678 = 0x123468AC, low 16 bits 0x68AC.
        let data = [0x78, 0x56, 0x34, 0x12];
        assert_eq!(checksum(&data), 0x68AC);
    }

    #[test]
    fn full_sector_data_size_pattern() {
        // A full 3968-byte SECTOR_DATA_SIZE buffer of repeated 0xAA bytes:
        // 992 words of 0xAAAAAAAA each. sum = (992 * 0xAAAAAAAA) mod 2^32,
        // fold as usual. Value below computed independently in Python.
        let data = [0xAAu8; 3968];
        assert_eq!(checksum(&data), 0xA815);
    }
}
