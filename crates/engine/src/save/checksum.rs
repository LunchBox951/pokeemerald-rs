//! Emerald's flash-sector checksum.

/// Computes the checksum stored in a flash-sector footer.
///
/// Complete four-byte words are decoded little-endian and added with 32-bit
/// wrapping. The sum is folded by adding its upper half to the full sum, then
/// truncated to 16 bits. Trailing bytes outside a complete word are ignored.
/// This order and partial-word behavior match `pokeemerald/src/save.c:674-685`.
#[expect(
    clippy::cast_possible_truncation,
    reason = "the checksum format keeps the folded sum's low 16 bits"
)]
#[must_use]
pub fn checksum(data: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    for word_bytes in data.chunks_exact(4) {
        let word = u32::from_le_bytes([word_bytes[0], word_bytes[1], word_bytes[2], word_bytes[3]]);
        sum = sum.wrapping_add(word);
    }
    sum.wrapping_shr(16).wrapping_add(sum) as u16
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::save::sector::SECTOR_DATA_SIZE;

    #[test]
    fn empty_data_checksums_to_zero() {
        assert_eq!(checksum(&[]), 0x0000);
    }

    #[test]
    fn two_small_words_sum_directly() {
        let data = [1_u32.to_le_bytes(), 2_u32.to_le_bytes()].concat();
        assert_eq!(checksum(&data), 0x0003);
    }

    #[test]
    fn single_word_requires_the_fold() {
        assert_eq!(checksum(&u32::MAX.to_le_bytes()), 0xFFFE);
    }

    #[test]
    fn two_max_words_wrap_the_running_sum() {
        let data = [u32::MAX.to_le_bytes(), u32::MAX.to_le_bytes()].concat();
        assert_eq!(checksum(&data), 0xFFFD);
    }

    #[test]
    fn trailing_partial_word_is_ignored() {
        let without_trailer = 0x0403_0201_u32.to_le_bytes();
        let with_trailer = [without_trailer.as_slice(), &[5, 6]].concat();
        assert_eq!(checksum(&with_trailer), 0x0604);
        assert_eq!(checksum(&with_trailer), checksum(&without_trailer));
    }

    #[test]
    fn known_word_folds_correctly() {
        let data = 0x1234_5678_u32.to_le_bytes();
        assert_eq!(checksum(&data), 0x68AC);
    }

    #[test]
    fn full_sector_data_size_pattern() {
        let data = [0xAAu8; SECTOR_DATA_SIZE];
        assert_eq!(checksum(&data), 0xA815);
    }
}
