//! A SWITCH made on the party screen survives the start menu's SAVE and a
//! continued game's reopened screen, through the real save medium.

use engine::save::{Pokemon, SaveBlock1, SaveBlock2, SaveFile};
use engine::text::render::TextSpeed;
use engine::text::Token;
use platform::ButtonState;

use super::*;
use crate::game_save::{SaveFileStatus, SaveLineage, SaveSlot, StoreOutcome};
use crate::party::records::PARTY_SIZE;
use crate::start_menu::{synthetic_start_menu, SaveMode, SaveTarget, StartMenuOutcome};

/// Frames one SAVE flow gets before the test calls it wedged.
const FRAME_BUDGET: usize = 4_000;

struct Scratch {
    dir: std::path::PathBuf,
}

impl Scratch {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "pokeemerald-rs-party-menu-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        drop(std::fs::remove_dir_all(&dir));
        std::fs::create_dir_all(&dir).expect("scratch directory must be creatable");
        Self { dir }
    }

    fn slot(&self) -> SaveSlot {
        SaveSlot::at(SaveFile::at(self.dir.join("pokeemerald.sav")))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        drop(std::fs::remove_dir_all(&self.dir));
    }
}

/// The start menu's save target over a real slot: a write is a stored save,
/// never a flag.
struct SlotTarget {
    slot: SaveSlot,
    block1: SaveBlock1,
    block2: SaveBlock2,
    lineage: SaveLineage,
    stores: usize,
}

impl SaveTarget for SlotTarget {
    fn boot_status(&self) -> SaveFileStatus {
        self.slot.boot_status()
    }

    fn different_save_file(&self) -> bool {
        false
    }

    fn player_name(&self) -> Vec<Token> {
        "STU".chars().map(Token::Char).collect()
    }

    fn player_text_speed(&self) -> TextSpeed {
        TextSpeed::Mid
    }

    fn try_saving_data(&mut self, _mode: SaveMode) -> bool {
        self.stores += 1;
        matches!(
            self.slot.store(&self.block1, &self.block2, self.lineage),
            Ok(StoreOutcome::Written)
        )
    }
}

/// Confirms SAVE on the start menu and answers every prompt YES.
fn save_through_start_menu(target: &mut SlotTarget) {
    let mut menu = synthetic_start_menu();
    for _ in 0..FRAME_BUDGET {
        let mut buttons = ButtonState::new();
        buttons.update(Buttons::A);
        if menu.tick(buttons, target) == StartMenuOutcome::Closed {
            return;
        }
    }
    panic!("the save flow must finish within {FRAME_BUDGET} frames");
}

fn party_bytes(block1: &SaveBlock1) -> Vec<Vec<u8>> {
    block1
        .player_party
        .iter()
        .map(|record| record.to_bytes().to_vec())
        .collect()
}

fn loaded_records(block1: &SaveBlock1) -> PartyRecords {
    PartyRecords::new(&block1.player_party, usize::from(block1.player_party_count))
        .expect("a saved count fits the party")
}

#[test]
fn a_switch_and_a_summary_visit_persist_through_save_and_continue() {
    let scratch = Scratch::new();
    let mut slot = scratch.slot();
    let fresh = slot.load();
    assert!(!fresh.status.menu_shows_continue());

    let original = three();
    let mut block1 = SaveBlock1 {
        player_party: *original.slots(),
        player_party_count: 3,
        ..SaveBlock1::default()
    };

    // Switch member 0 with member 2, then look at a summary and back out.
    let mut screen = field(loaded_records(&block1));
    let mut pad = Pad::new();
    taps(
        &mut screen,
        &mut pad,
        &[Buttons::A, Buttons::DOWN, Buttons::A],
    );
    taps(&mut screen, &mut pad, &[Buttons::DOWN, Buttons::DOWN]);
    assert_eq!(
        tap(&mut screen, &mut pad, Buttons::A),
        PartyScreenTick::Changed
    );
    block1.player_party = *screen.records().slots();
    taps(
        &mut screen,
        &mut pad,
        &[Buttons::A, Buttons::A, Buttons::B, Buttons::B],
    );
    assert_eq!(
        tap(&mut screen, &mut pad, Buttons::B),
        PartyScreenTick::Closed(PartyScreenResult::ReturnToField)
    );
    assert_eq!(
        screen.records().slots(),
        &block1.player_party,
        "the visit changed nothing"
    );

    let expected = {
        let o = original.slots();
        let mut slots = [Pokemon::default(); PARTY_SIZE];
        slots[..3].copy_from_slice(&[o[2], o[1], o[0]]);
        slots
    };
    assert_eq!(block1.player_party, expected);

    // SAVE from the start menu.
    let mut target = SlotTarget {
        slot,
        block1: block1.clone(),
        block2: fresh.block2,
        lineage: SaveLineage::NewGame,
        stores: 0,
    };
    save_through_start_menu(&mut target);
    assert_eq!(target.stores, 1);
    drop(target);

    // Continue: a new session reads the file back.
    let mut slot = scratch.slot();
    let continued = slot.load();
    assert!(continued.status.menu_shows_continue());
    assert_eq!(continued.block1.player_party_count, 3);
    assert_eq!(party_bytes(&continued.block1), party_bytes(&block1));

    // The reopened screen shows the switched order, and saving it again with
    // no edits leaves the file as it was.
    let reopened = field(loaded_records(&continued.block1));
    assert_eq!(reopened.records().slots(), &expected);
    let mut target = SlotTarget {
        slot,
        block1: continued.block1,
        block2: continued.block2,
        lineage: SaveLineage::Continued,
        stores: 0,
    };
    save_through_start_menu(&mut target);
    assert_eq!(target.stores, 1);
    drop(target);

    let again = scratch.slot().load();
    assert_eq!(party_bytes(&again.block1), party_bytes(&block1));
}
