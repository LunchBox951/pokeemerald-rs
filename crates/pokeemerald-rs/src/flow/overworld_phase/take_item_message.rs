//! The message the start menu's TAKE entry shows after it runs.
//!
//! Upstream prints these in the party menu's message window
//! (`CursorCb_TakeItem`, `pokeemerald/src/party_menu.c:3273-3298`); the port
//! has no party menu, so [`OverworldPhase::show_take_item_outcome`] puts the
//! same texts in the field message box.

use assets::items::ItemId;
use assets::ItemDisplayTable;
use std::fmt;

use engine::text::{decode, TextError, Token};

use crate::party::TakeHeldItem;

/// Whether [`take_item_message`] prints the lead's nickname for `outcome`.
pub(super) fn names_lead(outcome: Option<TakeHeldItem>) -> bool {
    matches!(
        outcome,
        Some(TakeHeldItem::NotHolding | TakeHeldItem::HoldsMail(_) | TakeHeldItem::Took(_))
    )
}

/// Why [`nickname_tokens`] could not name the lead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum NicknameError {
    /// The nickname bytes do not decode: a control sequence cut off by the
    /// field's end, or a byte with no Latin glyph.
    Text(TextError),
    /// The lead has no nickname and its species has no display name.
    UnknownSpecies(assets::SpeciesId),
}

impl fmt::Display for NicknameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Text(error) => write!(f, "the lead's nickname does not decode: {error:?}"),
            Self::UnknownSpecies(species) => {
                write!(f, "no species name for species {}", species.0)
            }
        }
    }
}

/// `GetMonNickname` (`pokeemerald/src/pokemon.c`): the record's nickname as
/// message tokens, without its terminator.
///
/// Upstream prints the record's own name bytes and never substitutes another
/// identity, and `CreateMon` gives a mon that was never renamed its species
/// name as nickname. So an empty nickname is shown as `species`'s display
/// name, and a nickname this port's text codec refuses is an error rather
/// than the species name, as for battle text
/// (`live_battle_text::nickname_or_species`).
///
/// # Errors
///
/// [`NicknameError::Text`] when the nickname does not decode;
/// [`NicknameError::UnknownSpecies`] when it is empty and `species` has no
/// name.
pub(super) fn nickname_tokens(
    record: &engine::save::Pokemon,
    species: assets::SpeciesId,
) -> Result<Vec<Token>, NicknameError> {
    let mut tokens = decode(&record.box_data.nickname()).map_err(NicknameError::Text)?;
    if tokens.last() == Some(&Token::End) {
        tokens.pop();
    }
    if tokens.is_empty() {
        let name = assets::SpeciesNames::new()
            .name(species)
            .map_err(|_| NicknameError::UnknownSpecies(species))?;
        return Ok(plain(name));
    }
    Ok(tokens)
}

/// The text `CursorCb_TakeItem` shows for `outcome`, ending in [`Token::End`].
/// `nickname` is `gStringVar1`; `None` is no lead.
///
/// - [`TakeHeldItem::NotHolding`] prints `gText_PkmnNotHolding`.
/// - [`TakeHeldItem::BagFull`] prints `BufferBagFullCantTakeItemMessage`'s
///   `gText_BagFullCouldNotRemoveItem`.
/// - [`TakeHeldItem::Took`] prints `DisplayTookHeldItemMessage`'s
///   `gText_ReceivedItemFromPkmn`, with `gStringVar2` the item's name.
/// - [`TakeHeldItem::HoldsMail`] has no upstream text. Upstream never reaches
///   TAKE ITEM for mail (it offers `CursorCb_TakeMail`, which this port does
///   not model, #3017), so the port says what is true: the lead is holding
///   MAIL, which TAKE leaves in place.
/// - No lead prints `gText_NoPokemon`.
pub(super) fn take_item_message(outcome: Option<TakeHeldItem>, nickname: Vec<Token>) -> Vec<Token> {
    let mut tokens = match outcome {
        None => plain("There is no\nPOKéMON."),
        Some(TakeHeldItem::NotHolding) => {
            let mut tokens = nickname;
            tokens.extend(plain(" isn't holding\nanything."));
            tokens
        }
        Some(TakeHeldItem::HoldsMail(_)) => {
            let mut tokens = nickname;
            tokens.extend(plain(" is holding\nMAIL."));
            tokens
        }
        Some(TakeHeldItem::BagFull(_)) => {
            plain("The BAG is full. The POKéMON's\nitem could not be removed.")
        }
        Some(TakeHeldItem::Took(item)) => {
            let mut tokens = plain("Received the ");
            tokens.extend(plain(item_name(item)));
            tokens.extend(plain("\nfrom "));
            tokens.extend(nickname);
            tokens.extend(plain("."));
            tokens
        }
    };
    tokens.push(Token::End);
    tokens
}

/// `CopyItemName` (`pokeemerald/src/item.c`): the item's display name.
fn item_name(item: ItemId) -> &'static str {
    ItemDisplayTable::new()
        .get(item)
        .map_or("?", |display| display.name)
}

fn plain(text: &str) -> Vec<Token> {
    text.chars()
        .map(|c| {
            if c == '\n' {
                Token::Newline
            } else {
                Token::Char(c)
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rendered(tokens: &[Token]) -> String {
        tokens
            .iter()
            .map(|token| match token {
                Token::Char(c) => *c,
                Token::Newline => '/',
                Token::End => '$',
                other => panic!("unexpected {other:?}"),
            })
            .collect()
    }

    fn nick() -> Vec<Token> {
        plain("TREECKO")
    }

    #[test]
    fn each_outcome_prints_its_upstream_text() {
        let message = |outcome| rendered(&take_item_message(outcome, nick()));
        assert_eq!(
            message(Some(TakeHeldItem::NotHolding)),
            "TREECKO isn't holding/anything.$"
        );
        assert_eq!(
            message(Some(TakeHeldItem::HoldsMail(ItemId::ORANGE_MAIL))),
            "TREECKO is holding/MAIL.$"
        );
        assert_eq!(
            message(Some(TakeHeldItem::BagFull(ItemId::ORAN_BERRY))),
            "The BAG is full. The POKéMON's/item could not be removed.$"
        );
        assert_eq!(
            message(Some(TakeHeldItem::Took(ItemId::ORAN_BERRY))),
            "Received the ORAN BERRY/from TREECKO.$"
        );
        assert_eq!(message(None), "There is no/POKéMON.$");
    }

    #[test]
    fn the_nickname_comes_from_the_record_without_its_terminator() {
        let mut record = engine::save::Pokemon::default();
        let mut field = [0u8; engine::save::BOX_NICKNAME_LEN];
        let encoded = engine::text::encode_str("MUDKIP").unwrap();
        field[..encoded.len()].copy_from_slice(&encoded);
        record.box_data.set_nickname(field);
        assert_eq!(
            rendered(&nickname_tokens(&record, assets::SpeciesId::TREECKO).unwrap()),
            "MUDKIP"
        );
    }

    fn record_with_nickname(field: [u8; engine::save::BOX_NICKNAME_LEN]) -> engine::save::Pokemon {
        let mut record = engine::save::Pokemon::default();
        record.box_data.set_nickname(field);
        record
    }

    #[test]
    fn only_the_messages_that_print_the_nickname_name_the_lead() {
        assert!(names_lead(Some(TakeHeldItem::NotHolding)));
        assert!(names_lead(Some(TakeHeldItem::HoldsMail(
            ItemId::ORANGE_MAIL
        ))));
        assert!(names_lead(Some(TakeHeldItem::Took(ItemId::ORAN_BERRY))));
        assert!(!names_lead(Some(TakeHeldItem::BagFull(ItemId::ORAN_BERRY))));
        assert!(!names_lead(None));
    }

    #[test]
    fn an_undecodable_nickname_is_reported_not_shown_as_the_species_name() {
        const UNASSIGNED_CHARACTER: u8 = 0x60;
        let mut field = [0xFFu8; engine::save::BOX_NICKNAME_LEN];
        field[0] = UNASSIGNED_CHARACTER;
        let record = record_with_nickname(field);
        assert!(decode(&record.box_data.nickname()).is_err());
        assert_eq!(
            nickname_tokens(&record, assets::SpeciesId::TREECKO),
            Err(NicknameError::Text(TextError::UnknownByte(
                UNASSIGNED_CHARACTER
            )))
        );
    }

    #[test]
    fn a_nickname_cut_off_inside_a_control_sequence_is_reported() {
        let mut field = [0u8; engine::save::BOX_NICKNAME_LEN];
        let encoded = engine::text::encode_str("MUDKIPABC").unwrap();
        field[..9].copy_from_slice(&encoded[..9]);
        field[9] = engine::text::PLACEHOLDER_BEGIN;
        let record = record_with_nickname(field);
        assert!(matches!(
            nickname_tokens(&record, assets::SpeciesId::TREECKO),
            Err(NicknameError::Text(_))
        ));
    }

    #[test]
    fn an_empty_nickname_shows_the_species_name() {
        let record = record_with_nickname([0xFFu8; engine::save::BOX_NICKNAME_LEN]);
        assert_eq!(
            rendered(&nickname_tokens(&record, assets::SpeciesId::TREECKO).unwrap()),
            "TREECKO"
        );
    }

    #[test]
    fn an_empty_nickname_of_an_unknown_species_is_reported() {
        let record = record_with_nickname([0xFFu8; engine::save::BOX_NICKNAME_LEN]);
        let species = assets::SpeciesId(u16::MAX);
        assert_eq!(
            nickname_tokens(&record, species),
            Err(NicknameError::UnknownSpecies(species))
        );
    }
}
