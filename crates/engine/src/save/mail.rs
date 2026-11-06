//! `struct Mail` and the save block's mail array (`pokeemerald/include/global.h:764-771,1053`).
//!
//! # Layout
//!
//! `SaveBlock1::mail` holds [`MAIL_COUNT`] entries at `0x2BE0`, six party slots
//! (`0..PARTY_SIZE`, attached to a Pokemon through its `mail` byte) followed by
//! ten PC mailbox slots. An entry's fields end at `0x22`, but the next
//! `SaveBlock1` field, `unlockedTrendySayings`, sits at `0x2E20`
//! (`global.h:1053-1054`), which is `MAIL_COUNT * 0x24` past the array: the
//! agbcc/APCS ABI upstream builds with rounds the struct up to a multiple of
//! four bytes. [`Mail::LEN`] is therefore the serialized stride, `0x24`, not
//! the host's `size_of`, and the two trailing bytes are kept in
//! [`Mail::padding`] so a whole-slot copy (`memcpy` in `TakeMailFromMonAndSave`,
//! `mail_data.c:173`) moves them too.
//!
//! A slot is empty when its `itemId` is `ITEM_NONE` (`mail_data.c:55,169`);
//! a loaded all-zero slot is therefore empty, and decoding keeps its zero
//! words rather than rewriting them to [`Mail::default`]'s `ClearMail` values.

use assets::items::ItemId;

/// Party-attached mail slots (`PARTY_SIZE`, `pokeemerald/include/constants/global.h:33`).
pub const MAIL_PARTY_SLOTS: usize = 6;
/// `MAIL_COUNT` (`pokeemerald/include/constants/global.h:47`): party slots plus
/// the PC mailbox's ten.
pub const MAIL_COUNT: usize = 10 + MAIL_PARTY_SLOTS;
/// `MAIL_NONE`, a Pokemon's `mail` byte when it carries none
/// (`pokeemerald/include/constants/items.h:446`).
pub const MAIL_NONE: u8 = 0xFF;
/// `MAIL_WORDS_COUNT` (`pokeemerald/include/constants/global.h:98`).
pub const MAIL_WORDS_COUNT: usize = 9;

const EC_EMPTY_WORD: u16 = 0xFFFF;
const EOS: u8 = 0xFF;
const SPECIES_BULBASAUR: u16 = 1;

const WORDS_OFFSET: usize = 0x00;
const PLAYER_NAME_OFFSET: usize = 0x12;
const TRAINER_ID_OFFSET: usize = 0x1A;
const SPECIES_OFFSET: usize = 0x1E;
const ITEM_ID_OFFSET: usize = 0x20;
const PADDING_OFFSET: usize = 0x22;

/// `ItemIsMail` (`pokeemerald/src/mail_data.c:185`): the twelve mail ids,
/// `ITEM_ORANGE_MAIL..=ITEM_RETRO_MAIL`. Not an item-type test: Master Ball
/// shares mail's `ItemType(0)`.
#[must_use]
pub fn item_is_mail(item: ItemId) -> bool {
    (ItemId::ORANGE_MAIL..=ItemId::RETRO_MAIL).contains(&item)
}

/// One `struct Mail`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mail {
    /// Easy Chat words.
    pub words: [u16; MAIL_WORDS_COUNT],
    /// The writer's name, Gen 3 encoded.
    pub player_name: [u8; 8],
    /// The writer's trainer id.
    pub trainer_id: [u8; 4],
    /// `species`, in the mail-species numbering.
    pub species: u16,
    /// The mail item; [`ItemId::NONE`] marks the slot empty.
    pub item_id: ItemId,
    /// The two bytes the ABI rounds the struct up by.
    pub padding: [u8; 2],
}

impl Mail {
    /// Serialized length of one entry.
    pub const LEN: usize = 0x24;

    /// Decodes one entry, keeping every byte as stored.
    #[must_use]
    pub fn from_bytes(bytes: &[u8; Self::LEN]) -> Self {
        let mut words = [0u16; MAIL_WORDS_COUNT];
        for (index, word) in words.iter_mut().enumerate() {
            let at = WORDS_OFFSET + index * 2;
            *word = u16::from_le_bytes([bytes[at], bytes[at + 1]]);
        }
        Self {
            words,
            player_name: bytes[PLAYER_NAME_OFFSET..][..8]
                .try_into()
                .unwrap_or_default(),
            trainer_id: bytes[TRAINER_ID_OFFSET..][..4]
                .try_into()
                .unwrap_or_default(),
            species: u16::from_le_bytes([bytes[SPECIES_OFFSET], bytes[SPECIES_OFFSET + 1]]),
            item_id: ItemId(u16::from_le_bytes([
                bytes[ITEM_ID_OFFSET],
                bytes[ITEM_ID_OFFSET + 1],
            ])),
            padding: [bytes[PADDING_OFFSET], bytes[PADDING_OFFSET + 1]],
        }
    }

    /// Encodes one entry.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; Self::LEN] {
        let mut out = [0u8; Self::LEN];
        for (index, word) in self.words.iter().enumerate() {
            let at = WORDS_OFFSET + index * 2;
            out[at..at + 2].copy_from_slice(&word.to_le_bytes());
        }
        out[PLAYER_NAME_OFFSET..][..8].copy_from_slice(&self.player_name);
        out[TRAINER_ID_OFFSET..][..4].copy_from_slice(&self.trainer_id);
        out[SPECIES_OFFSET..][..2].copy_from_slice(&self.species.to_le_bytes());
        out[ITEM_ID_OFFSET..][..2].copy_from_slice(&self.item_id.index().to_le_bytes());
        out[PADDING_OFFSET..][..2].copy_from_slice(&self.padding);
        out
    }

    /// Whether the slot is free: `itemId == ITEM_NONE`, the only test
    /// `GiveMailToMonByItemId` and `TakeMailFromMonAndSave` make.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.item_id == ItemId::NONE
    }
}

impl Default for Mail {
    /// `ClearMail` (`pokeemerald/src/mail_data.c:19-34`); the padding bytes are
    /// not fields upstream assigns, so they read zero.
    fn default() -> Self {
        Self {
            words: [EC_EMPTY_WORD; MAIL_WORDS_COUNT],
            player_name: [EOS; 8],
            trainer_id: [0; 4],
            species: SPECIES_BULBASAUR,
            item_id: ItemId::NONE,
            padding: [0; 2],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Mail {
        Mail {
            words: [
                0x0102, 0x0304, 0x0506, 0x0708, 0x090A, 0x0B0C, 0x0D0E, 0x0F10, 0x1112,
            ],
            player_name: [1, 2, 3, 4, 5, 6, 7, 8],
            trainer_id: [9, 10, 11, 12],
            species: 0x1314,
            item_id: ItemId::HARBOR_MAIL,
            padding: [0xAA, 0xBB],
        }
    }

    #[test]
    fn an_entry_round_trips_at_its_upstream_field_offsets() {
        let bytes = sample().to_bytes();
        assert_eq!(&bytes[0..2], &[0x02, 0x01]);
        assert_eq!(&bytes[0x10..0x12], &[0x12, 0x11]);
        assert_eq!(&bytes[0x12..0x1A], &[1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(&bytes[0x1A..0x1E], &[9, 10, 11, 12]);
        assert_eq!(&bytes[0x1E..0x20], &[0x14, 0x13]);
        assert_eq!(
            &bytes[0x20..0x22],
            &ItemId::HARBOR_MAIL.index().to_le_bytes()
        );
        assert_eq!(&bytes[0x22..0x24], &[0xAA, 0xBB]);
        assert_eq!(Mail::from_bytes(&bytes), sample());
    }

    #[test]
    fn the_default_is_clear_mail() {
        let mail = Mail::default();
        assert_eq!(mail.words, [0xFFFF; 9]);
        assert_eq!(mail.player_name, [0xFF; 8]);
        assert_eq!(mail.trainer_id, [0; 4]);
        assert_eq!(mail.species, 1);
        assert!(mail.is_empty());
    }

    #[test]
    fn a_zero_slot_decodes_empty_and_keeps_its_zeros() {
        let mail = Mail::from_bytes(&[0; Mail::LEN]);
        assert!(mail.is_empty());
        assert_eq!(mail.words, [0; 9]);
        assert_eq!(mail.to_bytes(), [0; Mail::LEN]);
    }

    #[test]
    fn exactly_the_twelve_mail_items_are_mail() {
        let mail_ids: Vec<u16> = (0..400).filter(|&id| item_is_mail(ItemId(id))).collect();
        assert_eq!(mail_ids, (121..=132).collect::<Vec<u16>>());
        assert!(!item_is_mail(ItemId::MASTER_BALL));
    }
}
