use assets::{AbilityId, MoveId, SpeciesId};
use battle::{BattlePokemon, BattleRng, Dex, Ivs, Nature, MAX_IV};

pub struct SequenceRng {
    scripted_values: Vec<u16>,
    draw_count: usize,
}

impl SequenceRng {
    pub fn new(scripted_values: impl IntoIterator<Item = u16>) -> Self {
        Self {
            scripted_values: scripted_values.into_iter().collect(),
            draw_count: 0,
        }
    }

    pub fn draws(&self) -> usize {
        self.draw_count
    }
}

impl BattleRng for SequenceRng {
    fn next_u16(&mut self) -> u16 {
        let value = self
            .scripted_values
            .get(self.draw_count)
            .copied()
            .unwrap_or_else(|| panic!("SequenceRng exhausted after {} draws", self.draw_count));
        self.draw_count += 1;
        value
    }
}

pub const MAX_IVS: Ivs = Ivs {
    hp: MAX_IV,
    attack: MAX_IV,
    defense: MAX_IV,
    speed: MAX_IV,
    sp_attack: MAX_IV,
    sp_defense: MAX_IV,
};

pub fn max_iv_mon(
    dex: &Dex,
    species_id: u16,
    level: u8,
    known_moves: Vec<MoveId>,
) -> BattlePokemon {
    max_iv_mon_with_personality(
        dex,
        species_id,
        level,
        known_moves,
        u32::from(Nature::Hardy.id()),
    )
}

/// Builds a max-IV mon like [`max_iv_mon`], but with an explicit personality
/// so a caller can choose which ability slot a two-ability species starts
/// with.
pub fn max_iv_mon_with_personality(
    dex: &Dex,
    species_id: u16,
    level: u8,
    known_moves: Vec<MoveId>,
    personality: u32,
) -> BattlePokemon {
    BattlePokemon::new(
        dex,
        SpeciesId(species_id),
        level,
        MAX_IVS,
        personality,
        known_moves,
    )
    .unwrap()
}

/// Personality 25 is odd (secondary ability slot) yet still lands on the
/// neutral Hardy nature (`25 % 25 == 0`), like [`max_iv_mon`]'s default --
/// see [`slow_runner_rattata`].
#[allow(
    dead_code,
    reason = "only turn_engine's escape/move_selection tests use this; wild_battle.rs's own test binary compiles this shared module too"
)]
pub const SECONDARY_ABILITY_PERSONALITY: u32 = 25;

/// The L5 Rattata every escape/move-selection runner fixture builds, on
/// ability slot 1 (Guts) instead of slot 0 (Run Away): outside the Battle
/// Pyramid, upstream's `TryRunFromBattle` escapes a Run Away holder
/// unconditionally, bypassing the plain speed/run-tries branch these tests
/// assert (`battle_util.c:427-447`).
#[allow(
    dead_code,
    reason = "only turn_engine's escape/move_selection tests use this; wild_battle.rs's own test binary compiles this shared module too"
)]
pub fn slow_runner_rattata(dex: &Dex) -> BattlePokemon {
    let runner =
        max_iv_mon_with_personality(dex, 19, 5, vec![MoveId(33)], SECONDARY_ABILITY_PERSONALITY);
    assert_eq!(
        runner.ability(),
        AbilityId::GUTS,
        "the runner must not carry Run Away, or upstream escapes unconditionally"
    );
    runner
}
