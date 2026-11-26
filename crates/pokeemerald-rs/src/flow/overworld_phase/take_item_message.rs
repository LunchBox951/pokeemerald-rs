//! The message the start menu's TAKE entry shows after it runs.
//!
//! Upstream prints these in the party menu's message window
//! (`CursorCb_TakeItem`, `pokeemerald/src/party_menu.c:3273-3298`); the port
//! has no party menu, so [`OverworldPhase::show_take_item_outcome`] puts the
//! same texts in the field message box.

use assets::items::ItemId;
use assets::ItemDisplayTable;
use engine::text::{decode, Token};

use crate::party::TakeHeldItem;

/// `GetMonNickname` (`pokeemerald/src/pokemon.c`): the record's nickname as
/// message tokens, without its terminator. A nickname this port's text codec
/// refuses (a control sequence cut off by the field's end, or a byte with no
/// Latin glyph) is replaced by `species`'s display name, the nickname
/// `CreateMon` gives a mon that was never renamed, so the message still names
/// the lead; a species name that cannot be resolved leaves the name empty.
pub(super) fn nickname_tokens(
    record: &engine::save::Pokemon,
    species: assets::SpeciesId,
) -> Vec<Token> {
    let mut tokens = decode(&record.box_data.nickname()).unwrap_or_else(|_| {
        assets::SpeciesNames::new()
            .name(species)
            .ok()
            .and_then(|name| engine::text::encode_str(name).ok())
            .and_then(|encoded| decode(&encoded).ok())
            .unwrap_or_default()
    });
    if tokens.last() == Some(&Token::End) {
        tokens.pop();
    }
    tokens
}

/// The text `CursorCb_TakeItem` shows for `outcome`, ending in [`Token::End`].
/// `nickname` is `gStringVar1`; `None` is no lead.
///
/// - [`TakeHeldItem::NotHolding`] prints `gText_PkmnNotHolding`.
/// - [`TakeHeldItem::BagFull`] prints `BufferBagFullCantTakeItemMessage`'s
///   `gText_BagFullCouldNotRemoveItem`.
/// - [`TakeHeldItem::Took`] prints `DisplayTookHeldItemMessage`'s
///   `gText_ReceivedItemFromPkmn`, with `gStringVar2` the item's name.
/// - [`TakeHeldItem::HoldsMail`] has no `CursorCb_TakeItem` text: upstream
///   never reaches TAKE ITEM for mail, and the start menu sends a mail holder
///   to `CursorCb_TakeMail`'s flow (`start_menu::take_mail_dialog`) before this
///   runs. The outcome cannot arise from the menu; it prints nothing.
/// - No lead prints `gText_NoPokemon`.
pub(super) fn take_item_message(outcome: Option<TakeHeldItem>, nickname: Vec<Token>) -> Vec<Token> {
    let mut tokens = match outcome {
        None => plain("There is no\nPOKéMON."),
        Some(TakeHeldItem::NotHolding) => {
            let mut tokens = nickname;
            tokens.extend(plain(" isn't holding\nanything."));
            tokens
        }
        Some(TakeHeldItem::HoldsMail(_)) => Vec::new(),
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
            "$",
            "the mail flow owns mail holders, so this prints nothing"
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
            rendered(&nickname_tokens(&record, assets::SpeciesId::TREECKO)),
            "MUDKIP"
        );
    }

    #[test]
    fn a_nickname_the_codec_refuses_falls_back_to_the_species_name() {
        let mut record = engine::save::Pokemon::default();
        let mut field = [0u8; engine::save::BOX_NICKNAME_LEN];
        let encoded = engine::text::encode_str("MUDKIPABC").unwrap();
        field[..9].copy_from_slice(&encoded[..9]);
        field[9] = engine::text::PLACEHOLDER_BEGIN;
        record.box_data.set_nickname(field);
        assert!(engine::text::decode(&record.box_data.nickname()).is_err());
        assert_eq!(
            rendered(&nickname_tokens(&record, assets::SpeciesId::TREECKO)),
            "TREECKO"
        );
    }
}
