//! Text the screen prints for items, pockets, and context actions
//! (`item_menu.c:266-282`, `item_menu.c:896-922`, `strings.c:257-294`).

use assets::battle_moves::{MoveTable, MoveType};
use assets::items::{ItemDisplayTable, ItemId, Pocket};
use assets::move_names::MoveNames;
use assets::tmhm_learnsets::TmHmLearnsets;
use engine::bag_menu::BagAction;

use super::chrome::{NO_SYMBOL, SPACER_SYMBOL};
use super::BagScreenMode;

/// `ITEM_TM01`, whose offset indexes the TM/HM move sequence.
const FIRST_TM: u16 = 289;
/// `ITEM_HM01`.
const FIRST_HM: u16 = 339;
/// `FIRST_BERRY_INDEX`, which numbers the berry list.
const FIRST_BERRY: u16 = 133;
/// `gText_xVar1`'s multiplication sign.
const QUANTITY_PREFIX: char = '×';
const CLOSE_ROW_LABEL: &str = "CLOSE BAG";
/// `gText_ThreeDashes` (`strings.c:205`).
const THREE_DASHES: &str = "---";
/// `TYPE_MYSTERY`, which `sMenuInfoIcons` gives an icon (`pokemon.h`).
const MYSTERY_TYPE_ID: u8 = 9;
/// `ConvertIntToDecimalStringN`'s width for the move data (`item_menu.c:2572`).
const MOVE_DATA_DIGITS: usize = 3;

/// An item row's printable parts: the optional number column and the name.
pub(super) struct RowLabel {
    /// `{NO}01` for TMs and berries, `"1"` for HMs, which sit beside the HM
    /// mark.
    pub(super) number: Option<String>,
    pub(super) is_hm: bool,
    pub(super) name: String,
}

/// `GetItemNameFromPocket` (`item_menu.c:896-922`).
pub(super) fn row_label(pocket: Pocket, item: ItemId, name: Option<&str>) -> RowLabel {
    let plain = name.unwrap_or("?").to_owned();
    match pocket {
        Pocket::TmHm if item.0 >= FIRST_TM => {
            let slot = usize::from(item.0 - FIRST_TM);
            let move_name = TmHmLearnsets::slot(slot)
                .ok()
                .and_then(|slot| MoveNames::new().name(TmHmLearnsets::slot_move(slot)).ok())
                .map_or(plain, str::to_owned);
            if item.0 >= FIRST_HM {
                RowLabel {
                    number: Some((item.0 - FIRST_HM + 1).to_string()),
                    is_hm: true,
                    name: move_name,
                }
            } else {
                RowLabel {
                    number: Some(format!("{NO_SYMBOL}{:02}", slot + 1)),
                    is_hm: false,
                    name: move_name,
                }
            }
        }
        Pocket::Berries if item.0 >= FIRST_BERRY => RowLabel {
            number: Some(format!("{NO_SYMBOL}{:02}", item.0 - FIRST_BERRY + 1)),
            is_hm: false,
            name: plain,
        },
        _ => RowLabel {
            number: None,
            is_hm: false,
            name: plain,
        },
    }
}

pub(super) const fn close_row_label() -> &'static str {
    CLOSE_ROW_LABEL
}

/// The description text of an item; a `{POKEBLOCK}` in it is expanded when
/// the text is rendered.
pub(super) fn description(item: ItemId) -> String {
    ItemDisplayTable::new()
        .get(item)
        .map_or_else(|_| String::new(), |data| data.description.to_owned())
}

/// The quantity column's text: `gText_xVar1` (`strings.c:218`) around the
/// count `ConvertIntToDecimalStringN` right-aligns in `digits` positions
/// with `CHAR_SPACER` (`item_menu.c:966-974`, `string_util.c:209`).
pub(super) fn quantity_text(quantity: u16, digits: usize) -> String {
    format!("{QUANTITY_PREFIX}{}", right_aligned(quantity, digits))
}

/// `ConvertIntToDecimalStringN` in `STR_CONV_MODE_RIGHT_ALIGN`: `value` with
/// `CHAR_SPACER` filling the leading positions of `digits`.
fn right_aligned(value: u16, digits: usize) -> String {
    let count = value.to_string();
    let padding = SPACER_SYMBOL
        .to_string()
        .repeat(digits.saturating_sub(count.len()));
    format!("{padding}{count}")
}

/// What `PrintTMHMMoveData` prints for a machine's move (`item_menu.c:2562-2593`).
pub(super) struct MoveDataText {
    /// The move's combat type id, which picks its icon.
    pub(super) type_id: u8,
    pub(super) power: String,
    pub(super) accuracy: String,
    pub(super) pp: String,
}

/// The move data of the TM or HM `item` (`ItemIdToBattleMoveId`, then
/// `gBattleMoves`), or `None` for an item that is not a machine.
pub(super) fn move_data_text(item: ItemId) -> Option<MoveDataText> {
    let slot = TmHmLearnsets::slot(usize::from(item.0.checked_sub(FIRST_TM)?)).ok()?;
    let table = MoveTable::new();
    let data = table.get(TmHmLearnsets::slot_move(slot)).ok()?;
    let value = |value: u8| right_aligned(u16::from(value), MOVE_DATA_DIGITS);
    Some(MoveDataText {
        type_id: match data.move_type {
            MoveType::Battle(combat_type) => combat_type.id(),
            MoveType::Mystery => MYSTERY_TYPE_ID,
        },
        power: if data.power <= 1 {
            THREE_DASHES.to_owned()
        } else {
            value(data.power)
        },
        accuracy: if data.accuracy == 0 {
            THREE_DASHES.to_owned()
        } else {
            value(data.accuracy)
        },
        pp: value(data.pp),
    })
}

/// `PrintItemDescription`'s close-row text (`item_menu.c:1003-1008`).
pub(super) const fn close_description(mode: BagScreenMode) -> &'static str {
    match mode {
        BagScreenMode::Field => "Return to\nthe field.",
        BagScreenMode::Battle => "Return to\nthe battle.",
    }
}

/// `gPocketNamesStringsTable` (`strings.c:288-294`), by browse index.
pub(super) const fn pocket_name(browse_index: usize) -> &'static str {
    match browse_index {
        0 => "ITEMS",
        1 => "POKé BALLS",
        2 => "TMs & HMs",
        3 => "BERRIES",
        _ => "KEY ITEMS",
    }
}

/// `sItemMenuActions` text (`item_menu.c:266-282`).
pub(super) const fn action_label(action: BagAction) -> &'static str {
    match action {
        BagAction::Use | BagAction::BattleUse => "USE",
        BagAction::Toss => "TOSS",
        BagAction::Register => "REGISTER",
        BagAction::Deselect => "DESELECT",
        BagAction::Give => "GIVE",
        BagAction::Check => "CHECK",
        BagAction::Walk => "WALK",
        BagAction::CheckTag => "CHECK TAG",
        BagAction::Cancel => "CANCEL",
    }
}
