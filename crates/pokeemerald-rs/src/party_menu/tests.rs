//! The party screen against the upstream handlers. Each submodule drives the
//! screen with button frames and checks what it returns.

use platform::Buttons;

use super::*;
use crate::party::test_records::{party, Mon, Pad};

mod battle;
mod item;
mod save_continue;
mod selection;
mod summary_return;
mod switch;

const POUND: u16 = 1;
const LEER: u16 = 43;
const CUT: u16 = 15;
const SURF: u16 = 57;
const POTION: u16 = 13;
const ORANGE_MAIL: u16 = 121;

/// A single-button frame followed by a release frame, both ticked; returns the
/// press frame's tick.
fn tap(screen: &mut PartyScreen, pad: &mut Pad, button: Buttons) -> PartyScreenTick {
    let tick = screen.tick(pad.frame(button));
    screen.tick(pad.frame(Buttons::NONE));
    tick
}

fn taps(screen: &mut PartyScreen, pad: &mut Pad, buttons: &[Buttons]) {
    for button in buttons {
        tap(screen, pad, *button);
    }
}

/// Three distinct, able members.
fn three() -> PartyRecords {
    party(vec![Mon::new(1), Mon::new(2), Mon::new(3)])
}

fn field(records: PartyRecords) -> PartyScreen {
    PartyScreen::new(PartyScreenRequest::new(PartyScreenMode::Field, records))
}
