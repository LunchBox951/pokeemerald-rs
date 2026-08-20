use super::{
    build_pokemon_with_random_personality, build_wild_pokemon, ensure_wild_startable,
    initial_moveset, roll_ivs, roll_nature, roll_personality_for_nature,
};
use crate::damage::BattleRng;
use crate::dex::Dex;
use crate::error::BattleError;
use crate::nature::Nature;
use crate::pokemon::{MAX_MON_MOVES, MOVE_NONE};
use assets::{MoveId, SpeciesId};

/// A `BattleRng` fed from a fixed sequence, panicking (loudly, not
/// hanging) if exhausted — for pinning exact draw order/count without
/// risking an infinite loop in a broken test.
struct SequenceRng {
    values: Vec<u16>,
    index: usize,
}
impl SequenceRng {
    fn new(values: impl IntoIterator<Item = u16>) -> Self {
        Self {
            values: values.into_iter().collect(),
            index: 0,
        }
    }
    fn draws(&self) -> usize {
        self.index
    }
}
impl BattleRng for SequenceRng {
    fn next_u16(&mut self) -> u16 {
        let v = self
            .values
            .get(self.index)
            .copied()
            .expect("SequenceRng exhausted");
        self.index += 1;
        v
    }
}

#[test]
fn roll_nature_is_modulo_twenty_five_of_one_draw() {
    let mut rng = SequenceRng::new([0]);
    assert_eq!(roll_nature(&mut rng), Nature::Hardy); // 0 % 25 == 0
    assert_eq!(rng.draws(), 1);

    let mut rng = SequenceRng::new([24]);
    assert_eq!(roll_nature(&mut rng), Nature::Quirky); // 24 % 25 == 24
    assert_eq!(rng.draws(), 1);

    let mut rng = SequenceRng::new([25]); // wraps back to Hardy
    assert_eq!(roll_nature(&mut rng), Nature::Hardy);
}

#[test]
fn roll_personality_for_nature_stops_at_the_first_match() {
    // Personality 0 has nature Hardy (0 % 25 == 0): a target of Hardy
    // matches on the very first Random32() draw.
    let mut rng = SequenceRng::new([0, 0]); // next_u32() composes 2 draws
    let personality = roll_personality_for_nature(Nature::Hardy, &mut rng);
    assert_eq!(personality, 0);
    assert_eq!(rng.draws(), 2);
}

#[test]
fn roll_personality_for_nature_rejects_until_a_match() {
    // First candidate personality = 1 (nature Lonely, id 1) does not
    // match a target of Bold (id 5); second candidate = 5 does.
    // next_u32() draws low-then-high per call: (1, 0) -> 0x0000_0001,
    // (5, 0) -> 0x0000_0005.
    let mut rng = SequenceRng::new([1, 0, 5, 0]);
    let personality = roll_personality_for_nature(Nature::Bold, &mut rng);
    assert_eq!(personality, 5);
    assert_eq!(rng.draws(), 4); // two full Random32() draws, rejected once
}

#[test]
fn roll_ivs_splits_two_draws_into_five_bit_fields() {
    // First draw 0b00_10101_01010_11111 = HP 0b11111=31, Atk
    // 0b01010=10, Def 0b10101=21 (top bit of the 16-bit value unused).
    let first = 0b0_10101_01010_11111u16;
    // Second draw similarly for Speed/SpAtk/SpDef.
    let second = 0b0_00001_00010_00011u16;
    let mut rng = SequenceRng::new([first, second]);
    let ivs = roll_ivs(&mut rng);
    assert_eq!(ivs.hp, 31);
    assert_eq!(ivs.attack, 10);
    assert_eq!(ivs.defense, 21);
    assert_eq!(ivs.speed, 3);
    assert_eq!(ivs.sp_attack, 2);
    assert_eq!(ivs.sp_defense, 1);
    assert_eq!(rng.draws(), 2);
}

#[test]
fn roll_ivs_masks_to_five_bits_even_with_high_bits_set() {
    let mut rng = SequenceRng::new([0xFFFF, 0xFFFF]);
    let ivs = roll_ivs(&mut rng);
    assert_eq!(ivs.hp, 31);
    assert_eq!(ivs.attack, 31);
    assert_eq!(ivs.defense, 31);
    assert_eq!(ivs.speed, 31);
    assert_eq!(ivs.sp_attack, 31);
    assert_eq!(ivs.sp_defense, 31);
}

#[test]
fn build_wild_pokemon_draws_nature_then_personality_then_ivs_in_order() {
    let dex = Dex::new();
    // Distinct, position-revealing values -- an all-zeros script cannot
    // pin the order, because every permutation of the three roll phases
    // would build the same mon from it. Each phase below only reads its
    // own values correctly in the documented order:
    // - nature draw: 24 % 25 -> Quirky;
    // - personality draws (low then high): (24, 0) -> 24, whose 24 % 25
    //   matches Quirky on the first try -- a reordered phase feeds the
    //   wrong values into the rejection loop and exhausts the script;
    // - IV draws: 0x001F -> hp 31 / attack 0 / defense 0, then
    //   0x03E0 -> speed 0 / sp_attack 31 / sp_defense 0.
    let mut rng = SequenceRng::new([24, 24, 0, 0x001F, 0x03E0]);
    let mon = build_wild_pokemon(
        &dex,
        SpeciesId(1), // Bulbasaur
        5,
        vec![MoveId(33)], // Tackle
        &mut rng,
    )
    .unwrap();
    assert_eq!(mon.nature(), Nature::Quirky);
    assert_eq!(mon.personality(), 24);
    assert_eq!(mon.ivs().hp, 31);
    assert_eq!(mon.ivs().attack, 0);
    assert_eq!(mon.ivs().defense, 0);
    assert_eq!(mon.ivs().speed, 0);
    assert_eq!(mon.ivs().sp_attack, 31);
    assert_eq!(mon.ivs().sp_defense, 0);
    assert_eq!(rng.draws(), 5);
    assert_eq!(mon.level(), 5);
    assert_eq!(mon.species(), SpeciesId(1));
    assert!(!mon.is_fainted());
}

#[test]
fn build_wild_pokemon_rejects_bad_inputs_before_drawing_anything() {
    let dex = Dex::new();
    // The level/species/moves are the caller's, not rolled, so they are
    // checked ahead of `PickWildMonNature`'s draw: every script below is
    // *empty*, so a single draw before the rejection would panic on an
    // exhausted SequenceRng rather than quietly pass.
    // (MIN_LEVEL..=MAX_LEVEL is 1..=100, `include/constants/pokemon.h:145`-`:146`.)
    for (level, moves, expected) in [
        (101, vec![MoveId(33)], BattleError::InvalidLevel(101)),
        (0, vec![MoveId(33)], BattleError::InvalidLevel(0)),
        (5, vec![], BattleError::InvalidMoveCount(0)),
        (5, vec![MOVE_NONE], BattleError::PlaceholderMove(0)),
        (
            5,
            vec![MoveId(60_000)],
            BattleError::UnknownMove(MoveId(60_000)),
        ),
    ] {
        let mut rng = SequenceRng::new([]);
        assert_eq!(
            build_wild_pokemon(&dex, SpeciesId(1), level, moves, &mut rng),
            Err(expected)
        );
        assert_eq!(rng.draws(), 0, "a rejected request must not draw");
    }
}

#[test]
fn build_pokemon_with_random_personality_draws_only_personality_then_ivs() {
    let dex = Dex::new();
    // Personality (low then high half): (1, 0) -> 1 -- whatever nature
    // `1 % 25` (Lonely) implies, taken as-is with no rejection loop
    // reading it back. IVs: 0x001F -> hp 31/attack 0/defense 0, then
    // 0x03E0 -> speed 0/sp_attack 31/sp_defense 0 (same split as
    // `build_wild_pokemon`'s equivalent pin).
    let mut rng = SequenceRng::new([1, 0, 0x001F, 0x03E0]);
    let mon = build_pokemon_with_random_personality(
        &dex,
        SpeciesId(1), // Bulbasaur
        5,
        vec![MoveId(33)], // Tackle
        &mut rng,
    )
    .unwrap();
    assert_eq!(
        mon.personality(),
        1,
        "no forced nature means no rejection loop to consume extra draws"
    );
    assert_eq!(mon.ivs().hp, 31);
    assert_eq!(mon.ivs().attack, 0);
    assert_eq!(mon.ivs().defense, 0);
    assert_eq!(mon.ivs().speed, 0);
    assert_eq!(mon.ivs().sp_attack, 31);
    assert_eq!(mon.ivs().sp_defense, 0);
    assert_eq!(
        rng.draws(),
        4,
        "exactly one Random32 (2 draws) plus roll_ivs's 2 draws -- never more, since \
         there is no nature to match"
    );
}

#[test]
fn build_pokemon_with_random_personality_rejects_bad_inputs_before_drawing_anything() {
    let dex = Dex::new();
    for (level, moves, expected) in [
        (101, vec![MoveId(33)], BattleError::InvalidLevel(101)),
        (0, vec![MoveId(33)], BattleError::InvalidLevel(0)),
        (5, vec![], BattleError::InvalidMoveCount(0)),
        (5, vec![MOVE_NONE], BattleError::PlaceholderMove(0)),
        (
            5,
            vec![MoveId(60_000)],
            BattleError::UnknownMove(MoveId(60_000)),
        ),
    ] {
        let mut rng = SequenceRng::new([]);
        assert_eq!(
            build_pokemon_with_random_personality(&dex, SpeciesId(1), level, moves, &mut rng),
            Err(expected)
        );
        assert_eq!(rng.draws(), 0, "a rejected request must not draw");
    }
}

#[test]
fn rolled_ivs_are_always_within_the_upstream_range() {
    // Every 16-bit draw splits into three 5-bit fields, so no roll can
    // produce an out-of-range individual value (Gen-3 stat rolls -- see
    // `Ivs` -- not cryptographic initialization vectors).
    for value in [0u16, 1, 0x5A5A, 0xFFFF] {
        let mut rng = SequenceRng::new([value, !value]);
        assert!(roll_ivs(&mut rng).is_valid());
    }
}

/// `GiveBoxMonInitialMoveset` against the three species a Route 101
/// encounter can actually produce, at the two levels their table allows
/// (issue #169). These are the movesets the overworld handoff feeds
/// `Battle::new`, so they double as the pin that the wild side of that
/// battle is *executable* by this crate's turn engine.
///
/// Poochyena's rows previously asserted on `SpeciesId(261)` --
/// `SPECIES_OLD_UNOWN_K`, whose placeholder learnset happens to be
/// Tackle-only -- so they passed while pinning nothing (issue #207
/// review, round 4). `SPECIES_POOCHYENA` is `286`
/// (`include/constants/species.h:292`); its level-4/5 rows below are the
/// boundary pin that `entry.level > level` is the *exclusive* break
/// upstream's `moveLevel > (level << 9)` is -- Howl arrives exactly at
/// 5, so flipping the comparison to `>=` fails here and nowhere else.
#[test]
fn route_101_wild_movesets_match_their_level_up_learnsets() {
    // Learnsets from `src/data/pokemon/level_up_learnsets.h`:
    // Poochyena (`:3730`) LEVEL_UP_MOVE(1, TACKLE) then HOWL at 5;
    // Zigzagoon (`:3765`) TACKLE + GROWL both at 1, TAIL_WHIP at 5;
    // Wurmple (`:3799`) TACKLE + STRING_SHOT both at 1, POISON_STING at
    // 5. So nothing new is learned between the table's levels 2 and 3.
    // Move ids: Tackle 33, Growl 45, String Shot 81, Howl 336.
    for (species, level, expected) in [
        (286, 2, vec![MoveId(33)]),              // Poochyena: Tackle
        (286, 3, vec![MoveId(33)]),              // Howl is still two levels away
        (286, 4, vec![MoveId(33)]),              // ...one level away...
        (286, 5, vec![MoveId(33), MoveId(336)]), // ...and arrives exactly at 5
        (288, 2, vec![MoveId(33), MoveId(45)]),  // Zigzagoon: Tackle, Growl
        (288, 3, vec![MoveId(33), MoveId(45)]),
        (290, 2, vec![MoveId(33), MoveId(81)]), // Wurmple: Tackle, String Shot
        (290, 3, vec![MoveId(33), MoveId(81)]),
    ] {
        assert_eq!(
            initial_moveset(SpeciesId(species), level),
            expected,
            "species {species} at level {level}"
        );
    }
}

/// Learning order is preserved, and the moveset grows as the level does
/// -- Zigzagoon picks up Tail Whip at 5 and Headbutt at 9
/// (`level_up_learnsets.h:3765-3769`).
#[test]
fn a_higher_level_mon_knows_the_moves_it_has_reached() {
    let at_five = initial_moveset(SpeciesId(288), 5);
    let at_nine = initial_moveset(SpeciesId(288), 9);
    assert!(
        at_five.len() < at_nine.len(),
        "Zigzagoon learns more by level 9: {at_five:?} vs {at_nine:?}"
    );
    assert!(
        at_nine.starts_with(&at_five),
        "learning order must be preserved: {at_nine:?} does not extend {at_five:?}"
    );
}

/// `MON_HAS_MAX_MOVES` -> `DeleteFirstMoveAndGiveMoveToBoxMon`
/// (`pokemon.c:3009-3010`): past four moves the oldest is dropped, so a
/// level-100 mon holds its last four learnable moves in learning order,
/// with no duplicates.
#[test]
fn a_full_moveset_drops_the_oldest_move_first() {
    let learnsets = assets::LevelUpLearnsets::new();
    // Real species only: 261/252 previously in this list are Old Unown
    // placeholders with one-move learnsets that exercise nothing (issue
    // #207 review, round 4). Bulbasaur (1) and Treecko (277) both learn
    // more than four moves by 100, so the drop path really runs.
    for species in [286u16, 288, 290, 1, 277] {
        let moves = initial_moveset(SpeciesId(species), 100);
        assert!(
            moves.len() <= MAX_MON_MOVES,
            "species {species} kept {} moves",
            moves.len()
        );
        let mut deduped = moves.clone();
        deduped.dedup();
        assert_eq!(deduped, moves, "species {species} repeated a move");

        // The retained moves are the tail of the (de-duplicated)
        // learnset, oldest dropped first.
        let learnset = learnsets
            .get(SpeciesId(species))
            .expect("species is in the extracted learnset table");
        let mut all: Vec<MoveId> = Vec::new();
        for entry in learnset {
            if entry.level <= 100 && !all.contains(&entry.move_id) {
                all.push(entry.move_id);
            }
        }
        assert_eq!(
            moves,
            all[all.len().saturating_sub(MAX_MON_MOVES)..].to_vec()
        );
    }
}

/// [`ensure_wild_startable`] must agree with the real handoff, both ways
/// (issue #207 review): Route 101's rollable wild mons all pass, and the
/// first reachable moveset the turn engine cannot execute -- a level-3
/// Seedot's, Route 102 slot data -- is rejected, **naming the exact move**.
/// The rejection arm is a deliberate ratchet, and it has already moved once:
/// before issue #322 the Seedot was blocked by Bide *and* Harden, but Harden
/// is `EFFECT_DEFENSE_UP`, now part of the widened
/// `BattleScript_EffectStatUp` family, so only Bide is left. Naming it is
/// what forces the next move-coverage slice back here rather than letting a
/// bare `is_err()` go quietly stale.
#[test]
fn ensure_wild_startable_accepts_route_101_mons_and_rejects_a_bide_seedot() {
    let dex = Dex::new();
    // Route 101's land table: Wurmple, Poochyena, Zigzagoon at 2..=3.
    for species in [290, 286, 288] {
        for level in 2..=3 {
            assert_eq!(
                ensure_wild_startable(&dex, SpeciesId(species), level),
                Ok(()),
                "species {species} at level {level} must be startable"
            );
        }
    }
    // SPECIES_SEEDOT at Route 102's level 3 knows Bide (level 1) and
    // Harden (level 3), in that learnset order.
    assert_eq!(
        initial_moveset(SpeciesId(298), 3),
        vec![MoveId(117), MoveId(106)],
        "the fixture is only meaningful if the moveset really is Bide then Harden"
    );
    // Harden alone is fine now.
    assert_eq!(
        crate::battle::ensure_executable(&dex, MoveId(106)),
        Ok(()),
        "MOVE_HARDEN is EFFECT_DEFENSE_UP, executable since issue #322"
    );
    // Bide is what still blocks the table -- and it is reported first,
    // because it is the earlier slot. Its `power` is **1**, not 0
    // (`src/data/battle_moves.h:1527`), so it is refused for its
    // `EFFECT_BIDE` script rather than for being a status move: base power
    // alone would have waved it straight into the ordinary hit pipeline.
    assert_eq!(dex.move_data(MoveId(117)).unwrap().power, 1);
    assert_eq!(
        ensure_wild_startable(&dex, SpeciesId(298), 3),
        Err(BattleError::UnsupportedMoveEffect(MoveId(117))),
    );
}

/// An unknown species fails closed with an empty moveset -- rejected
/// downstream by `BattlePokemon::new`, never silently turned into a
/// move-less mon.
#[test]
fn an_unknown_species_yields_no_moves_and_is_rejected_downstream() {
    assert!(initial_moveset(SpeciesId(60_000), 5).is_empty());
    let dex = Dex::new();
    let mut rng = SequenceRng::new([]);
    assert_eq!(
        build_wild_pokemon(&dex, SpeciesId(60_000), 5, Vec::new(), &mut rng),
        Err(BattleError::InvalidMoveCount(0))
    );
    assert_eq!(rng.draws(), 0);
}
