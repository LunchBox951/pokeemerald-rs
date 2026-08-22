//! `crate::flow::route103_rival` (issue #237), pinned the way
//! `crate::flow::first_battle`'s tests pin the scripted first battle: real
//! [`engine::rng::Rng`], real `gTrainers`/`gSpeciesNames` data, exact draw
//! counts for the parts this module adds. The turn-by-turn RNG math (the
//! trainer AI's per-script draws, the send-out, exp and money) is pinned by
//! `crates/battle/tests/turn_engine/trainer_battle.rs` and
//! `crates/battle/src/battle/trainer_ai.rs` against scripted streams; these
//! tests are about the new glue — `CreateNPCTrainerParty`'s seeded
//! personality, the party built off the real generator, and the driver's
//! action policy.

use assets::trainers::{TrainerId, TrainerParty, TrainerTable};
use assets::{MoveId, SpeciesId};
use battle::{trainer_data, trainer_money, BattleOutcome, BattlePokemon, Dex, Ivs};
use engine::rng::Rng;

use super::{route103_rival_for, PlayerStarter, Rival};
// The construction/driver -- `trainer_party_personalities`,
// `start_npc_trainer_battle`, `advance_npc_trainer_battle`, and the
// `NpcTrainerBattleError` they report -- all live in `npc_trainer_battle`
// since issue #264's construction split (`route103_rival`'s own module
// docs). Issue #347 retired this module's thin
// `start_route103_rival_battle`/`advance_route103_rival_battle`/
// `RivalBattleError` pass-throughs, so every test below imports the real
// names directly, the same way the personality test already did.
use crate::flow::npc_trainer_battle::{
    advance_npc_trainer_battle, start_npc_trainer_battle, trainer_party_personalities,
    NpcTrainerBattleError,
};

const TREECKO: u16 = 277;
const TORCHIC: u16 = 280;
const MUDKIP: u16 = 283;
const POUND: MoveId = MoveId(1);
const LEER: MoveId = MoveId(43);
const TACKLE: MoveId = MoveId(33);
const SLASH: MoveId = MoveId(163);

/// A max-IV, fixed-personality player mon — deterministic stats, mirroring
/// `first_battle/tests.rs`'s own helper, so a scenario's outcome depends on
/// the *rival's* real seeded stats and not on the player's.
fn player_mon(species: u16, level: u8, moves: Vec<MoveId>) -> BattlePokemon {
    let ivs = Ivs {
        hp: battle::MAX_IV,
        attack: battle::MAX_IV,
        defense: battle::MAX_IV,
        speed: battle::MAX_IV,
        sp_attack: battle::MAX_IV,
        sp_defense: battle::MAX_IV,
    };
    BattlePokemon::new(&Dex::new(), SpeciesId(species), level, ivs, 0, moves)
        .expect("player mon: species/moves must be in the dex")
}

/// The six `TRAINER_*_ROUTE_103_*` ids, and the fact that each one's party
/// really is the level-5 starter that beats the player's.
#[test]
fn every_route_103_rival_fields_the_type_advantaged_level_5_starter() {
    let table = TrainerTable::new();
    // (rival, player's starter) -> the rival's own species.
    let expected = [
        (Rival::Brendan, PlayerStarter::Mudkip, TREECKO),
        (Rival::Brendan, PlayerStarter::Treecko, TORCHIC),
        (Rival::Brendan, PlayerStarter::Torchic, MUDKIP),
        (Rival::May, PlayerStarter::Mudkip, TREECKO),
        (Rival::May, PlayerStarter::Treecko, TORCHIC),
        (Rival::May, PlayerStarter::Torchic, MUDKIP),
    ];
    for (rival, starter, species) in expected {
        let id = route103_rival_for(rival, starter);
        let data = table.trainer(id).expect("a real TRAINER_* id");
        let TrainerParty::NoItemDefaultMoves(party) = data.party else {
            panic!("every Route 103 rival is NO_ITEM_DEFAULT_MOVES");
        };
        assert_eq!(party.len(), 1, "{id:?} must field exactly one mon");
        assert_eq!(party[0].species, SpeciesId(species), "{id:?}");
        assert_eq!(party[0].lvl, 5, "{id:?}");
        assert_eq!(party[0].iv, 0, "{id:?} runs on all-zero IVs");
        assert_eq!(data.class.index(), 0x32, "TRAINER_CLASS_RIVAL");
    }
}

/// The seeded personality, computed by hand from the upstream formula and
/// the Gen-3 charmap — the pin that this port hashes the *game's* bytes
/// rather than ASCII, and therefore derives the same nature upstream does.
#[test]
fn the_seeded_personality_matches_the_hand_computed_upstream_formula() {
    // TRAINER_MAY_ROUTE_103_MUDKIP: trainerName "MAY", a female trainer
    // (F_TRAINER_FEMALE), fielding SPECIES_TREECKO.
    let id = route103_rival_for(Rival::May, PlayerStarter::Mudkip);
    let hash: u32 = "MAYTREECKO"
        .chars()
        .map(|c| u32::from(engine::text::char_to_byte(c).expect("A-Z are charmap glyphs")))
        .sum();
    let expected = 0x78u32.wrapping_add(hash << 8);
    assert_eq!(trainer_party_personalities(id).unwrap(), vec![expected]);

    // The male-rival base differs, and so does the name: BRENDAN's Torchic
    // (fought by a player who chose Treecko).
    let id = route103_rival_for(Rival::Brendan, PlayerStarter::Treecko);
    let hash: u32 = "BRENDANTORCHIC"
        .chars()
        .map(|c| u32::from(engine::text::char_to_byte(c).expect("A-Z are charmap glyphs")))
        .sum();
    assert_eq!(
        trainer_party_personalities(id).unwrap(),
        vec![0x88u32.wrapping_add(hash << 8)]
    );
}

/// Charmap bytes, not ASCII: the two disagree, so a naive `as u32` on the
/// Rust `char` would seed a different personality and therefore a different
/// nature.
#[test]
fn the_name_hash_uses_charmap_bytes_and_not_ascii() {
    let id = route103_rival_for(Rival::May, PlayerStarter::Mudkip);
    let ascii: u32 = "MAYTREECKO".chars().map(u32::from).sum();
    let charmap: u32 = "MAYTREECKO"
        .chars()
        .map(|c| u32::from(engine::text::char_to_byte(c).unwrap()))
        .sum();
    assert_ne!(ascii, charmap, "the two encodings must actually differ");
    assert_eq!(
        trainer_party_personalities(id).unwrap(),
        vec![0x78u32.wrapping_add(charmap << 8)]
    );
    assert_ne!(
        trainer_party_personalities(id).unwrap(),
        vec![0x78u32.wrapping_add(ascii << 8)]
    );
}

/// `nameHash` is declared outside `CreateNPCTrainerParty`'s per-mon loop and
/// never reset, so a multi-mon party's later members carry the trainer name
/// repeatedly *and* every earlier species name. No Route 103 rival has a
/// second mon, so this is pinned against a real multi-mon trainer instead.
#[test]
fn the_name_hash_accumulates_across_a_multi_mon_party() {
    // TRAINER_GRUNT_AQUA_HIDEOUT_2 (`sParty_GruntAquaHideout2`,
    // trainer_parties.h): two mons, no held items -- POOCHYENA then
    // CARVANHA.
    let table = TrainerTable::new();
    let id = (1..assets::trainers::TRAINERS_COUNT)
        .map(|i| TrainerId(u16::try_from(i).unwrap()))
        .find(|id| {
            let data = table.trainer(*id).unwrap();
            matches!(data.party, TrainerParty::NoItemDefaultMoves(p) if p.len() == 2)
        })
        .expect("some trainer fields a two-mon default-moveset party");
    let data = table.trainer(id).unwrap();
    let TrainerParty::NoItemDefaultMoves(party) = data.party else {
        unreachable!()
    };
    let names = assets::SpeciesNames::new();
    let byte_sum = |s: &str| -> u32 {
        s.chars()
            .map(|c| u32::from(engine::text::char_to_byte(c).unwrap()))
            .sum()
    };
    let base = if data.encounter_music.is_female {
        0x78u32
    } else {
        0x88u32
    };
    let first_hash = byte_sum(data.name) + byte_sum(names.name(party[0].species).unwrap());
    // The second mon adds the trainer name *again* on top of the running
    // total, then its own species name.
    let second_hash =
        first_hash + byte_sum(data.name) + byte_sum(names.name(party[1].species).unwrap());
    assert_eq!(
        trainer_party_personalities(id).unwrap(),
        vec![
            base.wrapping_add(first_hash.wrapping_shl(8)),
            base.wrapping_add(second_hash.wrapping_shl(8)),
        ],
        "the hash must accumulate, not reset per mon"
    );
}

/// Every extracted trainer name and species name encodes, so
/// [`NpcTrainerBattleError::UnnamedCharacter`] is genuinely unreachable for
/// real data rather than merely untested.
#[test]
fn every_trainer_and_species_name_has_a_charmap_encoding() {
    for data in TrainerTable::new().iter() {
        for character in data.name.chars() {
            assert!(
                engine::text::char_to_byte(character).is_some(),
                "trainer name `{}` has an unencodable `{character}`",
                data.name
            );
        }
    }
    let names = assets::SpeciesNames::new();
    for index in 0..names.len() {
        let name = names
            .name(SpeciesId(u16::try_from(index).unwrap()))
            .unwrap();
        for character in name.chars() {
            assert!(
                engine::text::char_to_byte(character).is_some(),
                "species name `{name}` has an unencodable `{character}`"
            );
        }
    }
}

/// A held-item party fails closed rather than silently dropping the item.
#[test]
fn a_held_item_party_is_refused_rather_than_stripped() {
    let table = TrainerTable::new();
    let id = (1..assets::trainers::TRAINERS_COUNT)
        .map(|i| TrainerId(u16::try_from(i).unwrap()))
        .find(|id| {
            matches!(
                table.trainer(*id).unwrap().party,
                TrainerParty::ItemDefaultMoves(_) | TrainerParty::ItemCustomMoves(_)
            )
        })
        .expect("some trainer fields held items");
    assert_eq!(
        trainer_party_personalities(id).unwrap_err(),
        NpcTrainerBattleError::HeldItemParty(id)
    );
}

/// The construction end to end: the real rival, its real level-up moveset,
/// its seeded personality and all-zero IVs, and proof that
/// `BATTLE_TYPE_TRAINER` really reached `Battle::new_trainer`.
#[test]
fn starting_the_rival_battle_builds_the_seeded_level_5_treecko() {
    let id = route103_rival_for(Rival::May, PlayerStarter::Mudkip);
    let mut rng = Rng::new(1);
    let lead = player_mon(MUDKIP, 5, vec![TACKLE]);
    let battle = start_npc_trainer_battle(lead, id, &mut rng).expect("construction must succeed");

    assert_eq!(battle.enemy().species(), SpeciesId(TREECKO));
    assert_eq!(battle.enemy().level(), 5);
    assert_eq!(
        battle.enemy().ivs().as_array(),
        [0; 6],
        "`.iv = 0` scales to all-zero IVs"
    );
    assert_eq!(
        battle.enemy().personality(),
        trainer_party_personalities(id).unwrap()[0]
    );
    let known: Vec<MoveId> = battle.enemy().moves().iter().map(|m| m.move_id).collect();
    assert_eq!(
        known,
        vec![POUND, LEER],
        "a level-5 Treecko's real level-up learnset"
    );
    let context = battle.trainer().expect("BATTLE_TYPE_TRAINER must be set");
    assert_eq!(context.id(), id);
    assert_eq!(context.money(), 300);
}

/// The exact draws `start_npc_trainer_battle` spends for a Route 103 rival:
/// one `Random32` OT id for the party's single mon, then
/// `Battle::new_trainer`'s turn-number draw. The personality and IVs draw
/// nothing at all — which is the whole difference from
/// `flow::first_battle`'s construction.
#[test]
fn construction_draws_only_the_ot_id_then_the_turn_number() {
    const SEED: u32 = 7;
    let id = route103_rival_for(Rival::May, PlayerStarter::Mudkip);
    let mut rng = Rng::new(SEED);
    let lead = player_mon(MUDKIP, 5, vec![TACKLE]);
    let battle = start_npc_trainer_battle(lead, id, &mut rng).expect("construction must succeed");

    let mut reference = Rng::new(SEED);
    let ot_id = reference.next_u32();
    assert!(
        battle::shiny_value(ot_id, battle.enemy().personality()) >= battle::SHINY_ODDS,
        "this seed's first OT id must not be shiny, or the loop would redraw"
    );
    assert_eq!(battle.enemy().original_trainer_id(), ot_id);
    // `Battle::new_trainer`'s own gRandomTurnNumber draw.
    reference.next_u16();

    assert_ne!(
        battle.player().effective_speed(),
        battle.enemy().effective_speed(),
        "this pin assumes no speed tie; a tie would cost one extra draw"
    );
    assert_eq!(
        rng.state(),
        reference.state(),
        "exactly OT id (2) + turn number (1) = 3 draws, no more"
    );
}

/// All six rivals construct and play against the starter they are paired
/// with — the sweep that proves the `battle` crate's two construction
/// screens really do pass for every configuration a real playthrough can
/// reach.
#[test]
fn all_six_rivals_construct_and_play_to_a_terminal_outcome() {
    for (rival, starter, species) in [
        (Rival::Brendan, PlayerStarter::Mudkip, MUDKIP),
        (Rival::Brendan, PlayerStarter::Treecko, TREECKO),
        (Rival::Brendan, PlayerStarter::Torchic, TORCHIC),
        (Rival::May, PlayerStarter::Mudkip, MUDKIP),
        (Rival::May, PlayerStarter::Treecko, TREECKO),
        (Rival::May, PlayerStarter::Torchic, TORCHIC),
    ] {
        let id = route103_rival_for(rival, starter);
        let mut rng = Rng::new(11);
        // A level-50 lead so the battle terminates quickly and
        // deterministically in a win; the point here is that every
        // configuration *constructs* and reaches a terminal outcome, not
        // who wins.
        let lead = player_mon(species, 50, vec![SLASH]);
        let mut slot = Some(
            start_npc_trainer_battle(lead, id, &mut rng)
                .unwrap_or_else(|e| panic!("{id:?} must construct: {e}")),
        );
        let mut written_back = None;
        let mut money = 0;
        let mut outcome = None;
        for _ in 0..32 {
            outcome =
                advance_npc_trainer_battle(&mut slot, &mut written_back, &mut money, &mut rng);
            if outcome.is_some() {
                break;
            }
        }
        assert_eq!(
            outcome,
            Some(BattleOutcome::PlayerWon),
            "{id:?} must reach a terminal outcome"
        );
        assert!(slot.is_none(), "a finished battle empties its slot");
        assert!(
            written_back.is_some(),
            "the driver writes the player's mon back"
        );
        assert_eq!(
            money,
            trainer_money(trainer_data(id).unwrap()),
            "{id:?}'s win must credit exactly its own AddMoney reward"
        );
    }
}

/// The driver's headline requirement, mirroring
/// `first_battle/tests.rs`'s: it must never choose
/// [`battle::PlayerAction::Run`], which a trainer battle refuses outright.
#[test]
fn the_driver_never_attempts_to_run() {
    let id = route103_rival_for(Rival::May, PlayerStarter::Mudkip);
    let mut rng = Rng::new(3);
    // A Magikarp that knows only Splash would be rejected outright; use a
    // real lead and simply watch that no turn ever fails with the trainer
    // run refusal, and that the battle really progresses.
    let lead = player_mon(MUDKIP, 5, vec![TACKLE]);
    let mut slot = Some(start_npc_trainer_battle(lead, id, &mut rng).unwrap());
    let enemy_hp_before = slot.as_ref().unwrap().enemy().current_hp();

    let mut written_back = None;
    let mut money = 0;
    let outcome = advance_npc_trainer_battle(&mut slot, &mut written_back, &mut money, &mut rng);
    assert_eq!(outcome, None, "one turn must not end this battle");
    assert!(slot.is_some(), "an ongoing battle keeps its slot");
    let battle = slot.as_ref().unwrap();
    assert!(
        battle.enemy().current_hp() < enemy_hp_before
            || battle.player().current_hp() < battle.player().stats().max_hp,
        "the turn really played: someone took damage"
    );
    assert_eq!(
        battle.turn_counter(),
        0,
        "gBattleResults.battleTurnCounter is 0 for the whole of turn 1"
    );
}

/// A driver turn that cannot be taken ends the battle rather than looping
/// forever: the same contract `advance_first_battle` documents, checked here
/// through the one reachable route (a lead whose slot 0 is out of PP).
#[test]
fn a_lead_with_no_pp_in_slot_zero_ends_the_battle_and_is_still_written_back() {
    let id = route103_rival_for(Rival::May, PlayerStarter::Mudkip);
    let mut rng = Rng::new(5);
    let mut lead = player_mon(MUDKIP, 5, vec![TACKLE]);
    for _ in 0..lead.moves()[0].pp {
        lead.deduct_pp(0).unwrap();
    }
    let mut slot = Some(start_npc_trainer_battle(lead, id, &mut rng).unwrap());
    let mut written_back = None;
    let mut money = 0;

    let outcome = advance_npc_trainer_battle(&mut slot, &mut written_back, &mut money, &mut rng);
    assert_eq!(outcome, None, "the engine reported no outcome...");
    assert!(slot.is_none(), "...but the battle was ended anyway");
    let mon = written_back.expect("the mon is still written back");
    assert_eq!(mon.moves()[0].pp, 0, "drained PP and all");
}

/// The species -> [`PlayerStarter`] mapping (issue #248, narrowed to the
/// var-write side by issue #251 -- `PlayerStarter::from_species`'s own doc
/// comment): the real three starters round-trip, and an arbitrary species
/// has no mapping.
#[test]
fn player_starter_from_species_covers_exactly_the_three_starters() {
    assert_eq!(
        PlayerStarter::from_species(SpeciesId(TREECKO)),
        Some(PlayerStarter::Treecko)
    );
    assert_eq!(
        PlayerStarter::from_species(SpeciesId(TORCHIC)),
        Some(PlayerStarter::Torchic)
    );
    assert_eq!(
        PlayerStarter::from_species(SpeciesId(MUDKIP)),
        Some(PlayerStarter::Mudkip)
    );
    // SPECIES_ZIGZAGOON -- not a starter.
    assert_eq!(PlayerStarter::from_species(SpeciesId(288)), None);
}

/// The real `VAR_STARTER_MON` encoding (issue #251):
/// `include/constants/vars.h:53`'s own comment (`0`=Treecko, `1`=Torchic,
/// `2`=Mudkip), round-tripped both ways, plus an out-of-range value's `None`.
#[test]
fn player_starter_var_value_and_from_var_round_trip_the_real_encoding() {
    for (value, starter) in [
        (0, PlayerStarter::Treecko),
        (1, PlayerStarter::Torchic),
        (2, PlayerStarter::Mudkip),
    ] {
        assert_eq!(PlayerStarter::from_var(value), Some(starter));
        assert_eq!(starter.var_value(), value);
    }
    assert_eq!(
        PlayerStarter::from_var(3),
        None,
        "VAR_STARTER_MON only ever names one of the three real starters"
    );
}

/// [`Rival::for_gender`]'s opposite-gender pairing, and the `Other` no-op
/// upstream's own `checkplayergender` branch documents (issue #248).
#[test]
fn rival_for_gender_is_always_the_opposite_protagonist() {
    use engine::save::PlayerGender;
    assert_eq!(Rival::for_gender(PlayerGender::Male), Some(Rival::May));
    assert_eq!(
        Rival::for_gender(PlayerGender::Female),
        Some(Rival::Brendan)
    );
    assert_eq!(Rival::for_gender(PlayerGender::Other(7)), None);
}

/// An empty slot is a no-op, and does not touch the lead.
#[test]
fn advancing_an_empty_slot_does_nothing() {
    let mut rng = Rng::new(1);
    let mut slot = None;
    let mut lead = None;
    let mut money = crate::new_game::STARTING_MONEY;
    assert_eq!(
        advance_npc_trainer_battle(&mut slot, &mut lead, &mut money, &mut rng),
        None
    );
    assert!(lead.is_none());
    assert_eq!(
        money,
        crate::new_game::STARTING_MONEY,
        "an empty slot must not touch the wallet"
    );
}
