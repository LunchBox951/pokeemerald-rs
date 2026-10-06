//! I-6 (issues #214/#232) shared fixtures and drivers for the `save_continue_*_tests`
//! modules.
//!
//! These drive the *production* path end to end — the same
//! [`OverworldPhase::step`] the windowed game runs, the same
//! `OverworldPhase::advance_start_menu_frame` its `START` press reaches,
//! the same [`crate::start_menu::StartMenu`] state machine and
//! [`crate::game_save::SaveSlot::store`] behind its `SAVE` action, the same
//! [`SaveSlot::load`](crate::game_save::SaveSlot::load) its boot calls, and
//! the same [`OverworldPhase::from_saved`]
//! [`OverworldPhase::continue_saved_game`] builds from — with three
//! substitutions that keep them runnable in CI, where there is no extracted
//! asset pack:
//!
//! * the room is `crate::overworld::tests::synthetic_scene`'s flat, open
//!   grid rather than a pack-decoded one (the same fixture
//!   `crate::flow::overworld_phase::step_tests` already walks the player
//!   around),
//!   paired with a *real* map id so every `MapHeaderTable`/`MapEventsTable`
//!   lookup on the way resolves;
//! * the start menu is opened through
//!   `OverworldPhase::open_synthetic_start_menu` — blank glyph sheet and
//!   blank window frames, everything else (the item list, the cursor, the
//!   whole `sSaveDialogCallback` chain, and the write itself) production;
//!   and
//! * the save file is a per-test scratch path, passed in as a
//!   [`crate::game_save::SaveSlot`] value rather than read from the
//!   process-wide environment.
//!
//! The production steps they cannot take are
//! [`OverworldPhase::continue_saved_game`]'s own
//! `crate::overworld::load_room` call and `crate::start_menu::open`,
//! both of which need the pack. The `#[ignore]`d cases in
//! [`super::save_continue_real_pack_tests`] close that gap on the real-pack lane.
//!
//! [`OverworldPhase::step`]: crate::flow::overworld_phase::OverworldPhase::step

use super::overworld_phase::OverworldPhase;
use super::tests::{held, pressed};
use crate::game_save::SaveSlot;
use crate::new_game;
use crate::start_menu::StartMenuItem;
use engine::event_data::EventData;
use engine::overworld::{Direction, PlayerState};
use engine::save::{BoxPokemon, Pokemon, SaveBlock1, SaveBlock2, WarpData};
use platform::Buttons;

/// `FLAG_RECEIVED_RUNNING_SHOES` (`pokeemerald/include/constants/flags.h:300`)
/// — an ordinary flag no new-game initialization sets, so setting it is a
/// real mid-play mutation rather than something the reloaded blocks would
/// have anyway.
const FLAG_RECEIVED_RUNNING_SHOES: u16 = 0x112;

/// `VAR_REPEL_STEP_COUNT` (`pokeemerald/include/constants/vars.h:51`) — an
/// ordinary (non-temp) var, likewise untouched by new-game init.
const VAR_REPEL_STEP_COUNT: u16 = 0x4021;

/// How many frames a save flow gets before the fixture calls it wedged.
/// Deliberately generous: `gText_DifferentSaveFile`'s four-page WARNING is
/// ~200 glyphs, and `TextSpeed::Mid` reveals one every four frames, so the
/// longest flow here is well over a thousand frames before the success
/// message's own `SaveStartTimer` 60 even starts.
pub(super) const SAVE_FLOW_FRAME_BUDGET: usize = 4_000;

/// A brand-new game in the protagonist's bedroom, on a synthetic room grid
/// (module docs). Goes through `OverworldPhase::for_test` -> `::new`, i.e.
/// through `new_game::init_save_blocks`, so the starting state is the real
/// one.
pub(super) fn new_game_phase() -> OverworldPhase {
    OverworldPhase::for_test(
        crate::overworld::tests::synthetic_scene(10, 10),
        new_game::SPAWN_MAP_ID,
        PlayerState::new(
            new_game::SPAWN_POSITION,
            new_game::SPAWN_ELEVATION,
            new_game::SPAWN_FACING,
        ),
        None,
    )
}

/// Everything the round trip asserts on, read out of a phase.
///
/// `lead` is the *battle-facing* mon (`OverworldPhase::party_lead`), not
/// the save block's bytes: that is the value a continued session actually
/// fights with, and re-deriving it from
/// [`new_game::provisional_starter`] is exactly the stand-in issue #232
/// removed.
#[derive(Debug, PartialEq)]
pub(super) struct Snapshot {
    pub(super) map: assets::MapId,
    pub(super) position: (i32, i32),
    pub(super) facing: Direction,
    pub(super) elevation: u8,
    pub(super) money: u32,
    pub(super) running_shoes: bool,
    pub(super) repel_steps: u16,
    /// `gSaveBlock1Ptr->playerPartyCount` as it sits in the block --
    /// `SavePlayerParty`'s own output (`src/load_save.c:180-186`). Pinned
    /// by name alongside `lead` because the two can disagree: a decode that
    /// silently dropped the mon would still leave `lead` matching a
    /// re-derived starter, and a count that never got written would still
    /// leave the bytes on disk.
    pub(super) party_count: u8,
    pub(super) lead: Option<battle::BattlePokemon>,
    pub(super) player_name: [u8; engine::save::block::PLAYER_NAME_BUF_LEN],
    pub(super) trainer_id: [u8; engine::save::block::TRAINER_ID_LENGTH],
    pub(super) encryption_key: u32,
}

pub(super) fn snapshot(phase: &OverworldPhase) -> Snapshot {
    let flags: &EventData = &phase.save1.event_data;
    Snapshot {
        map: phase.map_id,
        position: phase.player.position(),
        facing: phase.player.facing(),
        elevation: phase.player.elevation(),
        money: phase.save1.money,
        running_shoes: flags.flag_get(FLAG_RECEIVED_RUNNING_SHOES).unwrap(),
        repel_steps: flags.var_get(VAR_REPEL_STEP_COUNT).unwrap(),
        party_count: phase.save1.player_party_count,
        lead: phase.party_lead.clone(),
        player_name: phase.save2.player_name,
        trainer_id: phase.save2.player_trainer_id,
        encryption_key: phase.save2.encryption_key,
    }
}

/// Run input-free frames until no step is in flight: the start menu does
/// not open while the player is moving
/// (`OverworldPhase::start_menu_may_open`, #230 review round five), and
/// these fixtures save from a player at rest.
pub(super) fn settle(phase: &mut OverworldPhase) {
    for _ in 0..20 {
        if !phase.mid_step() {
            return;
        }
        phase.step(held(Buttons::NONE));
    }
    panic!("the fixture must come to rest before it saves");
}

/// Play a little: walk south, then west (so the player ends up facing a
/// direction the tile-derived `DIR_SOUTH` fallback would *not* produce),
/// then make the state unmistakably mid-game — a flag, a var, spent money,
/// a damaged party lead, and a nonzero encryption key (so the save's
/// *encrypted* fields are exercised too, not just its plaintext ones).
pub(super) fn play_a_bit(phase: &mut OverworldPhase) {
    for _ in 0..40 {
        phase.step(held(Buttons::DOWN));
    }
    for _ in 0..40 {
        phase.step(held(Buttons::LEFT));
    }
    settle(phase);
    assert_ne!(
        phase.player.position(),
        new_game::SPAWN_POSITION,
        "the fixture must actually walk the player off the spawn tile"
    );
    assert_eq!(
        phase.player.facing(),
        Direction::West,
        "the fixture must end facing somewhere the DIR_SOUTH fallback is not"
    );

    assert!(
        !phase
            .save1
            .event_data
            .flag_get(FLAG_RECEIVED_RUNNING_SHOES)
            .unwrap(),
        "the chosen flag must start clear, or setting it proves nothing"
    );
    phase
        .save1
        .event_data
        .flag_set(FLAG_RECEIVED_RUNNING_SHOES)
        .unwrap();
    phase
        .save1
        .event_data
        .var_set(VAR_REPEL_STEP_COUNT, 37)
        .unwrap();

    phase.save2.encryption_key = 0x0BAD_F00D;
    phase.save1.money = 1_234;
    phase.party_lead = Some(a_damaged_lead());
}

/// The provisional starter after a fight it did not walk away from
/// unscathed: HP spent and one move's PP spent. A full-HP, full-PP lead
/// would round-trip just as happily through an encoder that dropped both.
pub(super) fn a_damaged_lead() -> battle::BattlePokemon {
    let mut lead = new_game::provisional_starter();
    lead.apply_damage(5);
    lead.deduct_pp(0).unwrap();
    lead
}

/// A recognizable, checksum-valid serialized party member whose bytes can
/// prove a dormant slot survived a continue/save cycle untouched.
pub(super) fn dormant_party_member(marker: u32) -> Pokemon {
    let bytes = marker.to_le_bytes();
    let low = u16::from_le_bytes([bytes[0], bytes[1]]);
    let high = u16::from_le_bytes([bytes[2], bytes[3]]);
    Pokemon {
        box_data: BoxPokemon::new(marker, marker.rotate_left(7)),
        status: marker.rotate_right(3),
        level: bytes[0],
        mail: bytes[1],
        hp: low,
        max_hp: high,
        attack: low.wrapping_add(1),
        defense: low.wrapping_add(2),
        speed: low.wrapping_add(3),
        special_attack: low.wrapping_add(4),
        special_defense: low.wrapping_add(5),
    }
}

/// Drive an already-open start menu until it closes (or its SAVE flow is
/// cancelled back to the item list), answering each Yes/No prompt from
/// `answers` in order — `true` is YES — and defaulting to YES once
/// `answers` runs out.
///
/// Returns the cursor row each prompt *opened* on (`0` = YES, `1` = NO),
/// which is the observable difference between upstream's two overwrite
/// prompts: `DisplayYesNoMenuDefaultYes` for the ordinary one,
/// `DisplayYesNoMenuWithDefault(1)` for `gText_DifferentSaveFile`'s
/// WARNING.
pub(super) fn drive_start_menu(
    phase: &mut OverworldPhase,
    save_slot: &mut SaveSlot,
    answers: &[bool],
) -> Vec<u8> {
    let mut opened_on: Vec<u8> = Vec::new();
    let mut answered = 0usize;
    let mut entered_save_flow = false;
    for _ in 0..SAVE_FLOW_FRAME_BUDGET {
        let Some(menu) = phase.start_menu() else {
            return opened_on;
        };
        if menu.saving() {
            entered_save_flow = true;
        } else if entered_save_flow {
            // `SAVE_CANCELED`: the flow put the item list back up. Stop
            // here rather than picking SAVE again forever.
            return opened_on;
        }
        let buttons = match menu.yes_no_cursor() {
            Some(cursor) => {
                if opened_on.len() == answered {
                    opened_on.push(cursor);
                }
                let wants_yes = answers.get(answered).copied().unwrap_or(true);
                let desired = u8::from(!wants_yes);
                match cursor.cmp(&desired) {
                    std::cmp::Ordering::Equal => {
                        answered += 1;
                        pressed(Buttons::A)
                    }
                    std::cmp::Ordering::Greater => pressed(Buttons::UP),
                    std::cmp::Ordering::Less => pressed(Buttons::DOWN),
                }
            }
            // No prompt waiting: A advances whatever message is printing,
            // picks SAVE off the item list, and dismisses the result
            // message once the write is done.
            None => pressed(Buttons::A),
        };
        assert!(
            phase.advance_start_menu_frame(buttons, save_slot),
            "an open start menu owns its frame"
        );
    }
    panic!("the save flow must terminate within {SAVE_FLOW_FRAME_BUDGET} frames");
}

/// `START` -> `SAVE` -> YES to everything, through the real menu.
pub(super) fn save_from_the_start_menu(
    phase: &mut OverworldPhase,
    save_slot: &mut SaveSlot,
) -> Vec<u8> {
    phase.open_synthetic_start_menu();
    assert_eq!(
        phase.start_menu().unwrap().selected(),
        StartMenuItem::Save,
        "SAVE is the first item, so a fresh menu opens on it"
    );
    let prompts = drive_start_menu(phase, save_slot, &[]);
    assert!(
        phase.start_menu().is_none(),
        "a completed save closes the start menu"
    );
    prompts
}

/// A block pair whose `location` and `continue_game_warp` name different,
/// real maps, so a continue that lands at the wrong one is visible.
pub(super) fn blocks_with_divergent_continue_game_warp(flagged: bool) -> (SaveBlock1, SaveBlock2) {
    let block1 = SaveBlock1 {
        location: WarpData {
            map_group: 1,
            map_num: 0,
            warp_id: -1,
            x: 3,
            y: 4,
        },
        continue_game_warp: WarpData {
            map_group: new_game::SPAWN_MAP_GROUP,
            map_num: new_game::SPAWN_MAP_NUM,
            warp_id: -1,
            x: 5,
            y: 6,
        },
        ..SaveBlock1::default()
    };
    let block2 = SaveBlock2 {
        special_save_warp_flags: if flagged {
            SaveBlock2::CONTINUE_GAME_WARP | 0x80
        } else {
            0x80
        },
        ..SaveBlock2::default()
    };
    (block1, block2)
}
