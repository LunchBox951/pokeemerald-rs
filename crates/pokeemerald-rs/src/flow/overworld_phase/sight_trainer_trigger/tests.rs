use super::*;
use crate::flow::overworld_phase::test_support::admitted_objects;

/// [`SIGHT_TRAINERS`] must cover exactly Route 103's own nine
/// `TRAINER_TYPE_NORMAL` object events, by script name -- pinned against
/// the real extracted map data (that table's own citation).
#[test]
fn sight_trainers_table_covers_every_route_103_normal_trainer_by_script_name() {
    let events = assets::MapEventsTable::new()
        .resolve(assets::MapId("MAP_ROUTE103"))
        .expect("MAP_ROUTE103 is bundled map data, not pack-extracted");
    let normal_trainer_scripts: Vec<&str> = events
        .object_events
        .iter()
        .filter(|e| e.trainer_type == assets::TrainerType::Normal)
        .filter_map(|e| e.script.map(assets::ScriptRef::id))
        .collect();
    assert_eq!(
        normal_trainer_scripts.len(),
        9,
        "Route 103 must carry exactly nine TRAINER_TYPE_NORMAL object events"
    );
    for script in normal_trainer_scripts {
        assert!(
            sight_trainer_for_script(script).is_some(),
            "SIGHT_TRAINERS is missing a mapping for {script}"
        );
    }
}

/// Every id in [`SIGHT_TRAINERS`] resolves in the extracted `gTrainers`
/// table, and the double/single split is exactly the one
/// [`trainer_data_wants_double_battle`] describes: Amy & Liv's shared id
/// is a double battle, the other seven (Daisy, Andrew, Rhett, Marcos,
/// Isabelle, Pete, and Miguel) are not.
#[test]
fn every_sight_trainer_id_resolves_with_the_expected_double_battle_flag() {
    for entry in SIGHT_TRAINERS {
        let data = battle::trainer_data(entry.id)
            .unwrap_or_else(|e| panic!("{} -> {:?} must resolve: {e}", entry.script, entry.id));
        let expected_double = entry.script == "Route103_EventScript_Amy"
            || entry.script == "Route103_EventScript_Liv";
        assert_eq!(
            data.double_battle, expected_double,
            "{} -> {:?} double_battle mismatch",
            entry.script, entry.id
        );
    }
}

/// Every transcribed intro speech is real, printable text: non-empty,
/// Gen-3 encodable (`POKéMON`'s `é` and Pete's `…` included), and
/// carrying *no* trailing `{P}`, exactly like the
/// upstream strings it transcribes ([`SightTrainer::intro`]'s own docs).
///
/// The button wait is the script's, not the text's: `waitbuttonpress`
/// (`data/scripts/trainer_battle.inc:104`) is
/// [`crate::overworld::dialog::NpcDialog::with_waitbuttonpress`] here. A
/// trailing `{P}` on top of it would be a *second*, earlier wait that
/// clears the box first, stranding the player on a blank box until a
/// further fresh confirm edge landed -- see
/// [`crate::overworld::dialog`]'s own "Script-level `waitbuttonpress`"
/// module docs.
#[test]
fn every_intro_speech_is_encodable_and_leaves_the_button_wait_to_the_script() {
    for entry in SIGHT_TRAINERS {
        let tokens = crate::authored_message::parse_message(entry.intro)
            .unwrap_or_else(|err| panic!("{}'s intro speech is malformed: {err}", entry.script));
        engine::text::encode(&tokens).unwrap_or_else(|err| {
            panic!(
                "{}'s intro speech is not Gen-3 encodable: {err}",
                entry.script
            )
        });
        assert!(
            tokens
                .iter()
                .filter(|t| matches!(t, engine::text::Token::Char(_)))
                .count()
                > 0,
            "{}'s intro speech must contain visible text",
            entry.script
        );
        assert_ne!(
            tokens[tokens.len() - 2],
            engine::text::Token::PromptClear,
            "{}'s intro speech must not end in a synthetic `{{P}}` -- the wait before \
             `dotrainerbattle` is the script's `waitbuttonpress`, not a text control code",
            entry.script
        );
        assert_eq!(tokens.last(), Some(&engine::text::Token::End));
    }
}

/// `TRAINER_MIGUEL_1`'s real party is a single Oran Berry holder, so the
/// refusal rows above describe a party that really carries it.
#[test]
fn miguel_carries_an_oran_berry_party() {
    let data = battle::trainer_data(TrainerId(293)).expect("TRAINER_MIGUEL_1 must resolve");
    let assets::trainers::TrainerParty::ItemDefaultMoves(party) = data.party else {
        panic!("Miguel's party is the held-item default-moves shape");
    };
    assert_eq!(party.len(), 1);
    assert_eq!(party[0].species, assets::SpeciesId::SKITTY);
    assert_eq!(party[0].held_item, assets::items::ItemId::ORAN_BERRY);
}

/// [`find_sight_trainer`] against the real Route 103 object events:
/// Rhett's own cone (`(67, 5)`, facing south, range 2) reaches a player
/// standing one tile south of him, and an already-defeated flag refuses
/// the same otherwise-qualifying geometry -- [`already_defeated`]'s gate,
/// pinned at the scan level rather than only through the full
/// `OverworldPhase` (`overworld_phase::sight_trainer_tests` covers the
/// end-to-end wiring; this is the narrower, `MapRuntime`-only slice
/// `find_sight_trainer` itself owns).
#[test]
fn find_sight_trainer_matches_rhett_and_is_gated_by_the_defeated_flag() {
    let map_id = assets::MapId("MAP_ROUTE103");
    let scene = crate::overworld::test_support::synthetic_scene(80, 16).in_room_for_test(map_id);
    let runtime = scene.runtime();
    let player = PlayerState::new((67, 6), 3, engine::overworld::Direction::North);
    let objects = admitted_objects(scene.map_events().object_events, &player, &EventData::new());

    let event_data = EventData::new();
    let scan = find_sight_trainer(&objects, &runtime, &player, &event_data);
    let (_, entry) = scan
        .selected
        .expect("Rhett's own south-facing cone must reach a player one tile south of him");
    assert_eq!(entry.id, TrainerId(703), "must resolve to TRAINER_RHETT");
    assert!(scan.any_cone_reached);

    let mut defeated = EventData::new();
    defeated
        .flag_set(TRAINER_FLAGS_START + 703)
        .expect("TRAINER_FLAGS_START + 703 is an ordinary ranged flag id");
    let scan = find_sight_trainer(&objects, &runtime, &player, &defeated);
    assert!(
        scan.selected.is_none(),
        "an already-defeated trainer must not be selected even though the geometry \
         still qualifies"
    );
    assert!(
        !scan.any_cone_reached,
        "a defeated trainer is skipped before the geometry runs, so it cannot count as \
         the player standing in a cone either"
    );
}

/// A double-battle refusal is *reported*, not logged, and still counts
/// as the player standing inside a cone -- the distinction
/// [`SightTrainerLog`] resets on. Amy's own cone (`(64, 12)`, facing
/// south, range 1) reaches a player one tile south.
#[test]
fn a_double_battle_cone_is_reported_as_reached_but_never_selected() {
    let map_id = assets::MapId("MAP_ROUTE103");
    let scene = crate::overworld::test_support::synthetic_scene(80, 16).in_room_for_test(map_id);
    let runtime = scene.runtime();
    let player = PlayerState::new((64, 13), 3, engine::overworld::Direction::North);
    let objects = admitted_objects(scene.map_events().object_events, &player, &EventData::new());

    let scan = find_sight_trainer(&objects, &runtime, &player, &EventData::new());
    assert_eq!(
        scan.doubles_refused,
        vec![TrainerId(481)],
        "TRAINER_AMY_AND_LIV_1's cone reached and was refused"
    );
    assert!(scan.selected.is_none(), "no other cone reaches that tile");
    assert!(
        scan.any_cone_reached,
        "a refused-but-reaching cone still means the player is standing in one"
    );
}

/// `InitTrainerApproachTask`'s own `approachDistance - 1`
/// ([`approach_walk_tiles`]): an adjacent trainer walks nothing, and
/// each further tile of cone adds one step -- on either axis, in either
/// direction.
#[test]
fn the_walk_length_is_one_tile_short_of_the_approach_distance() {
    assert_eq!(approach_walk_tiles((5, 5), (5, 6)), 0, "already adjacent");
    assert_eq!(approach_walk_tiles((5, 5), (5, 8)), 2);
    assert_eq!(approach_walk_tiles((5, 5), (5, 1)), 3);
    assert_eq!(approach_walk_tiles((5, 5), (9, 5)), 3);
    assert_eq!(approach_walk_tiles((5, 5), (1, 5)), 3);
}

/// The log gate itself: once per trainer per cone entry, and a fresh
/// entry after the player has left every cone ([`SightTrainerLog`]).
#[test]
fn the_log_gate_reports_each_trainer_once_per_cone_entry() {
    let mut log = SightTrainerLog::default();
    assert!(log.should_log(TrainerId(703)));
    assert!(!log.should_log(TrainerId(703)), "still in the same cone");
    assert!(
        log.should_log(TrainerId(481)),
        "a second, overlapping cone is its own event -- a single-slot memo would \
         alternate between the two and log both forever"
    );
    assert!(!log.should_log(TrainerId(481)));

    log.left_every_cone();
    assert!(
        log.should_log(TrainerId(703)),
        "a fresh entry reports again"
    );
}

/// Only [`SightTrainerOutcome::Refused`] leaves the frame to the rest of
/// [`super::step::OverworldPhase::step`]; the three preempting outcomes
/// all consume it.
#[test]
fn only_a_refusal_leaves_the_frame_alone() {
    assert!(!SightTrainerOutcome::Refused.owns_frame());
    assert!(SightTrainerOutcome::ApproachStarted.owns_frame());
    assert!(SightTrainerOutcome::ApproachAdvanced.owns_frame());
    assert!(SightTrainerOutcome::BattleStarted.owns_frame());
}

/// The construction wall that remains, pinned rather than left as prose:
/// every one of [`SIGHT_TRAINERS`]' distinct trainer ids except the six
/// that fight (Daisy, Andrew, Rhett, Marcos, Pete, Isabelle: `real_party_outcome` below)
/// fails to construct through
/// [`npc_trainer_battle::start_npc_trainer_battle`], each for the first
/// move this battle engine does not yet implement or score, or for the
/// effect of a held item it does not model
/// ([`OverworldPhase::begin_sight_trainer_approach_if_seen`]'s "Refusals
/// cost nothing, forever").
///
/// The **exact** refusal is pinned per trainer, not merely "it fails": the reason a trainer is unreachable is the
/// interesting fact, and naming the offending move id is what forces a
/// future move-coverage slice to come back here -- scoring a
/// trainer's refusing move leaves one who fails on the next one, and a
/// bare `is_err()` would hide that.
///
/// Each refusal must also cost **nothing**: the whole party is screened
/// before the first draw (`npc_trainer_battle`'s module docs), which is
/// what makes the per-frame sight check above safe to run forever.
#[test]
fn refused_sight_trainers_real_parties_fail_to_construct_for_exactly_these_reasons() {
    use battle::BattleError;
    use npc_trainer_battle::NpcTrainerBattleError::Battle;

    // Every distinct id in `SIGHT_TRAINERS` that does not fight, with the
    // refusal its real extracted party currently produces. The move ids
    // are the first move in that trainer's own level-up-derived moveset
    // that some screen refuses. Miguel's Skitty no longer stops at its
    // selected Cute Charm, which the battle crate now models; it stops at Sing
    // instead, ahead of the later screens.
    let expected = [
        (
            "Amy & Liv",
            TrainerId(481),
            Battle(BattleError::NonDamagingMove(assets::MoveId::HELPING_HAND)),
        ),
        (
            "Miguel",
            TrainerId(293),
            Battle(BattleError::NonDamagingMove(assets::MoveId::SING)),
        ),
    ];

    let distinct: std::collections::HashSet<u16> =
        SIGHT_TRAINERS.iter().map(|entry| entry.id.0).collect();
    let covered: std::collections::HashSet<u16> = expected
        .iter()
        .map(|(_, id, _)| id.0)
        .chain(FIGHTING_TRAINERS.map(|id| id.0))
        .collect();
    assert_eq!(
        distinct, covered,
        "this table must cover exactly SIGHT_TRAINERS' distinct ids"
    );

    for (name, id, refusal) in expected {
        let lead = battle::BattlePokemon::new(
            &battle::Dex::new(),
            assets::SpeciesId(277), // SPECIES_TREECKO
            50,
            battle::fixed_ivs(31),
            0,
            vec![assets::MoveId::SLASH],
        )
        .expect("Treecko/Slash is a valid pairing");
        let mut rng = engine::rng::Rng::new(1);
        let before = rng.state();
        let result = npc_trainer_battle::start_npc_trainer_battle(lead, id, &mut rng);
        assert_eq!(
            result.err(),
            Some(refusal),
            "{name} -> {id:?} was expected to still fail to construct for exactly this \
             reason -- if move coverage has grown, update this row (or, if it now \
             succeeds, add a real construction-backed win/loss test)"
        );
        assert_eq!(
            rng.state(),
            before,
            "{name} -> {id:?}: a refused construction must draw nothing at all"
        );
    }
}

const DAISY: TrainerId = TrainerId(36);
const ANDREW: TrainerId = TrainerId(336);
const RHETT: TrainerId = TrainerId(703);
const MARCOS: TrainerId = TrainerId(702);
const PETE: TrainerId = TrainerId(735);
const ISABELLE: TrainerId = TrainerId(736);
/// The six sight trainers whose real parties construct and fight.
const FIGHTING_TRAINERS: [TrainerId; 6] = [DAISY, ANDREW, RHETT, MARCOS, PETE, ISABELLE];

/// Plays `trainer`'s real party against a strong lead and returns the
/// outcome.
fn real_party_outcome(trainer: TrainerId) -> Option<battle::BattleOutcome> {
    let lead = battle::BattlePokemon::new(
        &battle::Dex::new(),
        assets::SpeciesId(277), // SPECIES_TREECKO
        50,
        battle::fixed_ivs(31),
        0,
        vec![assets::MoveId::SLASH],
    )
    .expect("Treecko/Slash is a valid pairing");
    let mut rng = engine::rng::Rng::new(1);
    let mut slot = Some(
        npc_trainer_battle::start_npc_trainer_battle(lead, trainer, &mut rng)
            .expect("the party must construct"),
    );
    let mut written_back = None;
    let mut money = 0;
    let mut outcome = None;
    for _turn in 0..64 {
        outcome = npc_trainer_battle::advance_npc_trainer_battle(
            &mut slot,
            &mut written_back,
            &mut money,
            &mut rng,
        );
        if outcome.is_some() {
            break;
        }
    }
    outcome
}

/// Daisy (`CheckBadMove` only) has Leech Seed on her Shroomish, which the
/// seeded-drain pipeline executes and `AI_CBM_LeechSeed` scores, so her
/// party constructs and plays out.
#[test]
fn daisys_real_party_constructs_and_plays_to_a_win() {
    assert_eq!(
        real_party_outcome(DAISY),
        Some(battle::BattleOutcome::PlayerWon)
    );
}

/// Isabelle (`CheckBadMove` only) has Rollout on her Marill, which the
/// Rollout pipeline executes and the trainer AI scores like an ordinary
/// hit, so her party constructs and plays out.
#[test]
fn isabelles_real_party_constructs_and_plays_to_a_win() {
    assert_eq!(
        real_party_outcome(ISABELLE),
        Some(battle::BattleOutcome::PlayerWon)
    );
}

/// Andrew (`CheckBadMove` only) has Supersonic on his Tentacool, which
/// the Confuse handler now scores, so his party constructs and plays out.
#[test]
fn andrews_real_party_constructs_and_plays_to_a_win() {
    assert_eq!(
        real_party_outcome(ANDREW),
        Some(battle::BattleOutcome::PlayerWon)
    );
}

/// Rhett's Sand-Attack is scored by the Accuracy-down label, so his whole
/// party constructs and plays out.
#[test]
fn rhetts_real_party_constructs_and_plays_to_a_win() {
    assert_eq!(
        real_party_outcome(RHETT),
        Some(battle::BattleOutcome::PlayerWon)
    );
}

/// Marcos's Sonic Boom and Pete's Constrict land when the player's lead
/// cannot end the fight quickly: a level-100 lead that only Splashes lets
/// the real party spend its whole moveset.
#[test]
fn marcoss_sonic_boom_and_petes_constrict_land_in_a_long_fight() {
    let cases = [
        (MARCOS, assets::MoveId::SONIC_BOOM),
        (PETE, assets::MoveId::CONSTRICT),
    ];
    for (trainer, move_id) in cases {
        let lead = battle::BattlePokemon::new(
            &battle::Dex::new(),
            assets::SpeciesId(280), // SPECIES_TORCHIC
            100,
            battle::fixed_ivs(31),
            0,
            vec![assets::MoveId::SPLASH],
        )
        .expect("Torchic/Splash is a valid pairing");
        let mut rng = engine::rng::Rng::new(1);
        let mut battle = npc_trainer_battle::start_npc_trainer_battle(lead, trainer, &mut rng)
            .expect("the party must construct");
        let mut used = false;
        while battle.outcome().is_none() {
            let events =
                crate::flow::first_usable_move::take_first_usable_move_turn(&mut battle, &mut rng)
                    .expect("every turn executes");
            used |= events.iter().any(|event| {
                matches!(
                    event,
                    battle::BattleEvent::Hit { by_player: false, move_id: used, .. }
                        if *used == move_id
                )
            });
        }
        assert!(used, "{trainer:?} never landed {move_id:?}");
    }
}

/// Marcos's Voltorb runs Sonic Boom through `AI_CBM_HighRiskForDamage`,
/// so his party constructs and plays out.
#[test]
fn marcoss_real_party_constructs_and_plays_to_a_win() {
    assert_eq!(
        real_party_outcome(MARCOS),
        Some(battle::BattleOutcome::PlayerWon)
    );
}

/// Pete's Tentacool carries Constrict, whose Speed-drop secondary the hit
/// pipeline now applies, so his party constructs and plays out.
#[test]
fn petes_real_party_constructs_and_plays_to_a_win() {
    assert_eq!(
        real_party_outcome(PETE),
        Some(battle::BattleOutcome::PlayerWon)
    );
}
