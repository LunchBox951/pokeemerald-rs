//! The action menus a party member opens (`SetPartyMonSelectionActions`,
//! `pokeemerald/src/party_menu.c:2607-2649`; tables at
//! `pokeemerald/src/data/party_menu.h:695-763`).

use crate::party::records::{PartyRecords, SlotFacts};
use assets::items::ItemId;

/// A field move the party menu offers beside SUMMARY for a member that knows
/// it (`sFieldMoves`). Using one is the host's: it checks badges and the
/// surroundings and runs the move's effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FieldMove {
    Cut,
    Flash,
    RockSmash,
    Strength,
    Surf,
    Fly,
    Dive,
    Waterfall,
    Teleport,
    Dig,
    SecretPower,
    MilkDrink,
    SoftBoiled,
    SweetScent,
}

impl FieldMove {
    /// `sFieldMoves`' order and the move ids they stand for
    /// (`pokeemerald/include/constants/moves.h`).
    const ALL: [(Self, u16); 14] = [
        (Self::Cut, 15),
        (Self::Flash, 148),
        (Self::RockSmash, 249),
        (Self::Strength, 70),
        (Self::Surf, 57),
        (Self::Fly, 19),
        (Self::Dive, 291),
        (Self::Waterfall, 127),
        (Self::Teleport, 100),
        (Self::Dig, 91),
        (Self::SecretPower, 290),
        (Self::MilkDrink, 208),
        (Self::SoftBoiled, 135),
        (Self::SweetScent, 230),
    ];

    fn of_move(move_id: u16) -> Option<Self> {
        Self::ALL
            .iter()
            .find_map(|(field_move, id)| (*id == move_id).then_some(*field_move))
    }
}

/// One line of an action menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PartyAction {
    Summary,
    Switch,
    /// The held-item submenu (`MENU_ITEM`).
    Item,
    /// The held-mail submenu (`MENU_MAIL`), shown in place of ITEM.
    Mail,
    /// `MENU_CANCEL1`: back to the party list.
    Cancel,
    Give,
    TakeItem,
    Read,
    TakeMail,
    /// `MENU_CANCEL2`: back to the member's action menu.
    CancelToActions,
    /// `MENU_SEND_OUT`: the battle's forced replacement.
    SendOut,
    /// `MENU_SHIFT`: the battle's chosen switch.
    Shift,
    FieldMove(FieldMove),
}

/// Which battle menu a member gets (`GetPartyMenuActionsTypeInBattle`,
/// `party_menu.c:5791-5800`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BattleActions {
    /// The replacement the battle requires.
    SendOut,
    /// A switch the player chose.
    Shift,
}

fn battle_actions(facts: SlotFacts, party: &PartyRecords, kind: BattleActions) -> Vec<PartyAction> {
    if party.len() >= 2 && !facts.is_egg {
        let first = match kind {
            BattleActions::SendOut => PartyAction::SendOut,
            BattleActions::Shift => PartyAction::Shift,
        };
        vec![first, PartyAction::Summary, PartyAction::Cancel]
    } else {
        vec![PartyAction::Summary, PartyAction::Cancel]
    }
}

/// The field menu: an egg gets the fixed SUMMARY, SWITCH, CANCEL, and any
/// other member SUMMARY, its field moves in move order, SWITCH once a second
/// member exists, ITEM or MAIL, and CANCEL.
fn field_actions(facts: SlotFacts, party: &PartyRecords) -> Vec<PartyAction> {
    if facts.is_egg {
        return vec![
            PartyAction::Summary,
            PartyAction::Switch,
            PartyAction::Cancel,
        ];
    }
    let mut actions = vec![PartyAction::Summary];
    for move_id in facts.moves {
        if let Some(field_move) = FieldMove::of_move(move_id) {
            actions.push(PartyAction::FieldMove(field_move));
        }
    }
    if party.len() >= 2 {
        actions.push(PartyAction::Switch);
    }
    actions.push(if is_mail(facts.held_item) {
        PartyAction::Mail
    } else {
        PartyAction::Item
    });
    actions.push(PartyAction::Cancel);
    actions
}

/// `ItemIsMail`: `ITEM_ORANGE_MAIL` to `ITEM_RETRO_MAIL`
/// (`pokeemerald/include/constants/items.h:139-152`).
fn is_mail(item: ItemId) -> bool {
    (121..=132).contains(&item.0)
}

/// The kinds of menu a member can open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MenuKind {
    Field,
    Battle(BattleActions),
    Item,
    Mail,
}

/// An open action menu and its cursor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ActionMenu {
    pub(super) kind: MenuKind,
    pub(super) actions: Vec<PartyAction>,
    pub(super) cursor: usize,
}

impl ActionMenu {
    pub(super) fn build(kind: MenuKind, facts: SlotFacts, party: &PartyRecords) -> Self {
        let actions = match kind {
            MenuKind::Field => field_actions(facts, party),
            MenuKind::Battle(battle) => battle_actions(facts, party, battle),
            MenuKind::Item => {
                vec![
                    PartyAction::Give,
                    PartyAction::TakeItem,
                    PartyAction::CancelToActions,
                ]
            }
            MenuKind::Mail => {
                vec![
                    PartyAction::Read,
                    PartyAction::TakeMail,
                    PartyAction::CancelToActions,
                ]
            }
        };
        Self {
            kind,
            actions,
            cursor: 0,
        }
    }

    pub(super) fn selected(&self) -> PartyAction {
        self.actions[self.cursor]
    }

    /// The action B stands for: the menu's last line.
    pub(super) fn last(&self) -> PartyAction {
        self.actions[self.actions.len() - 1]
    }

    /// Moves the cursor one line. A menu of up to three lines stops at its
    /// ends and only sounds when the cursor moved; a longer one wraps and
    /// always sounds (`Menu_ProcessInputNoWrapAround_other`,
    /// `ProcessMenuInput_other`, `menu.c:1043-1095`). Returns whether to sound.
    pub(super) fn step(&mut self, down: bool) -> bool {
        let count = self.actions.len();
        let before = self.cursor;
        if count <= 3 {
            self.cursor = if down {
                (self.cursor + 1).min(count - 1)
            } else {
                self.cursor.saturating_sub(1)
            };
            self.cursor != before
        } else {
            self.cursor = if down {
                (self.cursor + 1) % count
            } else {
                (self.cursor + count - 1) % count
            };
            true
        }
    }
}
