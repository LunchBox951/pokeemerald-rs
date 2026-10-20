//! Construction and admission of a battle: the initial turn-number draw, the conditional Speed-tie draw, and the starting battler state for wild, first-battle, and trainer encounters.

use crate::damage::BattleRng;
use crate::dex::Dex;
use crate::error::BattleError;
use crate::pokemon::{BattlePokemon, MOVE_NONE};
use crate::secondary;
use crate::turn_order::resolve_order;
use assets::trainers::TrainerId;

use super::opponent_ai::all_known_moves_are_spent;
use super::trainer::{self, TrainerContext};
use super::{admission, ensure_executable, trainer_ai, Battle, BattleKind, NO_MOVE_PRIORITY};

fn initialize_turn_rng_state(
    player: &BattlePokemon,
    enemy: &BattlePokemon,
    rng: &mut impl BattleRng,
) -> u16 {
    let random_turn_number = rng.next_u16();
    // Upstream seeds a MOVE_NONE order here. A Speed tie consumes RNG even
    // though the order is discarded (`src/battle_main.c:3852`-`:3861`).
    let _initial_order = resolve_order(
        NO_MOVE_PRIORITY,
        NO_MOVE_PRIORITY,
        player.speed_for_turn_order(),
        enemy.speed_for_turn_order(),
        rng,
    );
    random_turn_number
}

/// Re-reads Deoxys's Speed Forme stats from the EVs the player's side carries
/// in. [`BattlePokemon::gain_evs`] leaves the earning battle's stats alone, as
/// `MonGainEVs` writes only the party copy (`pokemon.c:5988`); upstream's
/// battle-start copy then reads them back through `GetMonData` and
/// `GetDeoxysStat` (`battle_controller_player.c:1641`-`:1645`).
fn refresh_carried_speed_forme_stats(
    player: &mut BattlePokemon,
    player_reserves: &mut [BattlePokemon],
) {
    for mon in std::iter::once(player).chain(player_reserves.iter_mut()) {
        mon.apply_speed_forme_evs();
    }
}

impl Battle {
    /// Starts an ordinary wild battle or the scripted first battle.
    ///
    /// The enemy's full moveset is validated before RNG is consumed because
    /// its action selection can choose any slot. Player moves are validated
    /// when selected. The scripted first battle suppresses critical hits,
    /// refuses Run from Pokémon without Run Away, and uses its dedicated
    /// opponent AI.
    ///
    /// A depleted enemy slot needs only real move data, not an executable
    /// effect: `Battle::act` fails it as [`BattleEvent::FailedNoPp`](super::BattleEvent::FailedNoPp) before
    /// running one, matching `Cmd_attackcanceler`'s no-PP jump
    /// (`src/battle_script_commands.c:934`-`:939`). Soundproof's own block
    /// runs earlier still (`:932`-`:933`), but needs only the already-checked
    /// move data, not an executable effect, so it cannot make this unsafe.
    /// Struggle is that jump's one exemption (`:934`) and so the only move a
    /// depleted slot still executes; it has an executable pipeline of its own,
    /// so the relaxation still reaches `Battle::execute_move` with nothing
    /// unresolvable.
    ///
    /// # Errors
    ///
    /// Returns [`BattleError::FaintedBattler`] for a fainted participant or
    /// the first move or ability validation error from the enemy's moveset.
    /// Errors leave the RNG untouched.
    pub fn new(
        dex: Dex,
        player: BattlePokemon,
        enemy: BattlePokemon,
        first_battle: bool,
        rng: &mut impl BattleRng,
    ) -> Result<Self, BattleError> {
        Self::new_with_player_reserves(dex, player, Vec::new(), enemy, first_battle, rng)
    }

    /// Starts an ordinary wild battle or the scripted first battle with a
    /// player party larger than the active member.
    ///
    /// `player_reserves` is consumed in order under a headless party-order
    /// policy: an active faint sends out the first entry that is not itself
    /// fainted, with no player choice and no party-screen UI
    /// (`Cmd_checkteamslost`, `src/battle_script_commands.c:3534`-`:3564`;
    /// `BattleScript_HandleFaintedMon`'s send-out branch,
    /// `data/battle_scripts_1.s:2830`-`:2896`). [`Battle::new`] is this
    /// constructor with an empty reserve list. A reserve's own moves are
    /// validated as the player selects them, exactly like the active
    /// member's; the enemy's moveset is validated against every non-fainted
    /// reserve up front too, since any of them may face it as a defender
    /// with no further checkpoint before that turn. A fainted reserve is
    /// admitted without that check: `Battle::send_out_next_player_reserve`
    /// never selects it, so it can never become the enemy's defender.
    ///
    /// # Errors
    ///
    /// Returns [`BattleError::FaintedBattler`] for a fainted active
    /// participant, or the first move or ability validation error from the
    /// enemy's moveset against the active member or any non-fainted reserve.
    /// A fainted reserve is admitted: it models a player party that already
    /// lost a member before the battle. A non-fainted participant that is
    /// Paralysed with Limber, Poisoned with Immunity, confused with Own Tempo,
    /// or holds Truant, Intimidate, Speed Boost, Color Change, Trace, Effect
    /// Spore, Flame Body, Cute Charm, Drizzle, Drought or Sand Stream fails
    /// with [`BattleError::UnportedAbilityInteraction`].
    /// Errors leave the RNG untouched.
    pub fn new_with_player_reserves(
        dex: Dex,
        mut player: BattlePokemon,
        mut player_reserves: Vec<BattlePokemon>,
        enemy: BattlePokemon,
        first_battle: bool,
        rng: &mut impl BattleRng,
    ) -> Result<Self, BattleError> {
        refresh_carried_speed_forme_stats(&mut player, &mut player_reserves);
        if player.is_fainted() {
            return Err(BattleError::FaintedBattler(true));
        }
        if enemy.is_fainted() {
            return Err(BattleError::FaintedBattler(false));
        }
        for (index, slot) in enemy.moves().iter().enumerate() {
            // A depleted slot must still be real move data -- not the
            // empty-slot placeholder or an id outside the dex -- but its
            // effect need not be executable; see the no-PP note above.
            if slot.move_id == MOVE_NONE {
                return Err(BattleError::PlaceholderMove(index));
            }
            dex.move_data(slot.move_id)?;
            // Zero-PP moves stop before applying effects, Struggle
            // excepted (`src/battle_script_commands.c:934`-`:939`). A
            // non-fainted reserve becomes this same enemy's defender the
            // moment it is sent out, with no further validation at that
            // point, so every non-fainted reserve is checked here too. A
            // fainted reserve is skipped: `send_out_next_player_reserve`
            // never selects it.
            if slot.pp > 0 {
                ensure_executable(&dex, slot.move_id)?;
                for defender in std::iter::once(&player).chain(
                    player_reserves
                        .iter()
                        .filter(|reserve| !reserve.is_fainted()),
                ) {
                    secondary::ensure_admissible(&dex, slot.move_id, &enemy, defender)?;
                }
            }
        }
        admission::ensure_participants_admissible(
            [&player, &enemy].into_iter().chain(
                player_reserves
                    .iter()
                    .filter(|reserve| !reserve.is_fainted()),
            ),
        )?;
        let random_turn_number = initialize_turn_rng_state(&player, &enemy, rng);
        Ok(Self {
            dex,
            player,
            player_reserves,
            player_slot: 0,
            enemy,
            run_attempts: 0,
            random_turn_number,
            outcome: None,
            kind: if first_battle {
                BattleKind::FirstBattle
            } else {
                BattleKind::Wild
            },
            turn_counter: 0,
            turn_has_started: false,
            pending_residual_selection: None,
            retained_slots: [0; 2],
            last_move_used: MOVE_NONE,
        })
    }

    /// Starts a trainer battle with `party[0]` active and the rest on the
    /// bench. Equivalent to [`Battle::new_trainer_with_player_reserves`] with
    /// no player reserves.
    ///
    /// # Errors
    ///
    /// Returns [`BattleError::EmptyTrainerParty`] for an empty party,
    /// [`BattleError::UnknownTrainer`] for an unknown trainer,
    /// [`BattleError::FaintedBattler`] for a fainted active battler, or the
    /// first AI, move, or ability validation error. Errors leave the RNG untouched.
    pub fn new_trainer(
        dex: Dex,
        player: BattlePokemon,
        trainer: TrainerId,
        party: Vec<BattlePokemon>,
        rng: &mut impl BattleRng,
    ) -> Result<Self, BattleError> {
        Self::new_trainer_with_player_reserves(dex, player, Vec::new(), trainer, party, rng)
    }

    /// Starts a trainer battle with `party[0]` active and the rest on the
    /// bench, and `player_reserves` under the same ordered, headless
    /// replacement policy as [`Battle::new_with_player_reserves`]
    /// (`Cmd_checkteamslost`, `src/battle_script_commands.c:3534`-`:3564`).
    /// Every trainer party move is additionally validated against every
    /// non-fainted reserve, not just `player`.
    ///
    /// # Errors
    ///
    /// Returns [`BattleError::EmptyTrainerParty`] for an empty party,
    /// [`BattleError::UnknownTrainer`] for an unknown trainer,
    /// [`BattleError::FaintedBattler`] for a fainted active battler, or the
    /// first AI, move, or ability validation error against the active member
    /// or any non-fainted reserve, including
    /// [`BattleError::AmbiguousPlayerAbility`] for a two-ability player
    /// member without a trapping ability when the trainer scores with
    /// `CheckBadMove` and some non-fainted party member still has a
    /// PP-positive move. Errors leave the RNG untouched.
    pub fn new_trainer_with_player_reserves(
        dex: Dex,
        mut player: BattlePokemon,
        mut player_reserves: Vec<BattlePokemon>,
        trainer: TrainerId,
        mut party: Vec<BattlePokemon>,
        rng: &mut impl BattleRng,
    ) -> Result<Self, BattleError> {
        refresh_carried_speed_forme_stats(&mut player, &mut player_reserves);
        if party.is_empty() {
            return Err(BattleError::EmptyTrainerParty(trainer));
        }
        let data = trainer::trainer_data(trainer)?;
        trainer_ai::ensure_supported_flags(data.ai_flags)?;
        if player.is_fainted() {
            return Err(BattleError::FaintedBattler(true));
        }
        if party[0].is_fainted() {
            return Err(BattleError::FaintedBattler(false));
        }
        for mon in &party {
            for (index, slot) in mon.moves().iter().enumerate() {
                // A depleted slot must still be real move data, but the AI
                // discards zero-PP slots before scoring and the no-PP abort
                // precedes any effect, so executable/scoreable admission
                // applies only to slots with PP remaining.
                if slot.move_id == MOVE_NONE {
                    return Err(BattleError::PlaceholderMove(index));
                }
                dex.move_data(slot.move_id)?;
                if slot.pp > 0 {
                    trainer::ensure_move_playable(&dex, slot.move_id, data.ai_flags)?;
                    for defender in std::iter::once(&player).chain(
                        player_reserves
                            .iter()
                            .filter(|reserve| !reserve.is_fainted()),
                    ) {
                        secondary::ensure_admissible(&dex, slot.move_id, mon, defender)?;
                    }
                }
            }
        }

        admission::ensure_participants_admissible(
            std::iter::once(&player)
                .chain(party.iter().filter(|mon| !mon.is_fainted()))
                .chain(
                    player_reserves
                        .iter()
                        .filter(|reserve| !reserve.is_fainted()),
                ),
        )?;

        // An all-spent battler is diverted to Struggle before the AI picks a
        // move (`battle_main.c:4183`-`:4191`), and PP never refills in
        // battle, so only a party that can still score reads the target's
        // ability.
        let party_can_score = party
            .iter()
            .any(|mon| !mon.is_fainted() && !all_known_moves_are_spent(mon));
        if party_can_score
            && data
                .ai_flags
                .contains(assets::trainers::AiFlags::CHECK_BAD_MOVE)
        {
            for target in std::iter::once(&player).chain(
                player_reserves
                    .iter()
                    .filter(|reserve| !reserve.is_fainted()),
            ) {
                trainer_ai::ensure_target_ability_unambiguous(target)?;
            }
        }

        let enemy = party.remove(0);
        let random_turn_number = initialize_turn_rng_state(&player, &enemy, rng);
        Ok(Self {
            dex,
            player,
            player_reserves,
            player_slot: 0,
            enemy,
            run_attempts: 0,
            random_turn_number,
            outcome: None,
            kind: BattleKind::Trainer(TrainerContext::new(trainer, data, party)),
            turn_counter: 0,
            turn_has_started: false,
            pending_residual_selection: None,
            retained_slots: [0; 2],
            last_move_used: MOVE_NONE,
        })
    }
}
