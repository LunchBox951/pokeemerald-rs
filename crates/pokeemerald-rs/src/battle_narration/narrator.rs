//! The battle's live narration state, and the requests that open and close it.

use battle::BattleEvent;

use super::context::{BattleKind, Battler, NarrationContext, Side, Trainer};
use super::events;
use super::names::NarrationNames;
use super::{NarrationError, NarrationRequest, TemplateId};

/// What the narrator knows about the battle between events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scene {
    /// Wild or trainer opponent.
    pub kind: BattleKind,
    /// The player's name.
    pub player_name: String,
    /// The player's active battler.
    pub player: Battler,
    /// The opponent's active battler.
    pub opponent: Battler,
    /// The opposing trainer, for a trainer battle.
    pub trainer: Option<Trainer>,
    /// `gBattleStruct->hpScale`, which picks the player's send-out wording.
    pub hp_scale: u8,
}

/// Turns battle events into narration requests, tracking the active battlers
/// so each request names the battler that was active when its event happened.
#[derive(Debug, Clone)]
pub struct Narrator {
    pub(super) scene: Scene,
    /// The incoming battler per side, set by [`Narrator::stage_replacement`].
    pub(super) staged: [Option<Battler>; 2],
}

/// `gBattleStruct->hpScale` for an opponent at `current_hp` of `max_hp`.
fn hp_scale(current_hp: u32, max_hp: u32) -> u8 {
    if max_hp == 0 || current_hp == 0 {
        return 0;
    }
    // Widen so the percentage cannot overflow; a nonzero HP reads as 1%.
    let percent = (u64::from(current_hp) * 100 / u64::from(max_hp)).max(1);
    match percent {
        70.. => 0,
        40..=69 => 1,
        10..=39 => 2,
        _ => 3,
    }
}

pub(super) const fn side_index(side: Side) -> usize {
    match side {
        Side::Player => 0,
        Side::Opponent => 1,
    }
}

impl Narrator {
    /// A narrator starting from `scene`.
    #[must_use]
    pub const fn new(scene: Scene) -> Self {
        Self {
            scene,
            staged: [None, None],
        }
    }

    /// Names the battler `side` sends out next, with its nickname and ability.
    ///
    /// A send-out event carries only a species, so without a staged battler
    /// the replacement is named after its species and has no recorded ability.
    /// The staged battler is used by the next send-out event for `side`.
    pub fn stage_replacement(&mut self, side: Side, battler: Battler) {
        self.staged[side_index(side)] = Some(battler);
    }

    /// Records the opponent's health for the player's next send-out wording.
    ///
    /// Upstream runs `hpthresholds BS_FAINTED` right before
    /// `STRINGID_SWITCHINMON` (`data/battle_scripts_1.s:2887`-`:2888`), so the
    /// wording follows the opponent's health when the replacement is sent out,
    /// not when the narrator was built. Call this before narrating the
    /// [`BattleEvent::PlayerSentOut`] event. `Cmd_hpthresholds`
    /// (`src/battle_script_commands.c:6258`-`:6285`) reads the percentage
    /// from the live HP; a fainted opponent keeps scale 0.
    pub fn observe_opponent_health(&mut self, current_hp: u32, max_hp: u32) {
        self.scene.hp_scale = hp_scale(current_hp, max_hp);
    }

    /// The current scene.
    #[must_use]
    pub const fn scene(&self) -> &Scene {
        &self.scene
    }

    /// The requests for one battle event, in print order.
    ///
    /// A send-out event first makes the staged replacement (see
    /// [`Narrator::stage_replacement`]) the active battler.
    ///
    /// # Errors
    ///
    /// Returns [`NarrationError`] when the event needs context the scene lacks.
    pub fn requests(
        &mut self,
        names: &impl NarrationNames,
        event: &BattleEvent,
    ) -> Result<Vec<NarrationRequest>, NarrationError> {
        events::requests(self, names, event)
    }

    /// The opening message (`STRINGID_INTROMSG`, `src/battle_message.c:1985`-`:2022`).
    #[must_use]
    pub fn introduction(&self) -> NarrationRequest {
        let symbol = match self.scene.kind {
            BattleKind::Trainer => "sText_Trainer1WantsToBattle",
            BattleKind::Wild => "sText_WildPkmnAppeared",
        };
        NarrationRequest {
            template: TemplateId::Special(symbol),
            context: self.context(),
        }
    }

    /// The first send-out message of `side` (`STRINGID_INTROSENDOUT`,
    /// `:2024`-`:2064`); a wild opponent sends out nothing.
    #[must_use]
    pub fn initial_send_out(&self, side: Side) -> Option<NarrationRequest> {
        let symbol = match (side, self.scene.kind) {
            (Side::Player, _) => "sText_GoPkmn",
            (Side::Opponent, BattleKind::Trainer) => "sText_Trainer1SentOutPkmn",
            (Side::Opponent, BattleKind::Wild) => return None,
        };
        Some(NarrationRequest {
            template: TemplateId::Special(symbol),
            context: self.context(),
        })
    }

    /// A snapshot of the scene with no roles and empty buffers.
    pub(super) fn context(&self) -> NarrationContext {
        let mut context = NarrationContext::new(self.scene.kind, self.scene.player_name.clone());
        context.player = Some(self.scene.player.clone());
        context.opponent = Some(self.scene.opponent.clone());
        context.trainer.clone_from(&self.scene.trainer);
        context
    }
}
