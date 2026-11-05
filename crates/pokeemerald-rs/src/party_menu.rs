//! The party screen: the six-member list, a member's action menu, SWITCH, the
//! summary round trip, and the typed request and result its callers speak
//! (`pokeemerald/src/party_menu.c`).
//!
//! One screen serves three callers, told apart by [`PartyScreenMode`] and
//! never by drawing: the start menu's `POKéMON` ([`PartyScreenMode::Field`]),
//! and the battle's switch and item choices ([`PartyScreenMode::BattleSwitch`],
//! [`PartyScreenMode::BattleItem`]). A close returns a [`PartyScreenResult`]
//! for the caller to act on; the screen applies no item or battle effect and
//! reorders nothing in a battle, so slots always name the records the caller
//! supplied. Converting between field and battle order is the battle
//! caller's.
//!
//! The screen works on its own copy of the party. A SWITCH or a summary move
//! reorder changes that copy on the frame upstream commits it and the tick
//! returns [`PartyScreenTick::Changed`]: the host then stores
//! [`PartyScreen::records`] back, as the bag screen hands back a reordered
//! bag. Actions that leave the screen's hands (using a field move, giving or
//! taking an item, reading mail) return as [`PartyScreenTick::Host`] and the
//! screen waits for [`PartyScreen::resume`].
//!
//! Everything here is the screen's logic. Its pictures, the slide that plays
//! while SWITCH exchanges two panels, the message text, and playback of the
//! requested sounds belong to the presentation, which this module does not
//! yet have; the module neither owns nor reads any pack data.

#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the start menu and battle callers, and the screen's renderer, are the production users"
    )
)]

use platform::{ButtonState, Buttons};

use crate::key_repeat::{self, KeyRepeat};
use crate::party::records::PartyRecords;
use crate::party::sound::MenuSound;
use crate::pokemon_summary_screen::{
    PokemonSummaryScreen, SummaryMode, SummaryRequest, SummaryTick,
};

mod actions;
mod cursor;
pub(crate) mod item;
pub(crate) mod request;

pub(crate) use actions::{FieldMove, PartyAction};
pub(crate) use cursor::PartyCursor;

use actions::{ActionMenu, BattleActions, MenuKind};
use cursor::Direction;
pub(crate) use request::{
    PartyHostRequest, PartyResume, PartyScreenMode, PartyScreenRequest, PartyScreenResult,
    PartyScreenTick, SwitchRefusal, SwitchRestriction,
};

#[derive(Debug)]
enum Stage {
    /// `Task_HandleChooseMonInput` with the party in front.
    List,
    /// A selection window over the member under the cursor.
    Menu(ActionMenu),
    /// `PARTY_ACTION_SWITCH`: the member to move is `source`, and the second
    /// cursor chooses where it goes (`slotId2`).
    Switch {
        source: usize,
        target: PartyCursor,
    },
    Summary(Box<PokemonSummaryScreen>),
    /// A [`PartyHostRequest`] is out and unanswered.
    Waiting(PartyHostRequest),
    Closed(PartyScreenResult),
}

/// The open party screen.
#[derive(Debug)]
pub(crate) struct PartyScreen {
    records: PartyRecords,
    mode: PartyScreenMode,
    lr_buttons: bool,
    cursor: PartyCursor,
    /// `lastSelectedSlot`.
    side: usize,
    stage: Stage,
    repeat: KeyRepeat,
    sounds: Vec<MenuSound>,
}

impl PartyScreen {
    /// Opens the screen on the list (`InitPartyMenu`, `:489-560`).
    pub(crate) fn new(request: PartyScreenRequest) -> Self {
        let cursor = request.cursor.within(request.records.len());
        Self {
            records: request.records,
            mode: request.mode,
            lr_buttons: request.lr_buttons,
            cursor,
            side: 0,
            stage: Stage::List,
            repeat: KeyRepeat::new(key_repeat::START_DELAY),
            sounds: Vec::new(),
        }
    }

    /// The party as the screen holds it.
    pub(crate) const fn records(&self) -> &PartyRecords {
        &self.records
    }

    /// The list's cursor; while SWITCH is choosing a destination, the
    /// destination's.
    pub(crate) const fn cursor(&self) -> PartyCursor {
        match self.stage {
            Stage::Switch { target, .. } => target,
            _ => self.cursor,
        }
    }

    /// The member being moved while SWITCH is choosing a destination.
    pub(crate) const fn switching(&self) -> Option<usize> {
        match self.stage {
            Stage::Switch { source, .. } => Some(source),
            _ => None,
        }
    }

    /// The action menu showing, if one is.
    pub(crate) fn menu(&self) -> Option<(&[PartyAction], usize)> {
        match &self.stage {
            Stage::Menu(menu) => Some((&menu.actions, menu.cursor)),
            _ => None,
        }
    }

    /// The summary screen in front, if one is.
    pub(crate) fn summary(&self) -> Option<&PokemonSummaryScreen> {
        match &self.stage {
            Stage::Summary(summary) => Some(summary),
            _ => None,
        }
    }

    /// The sounds requested since the last call, oldest first.
    pub(crate) fn take_sounds(&mut self) -> Vec<MenuSound> {
        let mut sounds = std::mem::take(&mut self.sounds);
        if let Stage::Summary(summary) = &mut self.stage {
            sounds.extend(summary.take_sounds());
        }
        sounds
    }

    /// Replaces the record in `slot` with one the host changed while the
    /// screen waited, such as a member whose held item was taken.
    pub(crate) fn replace_record(&mut self, slot: usize, record: engine::save::Pokemon) {
        if let Some(held) = self.records.get_mut(slot) {
            *held = record;
        }
    }

    /// Carries on after a [`PartyHostRequest`]. GIVE leaves for the bag and
    /// READ for the mail screen, and upstream rebuilds the menu on the way
    /// back with `InitPartyMenu`, which forgets the remembered right-hand
    /// panel (`party_menu.c:507`, `CB2_GiveHoldItem` at `:3101-3105`,
    /// `:3393-3398`); a refused field move or a TAKE never leaves the menu,
    /// so those keep it.
    pub(crate) fn resume(&mut self, resume: PartyResume) {
        let Stage::Waiting(request) = self.stage else {
            return;
        };
        if matches!(
            request,
            PartyHostRequest::GiveItem { .. } | PartyHostRequest::ReadMail { .. }
        ) {
            self.side = 0;
        }
        self.stage = match resume {
            PartyResume::List => Stage::List,
            PartyResume::ActionMenu => self.member_menu(),
        };
    }

    fn selected_member(&self) -> Option<usize> {
        match self.cursor {
            PartyCursor::Mon(slot) => Some(slot),
            PartyCursor::Cancel => None,
        }
    }

    /// The action menu for the member under the cursor
    /// (`CreateSelectionWindow`, `:2652-2690`).
    fn member_menu(&self) -> Stage {
        let kind = match self.mode {
            PartyScreenMode::Field => MenuKind::Field,
            PartyScreenMode::BattleSwitch(request) if request.forced => {
                MenuKind::Battle(BattleActions::SendOut)
            }
            PartyScreenMode::BattleSwitch(_) | PartyScreenMode::BattleItem => {
                MenuKind::Battle(BattleActions::Shift)
            }
        };
        match self
            .selected_member()
            .and_then(|slot| self.records.facts(slot))
        {
            Some(facts) => Stage::Menu(ActionMenu::build(kind, facts, &self.records)),
            None => Stage::List,
        }
    }

    /// Advances the screen by one frame. Once closed it keeps reporting the
    /// close.
    pub(crate) fn tick(&mut self, buttons: ButtonState) -> PartyScreenTick {
        // Upstream counts key repeat every frame, whoever reads it.
        let repeated = self.repeat.advance(buttons);
        match std::mem::replace(&mut self.stage, Stage::List) {
            Stage::List => self.list(buttons, repeated),
            Stage::Menu(menu) => self.menu_input(menu, buttons, repeated),
            Stage::Switch { source, target } => {
                self.switch_input(source, target, buttons, repeated)
            }
            Stage::Summary(summary) => self.summary_input(summary, buttons),
            Stage::Waiting(request) => {
                self.stage = Stage::Waiting(request);
                PartyScreenTick::Open
            }
            Stage::Closed(result) => {
                self.stage = Stage::Closed(result);
                PartyScreenTick::Closed(result)
            }
        }
    }

    fn close(&mut self, result: PartyScreenResult) -> PartyScreenTick {
        self.stage = Stage::Closed(result);
        PartyScreenTick::Closed(result)
    }
}

/// What `PartyMenuButtonHandler` made of a frame (`:1455-1502`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ListInput {
    Nothing,
    Moved(Direction),
    Confirm,
    Back,
}

impl PartyScreen {
    /// `PartyMenuButtonHandler`: the repeated directions must be exactly one
    /// of the four; failing that the shoulder buttons step up and down in L/R
    /// mode; START is consumed; a direction wins over A and B; A on CANCEL is
    /// B; and a frame with A and B together is neither.
    fn read_list(&self, buttons: ButtonState, repeated: Buttons, on_cancel: bool) -> ListInput {
        let direction = [
            (Buttons::UP, Direction::Up),
            (Buttons::DOWN, Direction::Down),
            (Buttons::LEFT, Direction::Left),
            (Buttons::RIGHT, Direction::Right),
        ]
        .into_iter()
        .find_map(|(button, direction)| (repeated == button).then_some(direction))
        .or_else(|| {
            if !self.lr_buttons {
                None
            } else if repeated.intersects(Buttons::L) {
                Some(Direction::Up)
            } else if repeated.intersects(Buttons::R) {
                Some(Direction::Down)
            } else {
                None
            }
        });
        if buttons.is_newly_pressed(Buttons::START) {
            return ListInput::Nothing;
        }
        if let Some(direction) = direction {
            return ListInput::Moved(direction);
        }
        let a = buttons.is_newly_pressed(Buttons::A);
        let b = buttons.is_newly_pressed(Buttons::B);
        if a && on_cancel || b && !a {
            ListInput::Back
        } else if a && !b {
            ListInput::Confirm
        } else {
            ListInput::Nothing
        }
    }

    /// Moves `cursor` and sounds when it landed somewhere else
    /// (`UpdateCurrentPartySelection`, `:1505-1521`).
    fn move_cursor(&mut self, cursor: PartyCursor, direction: Direction) -> PartyCursor {
        let moved = cursor.moved(direction, self.records.len(), &mut self.side);
        if moved != cursor {
            self.sounds.push(MenuSound::Select);
        }
        moved
    }

    /// `HandleChooseMonSelection` and `HandleChooseMonCancel` for the action
    /// the list is in (`:1292-1403`).
    fn list(&mut self, buttons: ButtonState, repeated: Buttons) -> PartyScreenTick {
        self.stage = Stage::List;
        let on_cancel = self.cursor == PartyCursor::Cancel;
        match self.read_list(buttons, repeated, on_cancel) {
            ListInput::Nothing => PartyScreenTick::Open,
            ListInput::Moved(direction) => {
                self.cursor = self.move_cursor(self.cursor, direction);
                PartyScreenTick::Open
            }
            ListInput::Confirm => self.choose_member(),
            ListInput::Back => self.leave_list(),
        }
    }

    fn choose_member(&mut self) -> PartyScreenTick {
        let Some(slot) = self.selected_member() else {
            return self.leave_list();
        };
        match self.mode {
            PartyScreenMode::BattleItem => {
                if self.records.facts(slot).is_some_and(|facts| facts.is_egg) {
                    self.sounds.push(MenuSound::Failure);
                    PartyScreenTick::Open
                } else {
                    self.close(PartyScreenResult::ItemTarget { slot })
                }
            }
            PartyScreenMode::Field | PartyScreenMode::BattleSwitch(_) => {
                self.sounds.push(MenuSound::Select);
                self.stage = self.member_menu();
                PartyScreenTick::Open
            }
        }
    }

    fn leave_list(&mut self) -> PartyScreenTick {
        match self.mode {
            PartyScreenMode::BattleSwitch(request) if request.forced => {
                self.sounds.push(MenuSound::Failure);
                PartyScreenTick::Open
            }
            PartyScreenMode::Field => {
                self.sounds.push(MenuSound::Select);
                self.close(PartyScreenResult::ReturnToField)
            }
            PartyScreenMode::BattleSwitch(_) | PartyScreenMode::BattleItem => {
                self.sounds.push(MenuSound::Select);
                self.close(PartyScreenResult::Cancelled)
            }
        }
    }

    /// `Task_HandleSelectionMenuInput` (`:2740-2766`): A runs the line under
    /// the cursor, B runs the last line and sounds first.
    fn menu_input(
        &mut self,
        mut menu: ActionMenu,
        buttons: ButtonState,
        repeated: Buttons,
    ) -> PartyScreenTick {
        let chosen = if buttons.is_newly_pressed(Buttons::A) {
            Some(menu.selected())
        } else if buttons.is_newly_pressed(Buttons::B) {
            self.sounds.push(MenuSound::Select);
            Some(menu.last())
        } else {
            let dpad = repeated & (Buttons::UP | Buttons::DOWN | Buttons::LEFT | Buttons::RIGHT);
            if (dpad == Buttons::UP || dpad == Buttons::DOWN) && menu.step(dpad == Buttons::DOWN) {
                self.sounds.push(MenuSound::Select);
            }
            None
        };
        let Some(action) = chosen else {
            self.stage = Stage::Menu(menu);
            return PartyScreenTick::Open;
        };
        self.sounds.push(MenuSound::Select);
        self.run_action(action)
    }

    /// The `CursorCb_*` handlers.
    fn run_action(&mut self, action: PartyAction) -> PartyScreenTick {
        let slot = self.selected_member().unwrap_or(0);
        match action {
            PartyAction::Summary => {
                self.open_summary(slot);
                PartyScreenTick::Open
            }
            PartyAction::Switch => {
                self.stage = Stage::Switch {
                    source: slot,
                    target: PartyCursor::Mon(slot),
                };
                PartyScreenTick::Open
            }
            PartyAction::Item | PartyAction::Mail => {
                let kind = if action == PartyAction::Item {
                    MenuKind::Item
                } else {
                    MenuKind::Mail
                };
                let facts = self.records.facts(slot);
                self.stage = match facts {
                    Some(facts) => Stage::Menu(ActionMenu::build(kind, facts, &self.records)),
                    None => Stage::List,
                };
                PartyScreenTick::Open
            }
            PartyAction::Cancel => {
                self.stage = Stage::List;
                PartyScreenTick::Open
            }
            PartyAction::CancelToActions => {
                self.stage = self.member_menu();
                PartyScreenTick::Open
            }
            PartyAction::SendOut | PartyAction::Shift => self.try_switch_in(slot),
            PartyAction::Give => self.wait_for(PartyHostRequest::GiveItem { slot }),
            PartyAction::TakeItem => self.wait_for(PartyHostRequest::TakeItem { slot }),
            PartyAction::Read => self.wait_for(PartyHostRequest::ReadMail { slot }),
            PartyAction::TakeMail => self.wait_for(PartyHostRequest::TakeMail { slot }),
            PartyAction::FieldMove(field_move) => {
                self.wait_for(PartyHostRequest::FieldMove { slot, field_move })
            }
        }
    }

    fn wait_for(&mut self, request: PartyHostRequest) -> PartyScreenTick {
        self.stage = Stage::Waiting(request);
        PartyScreenTick::Host(request)
    }

    /// `TrySwitchInPokemon` (`:5813-5857`): the refusals in upstream's order.
    fn try_switch_in(&mut self, slot: usize) -> PartyScreenTick {
        let PartyScreenMode::BattleSwitch(request) = self.mode else {
            self.stage = Stage::List;
            return PartyScreenTick::Open;
        };
        let facts = self.records.facts(slot);
        let refusal = if facts.is_some_and(|facts| facts.hp == 0) {
            Some(SwitchRefusal::NoEnergy { slot })
        } else if request.in_battle.contains(slot) {
            Some(SwitchRefusal::AlreadyInBattle { slot })
        } else if request.already_selected == Some(slot) {
            Some(SwitchRefusal::AlreadySelected { slot })
        } else {
            request.restriction.map(|restriction| match restriction {
                SwitchRestriction::AbilityPrevents => SwitchRefusal::AbilityPrevents,
                SwitchRestriction::CantSwitch => SwitchRefusal::CantSwitchOut,
            })
        };
        match refusal {
            Some(refusal) => {
                self.stage = Stage::List;
                PartyScreenTick::Refused(refusal)
            }
            None => self.close(PartyScreenResult::SwitchIn { slot }),
        }
    }

    /// `CB2_ShowPokemonSummaryScreen` (`:2777-2787`): a battle's summary
    /// locks the moves.
    fn open_summary(&mut self, slot: usize) {
        let mode = match self.mode {
            PartyScreenMode::Field => SummaryMode::Normal,
            PartyScreenMode::BattleSwitch(_) | PartyScreenMode::BattleItem => {
                SummaryMode::LockedMoves
            }
        };
        let request = SummaryRequest {
            mode,
            index: slot,
            lr_buttons: self.lr_buttons,
        };
        self.stage = match PokemonSummaryScreen::new(self.records.clone(), request) {
            Ok(summary) => Stage::Summary(Box::new(summary)),
            Err(_) => Stage::List,
        };
    }

    /// The summary's frame, and its return (`CB2_ReturnToPartyMenuFromSummaryScreen`,
    /// `:2790-2795`): the party comes back on the last viewed member, with the
    /// menu `Task_TryCreateSelectionWindow` opens for it. The menu is a new
    /// `InitPartyMenu`, so the remembered right-hand panel starts over.
    fn summary_input(
        &mut self,
        mut summary: Box<PokemonSummaryScreen>,
        buttons: ButtonState,
    ) -> PartyScreenTick {
        let tick = summary.tick(buttons);
        let changed = summary.records() != &self.records;
        if changed {
            self.records = summary.records().clone();
        }
        self.sounds.extend(summary.take_sounds());
        match tick {
            SummaryTick::Open => {
                self.stage = Stage::Summary(summary);
            }
            SummaryTick::Closed { last_viewed } => {
                self.cursor = PartyCursor::Mon(last_viewed);
                self.side = 0;
                self.stage = self.member_menu();
            }
        }
        if changed {
            PartyScreenTick::Changed
        } else {
            PartyScreenTick::Open
        }
    }

    /// The SWITCH destination chooser (`:2822-2864`, `:3037-3046`): the same
    /// cursor and controls as the list, with A exchanging the two records and
    /// B or CANCEL abandoning the move. Either way the list resumes on the
    /// destination cursor, not on the member picked up.
    fn switch_input(
        &mut self,
        source: usize,
        target: PartyCursor,
        buttons: ButtonState,
        repeated: Buttons,
    ) -> PartyScreenTick {
        self.stage = Stage::Switch { source, target };
        let on_cancel = target == PartyCursor::Cancel;
        match self.read_list(buttons, repeated, on_cancel) {
            ListInput::Nothing => PartyScreenTick::Open,
            ListInput::Moved(direction) => {
                let target = self.move_cursor(target, direction);
                self.stage = Stage::Switch { source, target };
                PartyScreenTick::Open
            }
            ListInput::Back => {
                self.sounds.push(MenuSound::Select);
                self.finish_switch(target);
                PartyScreenTick::Open
            }
            ListInput::Confirm => {
                self.sounds.push(MenuSound::Select);
                let PartyCursor::Mon(destination) = target else {
                    self.finish_switch(target);
                    return PartyScreenTick::Open;
                };
                self.finish_switch(target);
                if destination == source {
                    PartyScreenTick::Open
                } else {
                    self.records.swap(source, destination);
                    PartyScreenTick::Changed
                }
            }
        }
    }

    /// `FinishTwoMonAction` (`:3037-3046`).
    fn finish_switch(&mut self, target: PartyCursor) {
        self.cursor = target;
        self.stage = Stage::List;
    }
}

#[cfg(test)]
mod tests;
