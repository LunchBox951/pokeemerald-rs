//! The Pokémon summary screen's interaction: page and Pokémon navigation,
//! move inspection and reordering, and the close that hands the last viewed
//! slot back to the party screen (`pokeemerald/src/pokemon_summary_screen.c`).
//!
//! The screen works on its own copy of the party's records. A move reorder is
//! written to that copy at the frame upstream commits it, and the caller reads
//! it back through [`PokemonSummaryScreen::records`]; the screen never touches
//! the caller's party.
//!
//! The four pages are INFO, SKILLS, BATTLE MOVES and CONTEST MOVES
//! (`PSS_PAGE_*`, `:50-55`). An egg shows INFO only and cannot change page
//! (`ChangePage`, `:1761-1782`); the move pages skip eggs when changing
//! Pokémon, INFO does not (`AdvanceMonIndex`, `:1696-1720`). A battle caller
//! opens the screen with [`SummaryMode::LockedMoves`], which inspects moves
//! but never reorders them (`:1138-1142`, `:1930-1935`).
//!
//! Page and Pokémon changes take effect on the frame they are accepted. The
//! scroll between pages and the staged reload between Pokémon, during which
//! upstream reads no input (`PssScrollRight`, `Task_ChangeSummaryMon`), belong
//! to the presentation, which this module does not yet have.

#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the party screen and its renderer are the production users"
    )
)]

use platform::{ButtonState, Buttons};

use crate::party::records::{swap_moves, PartyRecords, MOVE_SLOTS};
use crate::party::sound::MenuSound;

/// A summary page, in upstream's order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SummaryPage {
    Info,
    Skills,
    BattleMoves,
    ContestMoves,
}

impl SummaryPage {
    const fn previous(self) -> Option<Self> {
        match self {
            Self::Info => None,
            Self::Skills => Some(Self::Info),
            Self::BattleMoves => Some(Self::Skills),
            Self::ContestMoves => Some(Self::BattleMoves),
        }
    }

    const fn next(self) -> Option<Self> {
        match self {
            Self::Info => Some(Self::Skills),
            Self::Skills => Some(Self::BattleMoves),
            Self::BattleMoves => Some(Self::ContestMoves),
            Self::ContestMoves => None,
        }
    }
}

/// Whether the caller lets the player reorder moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SummaryMode {
    /// `SUMMARY_MODE_NORMAL`: the field party menu's summary.
    Normal,
    /// `SUMMARY_MODE_LOCK_MOVES`: the in-battle party menu's summary.
    LockedMoves,
}

/// Everything the caller supplies to open the screen.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SummaryRequest {
    pub(crate) mode: SummaryMode,
    /// The member shown first.
    pub(crate) index: usize,
    /// Whether the save's button mode is L/R, which makes the shoulder buttons
    /// turn pages (`GetLRKeysPressed`, `menu_helpers.c:252-263`).
    pub(crate) lr_buttons: bool,
}

/// Why opening the screen failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SummaryError {
    /// The party has no member to show, or `index` is not one.
    NoSuchMember(usize),
}

impl std::fmt::Display for SummaryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoSuchMember(index) => write!(f, "summary screen: no party member {index}"),
        }
    }
}

impl std::error::Error for SummaryError {}

/// The move rows' cursor. Row [`CANCEL_ROW`] is the CANCEL line below the four
/// moves.
const CANCEL_ROW: u8 = 4;
const _: () = assert!(CANCEL_ROW as usize == MOVE_SLOTS);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// `Task_HandleInput`: pages and Pokémon.
    Browse,
    /// `Task_HandleInput_MoveSelect`.
    SelectMove {
        cursor: u8,
    },
    /// `Task_HandleInput_MovePositionSwitch`: `first` is the move being
    /// moved, `cursor` where it would go.
    SwapMove {
        first: u8,
        cursor: u8,
    },
    Closed {
        last_viewed: usize,
    },
}

/// What [`PokemonSummaryScreen::tick`] decided this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SummaryTick {
    /// The screen still owns input.
    Open,
    /// The screen is done. `last_viewed` is `gLastViewedMonIndex`
    /// (`:1514-1520`), the member the party screen resumes on.
    Closed { last_viewed: usize },
}

/// The open summary screen.
#[derive(Debug)]
pub(crate) struct PokemonSummaryScreen {
    records: PartyRecords,
    mode: SummaryMode,
    lr_buttons: bool,
    index: usize,
    page: SummaryPage,
    phase: Phase,
    sounds: Vec<MenuSound>,
}

impl PokemonSummaryScreen {
    /// Opens the screen on member `request.index` of `records`, on the INFO
    /// page (`ShowPokemonSummaryScreen`, `:1100-1146`).
    ///
    /// # Errors
    ///
    /// Returns [`SummaryError::NoSuchMember`] when `request.index` is not a
    /// member of `records`.
    pub(crate) fn new(
        records: PartyRecords,
        request: SummaryRequest,
    ) -> Result<Self, SummaryError> {
        if request.index >= records.len() {
            return Err(SummaryError::NoSuchMember(request.index));
        }
        Ok(Self {
            records,
            mode: request.mode,
            lr_buttons: request.lr_buttons,
            index: request.index,
            page: SummaryPage::Info,
            phase: Phase::Browse,
            sounds: Vec::new(),
        })
    }

    /// The page showing.
    pub(crate) const fn page(&self) -> SummaryPage {
        self.page
    }

    /// The member shown.
    pub(crate) const fn index(&self) -> usize {
        self.index
    }

    /// The move row the cursor is on while a move page is being inspected, and
    /// the row a held move would drop onto while one is being moved.
    pub(crate) const fn move_cursor(&self) -> Option<u8> {
        match self.phase {
            Phase::SelectMove { cursor } | Phase::SwapMove { cursor, .. } => Some(cursor),
            Phase::Browse | Phase::Closed { .. } => None,
        }
    }

    /// The move picked up to be reordered, if one is.
    pub(crate) const fn held_move(&self) -> Option<u8> {
        match self.phase {
            Phase::SwapMove { first, .. } => Some(first),
            _ => None,
        }
    }

    /// The party as the screen holds it, including any reorder made here.
    pub(crate) const fn records(&self) -> &PartyRecords {
        &self.records
    }

    /// The sounds requested since the last call, oldest first.
    pub(crate) fn take_sounds(&mut self) -> Vec<MenuSound> {
        std::mem::take(&mut self.sounds)
    }

    /// Advances the screen by one frame. Once closed it keeps reporting the
    /// close.
    pub(crate) fn tick(&mut self, buttons: ButtonState) -> SummaryTick {
        match self.phase {
            Phase::Browse => self.browse(buttons),
            Phase::SelectMove { cursor } => self.select_move(buttons, cursor),
            Phase::SwapMove { first, cursor } => self.swap_move(buttons, first, cursor),
            Phase::Closed { .. } => {}
        }
        match self.phase {
            Phase::Closed { last_viewed } => SummaryTick::Closed { last_viewed },
            _ => SummaryTick::Open,
        }
    }

    fn pressed(buttons: ButtonState, button: Buttons) -> bool {
        buttons.is_newly_pressed(button)
    }

    /// `Task_HandleInput` (`:1532-1574`): one branch per frame, in this order.
    fn browse(&mut self, buttons: ButtonState) {
        let lr_buttons = self.lr_buttons;
        let shoulder = |button| lr_buttons && Self::pressed(buttons, button);
        if Self::pressed(buttons, Buttons::UP) {
            self.change_member(-1);
        } else if Self::pressed(buttons, Buttons::DOWN) {
            self.change_member(1);
        } else if Self::pressed(buttons, Buttons::LEFT) || shoulder(Buttons::L) {
            self.change_page(SummaryPage::previous);
        } else if Self::pressed(buttons, Buttons::RIGHT) || shoulder(Buttons::R) {
            self.change_page(SummaryPage::next);
        } else if Self::pressed(buttons, Buttons::A) {
            match self.page {
                SummaryPage::Skills => {}
                SummaryPage::Info => self.close(),
                SummaryPage::BattleMoves | SummaryPage::ContestMoves => {
                    self.sounds.push(MenuSound::Select);
                    self.phase = Phase::SelectMove { cursor: 0 };
                }
            }
        } else if Self::pressed(buttons, Buttons::B) {
            self.close();
        }
    }

    fn close(&mut self) {
        self.sounds.push(MenuSound::Select);
        self.phase = Phase::Closed {
            last_viewed: self.index,
        };
    }

    fn is_egg(&self, index: usize) -> bool {
        self.records.facts(index).is_some_and(|facts| facts.is_egg)
    }

    /// `ChangeSummaryPokemon` and `AdvanceMonIndex` (`:1578-1626`,
    /// `:1696-1720`): no wrap, and the move pages and SKILLS skip eggs.
    fn change_member(&mut self, delta: isize) {
        let last = self.records.len() - 1;
        let step = |from: usize| from.checked_add_signed(delta).filter(|next| *next <= last);
        let mut next = step(self.index);
        if self.page != SummaryPage::Info {
            while let Some(candidate) = next.filter(|candidate| self.is_egg(*candidate)) {
                next = step(candidate);
            }
        }
        if let Some(next) = next {
            self.sounds.push(MenuSound::Select);
            self.index = next;
        }
    }

    /// `ChangePage` (`:1761-1782`): an egg stays on INFO; the ends do not wrap.
    fn change_page(&mut self, toward: fn(SummaryPage) -> Option<SummaryPage>) {
        if self.is_egg(self.index) {
            return;
        }
        if let Some(page) = toward(self.page) {
            self.sounds.push(MenuSound::Select);
            self.page = page;
        }
    }

    fn moves(&self) -> [u16; MOVE_SLOTS] {
        self.records
            .facts(self.index)
            .map_or([0; MOVE_SLOTS], |facts| facts.moves)
    }

    /// `ChangeSelectedMove` (`:1990-2040`): the cursor steps over empty moves
    /// and wraps, and `last` is the highest row it may reach: the CANCEL row
    /// while inspecting, the fourth move while reordering. The step always
    /// sounds, even when the cursor ends where it began.
    fn stepped_cursor(&mut self, cursor: u8, direction: i8, last: u8) -> u8 {
        self.sounds.push(MenuSound::Select);
        let moves = self.moves();
        let mut row = i16::from(cursor);
        for _ in 0..MOVE_SLOTS {
            row += i16::from(direction);
            if row > i16::from(last) {
                row = 0;
            } else if row < 0 {
                row = i16::from(last);
            }
            let landed = usize::try_from(row).unwrap_or(0);
            if row == i16::from(CANCEL_ROW) || moves[landed] != 0 {
                break;
            }
        }
        u8::try_from(row).unwrap_or(0)
    }

    /// `Task_HandleInput_MoveSelect` (`:1911-1950`).
    fn select_move(&mut self, buttons: ButtonState, cursor: u8) {
        if Self::pressed(buttons, Buttons::UP) {
            let cursor = self.stepped_cursor(cursor, -1, CANCEL_ROW);
            self.phase = Phase::SelectMove { cursor };
        } else if Self::pressed(buttons, Buttons::DOWN) {
            let cursor = self.stepped_cursor(cursor, 1, CANCEL_ROW);
            self.phase = Phase::SelectMove { cursor };
        } else if Self::pressed(buttons, Buttons::A) {
            if self.mode == SummaryMode::LockedMoves || cursor == CANCEL_ROW {
                self.sounds.push(MenuSound::Select);
                self.phase = Phase::Browse;
            } else if self.moves()[1..].iter().any(|id| *id != 0) {
                self.sounds.push(MenuSound::Select);
                self.phase = Phase::SwapMove {
                    first: cursor,
                    cursor,
                };
            } else {
                self.sounds.push(MenuSound::Failure);
            }
        } else if Self::pressed(buttons, Buttons::B) {
            self.sounds.push(MenuSound::Select);
            self.phase = Phase::Browse;
        }
    }

    /// `Task_HandleInput_MovePositionSwitch` and `ExitMovePositionSwitchMode`
    /// (`:1953-2025`): A on another move swaps the two; A on the same move and
    /// B put it back. The cursor returns to the move rows on the landing row.
    fn swap_move(&mut self, buttons: ButtonState, first: u8, cursor: u8) {
        if Self::pressed(buttons, Buttons::UP) {
            let cursor = self.stepped_cursor(cursor, -1, CANCEL_ROW - 1);
            self.phase = Phase::SwapMove { first, cursor };
        } else if Self::pressed(buttons, Buttons::DOWN) {
            let cursor = self.stepped_cursor(cursor, 1, CANCEL_ROW - 1);
            self.phase = Phase::SwapMove { first, cursor };
        } else if Self::pressed(buttons, Buttons::A) {
            self.sounds.push(MenuSound::Select);
            if first == cursor {
                self.phase = Phase::SelectMove { cursor: first };
            } else {
                if let Some(record) = self.records.get_mut(self.index) {
                    swap_moves(record, usize::from(first), usize::from(cursor));
                }
                self.phase = Phase::SelectMove { cursor };
            }
        } else if Self::pressed(buttons, Buttons::B) {
            self.sounds.push(MenuSound::Select);
            self.phase = Phase::SelectMove { cursor: first };
        }
    }
}

#[cfg(test)]
mod tests;
