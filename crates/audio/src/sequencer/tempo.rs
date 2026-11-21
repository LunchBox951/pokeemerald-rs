//! The player's tempo state: the base tempo a `TEMPO` command sets, the
//! player-local multiplier a caller adjusts, and the effective tempo that
//! drives sequence ticks.

use crate::sequence::clamp_tempo;

/// The multiplier a new playback starts with: `0x100` is a factor of one
/// (`m4a.c:629`-`:637`, `tempoU = 0x100`).
pub(super) const UNITY_TEMPO_MULTIPLIER: u16 = 0x100;

/// Base tempo (`tempoD`), multiplier (`tempoU`) and effective tempo
/// (`tempoI`) of one music player.
///
/// Both the `TEMPO` command (`m4a_1.s:920`-`:929`) and the multiplier control
/// (`m4a.c:1234`-`:1242`) recompute the effective tempo as
/// `(tempoD * tempoU) >> 8` over the full 32-bit product, and store it in the
/// player's 16-bit `tempoI` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct TempoControl {
    base: u16,
    multiplier: u16,
    effective: u16,
}

impl TempoControl {
    /// A new player at `base` BPM and the unity multiplier.
    pub(super) fn new(base: u16) -> Self {
        let mut tempo = Self {
            base: clamp_tempo(base),
            multiplier: UNITY_TEMPO_MULTIPLIER,
            effective: 0,
        };
        tempo.recompute();
        tempo
    }

    /// The base tempo the last `TEMPO` command (or the song header) set.
    pub(super) fn base(self) -> u16 {
        self.base
    }

    /// The player multiplier, `0x100` for a factor of one.
    pub(super) fn multiplier(self) -> u16 {
        self.multiplier
    }

    /// The tempo that advances the tick accumulator each frame.
    pub(super) fn effective(self) -> u16 {
        self.effective
    }

    /// Applies a `TEMPO` command: replaces the base tempo (clamped to the
    /// command's own bound), keeps the multiplier, and recomputes.
    pub(super) fn set_base(&mut self, bpm: u16) {
        self.base = clamp_tempo(bpm);
        self.recompute();
    }

    /// `m4aMPlayTempoControl`: replaces the multiplier, keeps the base tempo,
    /// and recomputes.
    pub(super) fn set_multiplier(&mut self, multiplier: u16) {
        self.multiplier = multiplier;
        self.recompute();
    }

    #[expect(
        clippy::cast_possible_truncation,
        reason = "tempoI is a 16-bit field: `strh` keeps the low half of the shifted product \
                  (m4a_1.s:925-:928, m4a.c:1239)"
    )]
    fn recompute(&mut self) {
        self.effective = ((u32::from(self.base) * u32::from(self.multiplier)) >> 8) as u16;
    }
}
