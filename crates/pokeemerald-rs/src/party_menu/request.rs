//! The party screen's typed boundary: what a caller asks for, what the screen
//! hands back each frame, and how a closed screen ends.

use crate::party::records::PartyRecords;

use super::{FieldMove, PartyCursor};

/// A set of party slots, for a battle caller to name the members it has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct PartySlotSet(u8);

impl PartySlotSet {
    /// The set holding exactly `slots`.
    pub(crate) fn of(slots: &[usize]) -> Self {
        Self(slots.iter().fold(0, |bits, slot| bits | (1 << slot)))
    }

    pub(super) fn contains(self, slot: usize) -> bool {
        self.0 & (1 << slot) != 0
    }
}

/// Why a battle switch cannot be made even to an able member
/// (`PARTY_ACTION_ABILITY_PREVENTS`, `PARTY_ACTION_CANT_SWITCH`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SwitchRestriction {
    /// An opposing ability traps the battler.
    AbilityPrevents,
    /// A trapping condition holds the battler.
    CantSwitch,
}

/// A battle's request to choose a member to send in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BattleSwitchRequest {
    /// Whether the battle requires a replacement (`PARTY_ACTION_SEND_OUT`),
    /// which B cannot leave, or the player chose to switch.
    pub(crate) forced: bool,
    /// The members already in the fight.
    pub(crate) in_battle: PartySlotSet,
    /// A member already picked to replace another this turn
    /// (`prevSelectedPartySlot`).
    pub(crate) already_selected: Option<usize>,
    pub(crate) restriction: Option<SwitchRestriction>,
}

/// Which caller opened the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PartyScreenMode {
    /// The start menu's `POKéMON` (`PARTY_MENU_TYPE_FIELD`): SUMMARY, SWITCH,
    /// ITEM and CANCEL, and B returns to the start menu.
    Field,
    /// The battle's switch or forced replacement: SEND OUT or SHIFT, SUMMARY
    /// (with moves locked) and CANCEL.
    BattleSwitch(BattleSwitchRequest),
    /// The battle's item choice: A on a member that is not an egg picks it.
    BattleItem,
}

/// Everything the host supplies to open the screen.
#[derive(Debug, Clone)]
pub(crate) struct PartyScreenRequest {
    pub(crate) mode: PartyScreenMode,
    /// The party as the host holds it.
    pub(crate) records: PartyRecords,
    /// Where the cursor starts; the first member by default.
    pub(crate) cursor: PartyCursor,
    /// Whether the save's button mode is L/R, which makes the shoulder buttons
    /// step the cursor (`GetLRKeysPressedAndHeld`, `menu_helpers.c:265-275`).
    pub(crate) lr_buttons: bool,
}

impl PartyScreenRequest {
    /// A request with the cursor on the first member and the A button mode.
    pub(crate) fn new(mode: PartyScreenMode, records: PartyRecords) -> Self {
        Self {
            mode,
            records,
            cursor: PartyCursor::Mon(0),
            lr_buttons: false,
        }
    }
}

/// How a closed screen ends, for the caller to route.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PartyScreenResult {
    /// The field screen was left: the host reopens the start menu.
    ReturnToField,
    /// A battle screen was left without a choice.
    Cancelled,
    /// A member was chosen to be sent in.
    SwitchIn { slot: usize },
    /// A member was chosen to receive the battle item.
    ItemTarget { slot: usize },
}

/// Why a battle switch was refused (`TrySwitchInPokemon`,
/// `party_menu.c:5813-5857`), for the presentation to print. An egg is never
/// offered SEND OUT or SHIFT, so its refusal is unreachable here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SwitchRefusal {
    /// The member has fainted.
    NoEnergy {
        slot: usize,
    },
    /// The member is already fighting.
    AlreadyInBattle {
        slot: usize,
    },
    /// The member is already picked to switch in.
    AlreadySelected {
        slot: usize,
    },
    AbilityPrevents,
    CantSwitchOut,
}

/// An action the screen hands to the host before it can go on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PartyHostRequest {
    /// A field move was chosen. The host checks badges and surroundings and
    /// either runs it or refuses (`CursorCb_FieldMove`, `:3702-3760`).
    FieldMove { slot: usize, field_move: FieldMove },
    /// GIVE: the host opens the bag to pick an item for this member and
    /// answers with [`super::PartyScreen::resume`].
    GiveItem { slot: usize },
    /// TAKE under ITEM: the host runs [`take_held_item`].
    TakeItem { slot: usize },
    /// READ under MAIL.
    ReadMail { slot: usize },
    /// TAKE under MAIL.
    TakeMail { slot: usize },
}

/// Where the screen goes when the host has finished a [`PartyHostRequest`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PartyResume {
    /// Back to the list (`Task_ReturnToChooseMonAfterText`).
    List,
    /// Back to the member's action menu (`Task_TryCreateSelectionWindow`),
    /// as when the player backs out of the bag.
    ActionMenu,
}

/// What [`PartyScreen::tick`] decided this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PartyScreenTick {
    /// The screen still owns input.
    Open,
    /// The party changed; the host stores [`PartyScreen::records`].
    Changed,
    /// A battle switch was refused. The list is showing again, but upstream
    /// holds list input until the refusal text has printed
    /// (`Task_ReturnToChooseMonAfterText`, `party_menu.c:1745-1761`), so the
    /// host stops ticking the screen while it shows the text.
    Refused(SwitchRefusal),
    /// The screen waits for [`PartyScreen::resume`].
    Host(PartyHostRequest),
    /// The screen is done; the host drops it and acts on the result.
    Closed(PartyScreenResult),
}
