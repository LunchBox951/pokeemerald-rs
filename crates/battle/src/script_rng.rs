//! A test-only [`BattleRng`] fed from a fixed sequence — the tool every
//! pipeline's tests use to pin *both* the values a script consumes and how
//! **many** it consumes.
//!
//! Compiled only under `cfg(test)`. It exists because a draw *count* is a
//! behavioural claim in this crate (see any pipeline module's "RNG draws"
//! table): a shared stream that advances one step too far silently
//! desynchronises every later roll in the battle, so the tests assert on
//! [`SequenceRng::draws`] as routinely as on outcomes, and a script that
//! draws one time too many runs off the end of the sequence and panics
//! rather than reading a value the test never meant it to have.

use crate::damage::BattleRng;

/// A [`BattleRng`] that hands back a fixed sequence of `u16`s and counts how
/// many were taken.
pub(crate) struct SequenceRng {
    values: Vec<u16>,
    index: usize,
}

impl SequenceRng {
    /// A stream that will yield `values`, in order, and panic if asked for
    /// one more.
    pub(crate) fn new(values: impl IntoIterator<Item = u16>) -> Self {
        Self {
            values: values.into_iter().collect(),
            index: 0,
        }
    }

    /// How many values have been drawn so far.
    pub(crate) fn draws(&self) -> usize {
        self.index
    }
}

impl BattleRng for SequenceRng {
    fn next_u16(&mut self) -> u16 {
        let value = self
            .values
            .get(self.index)
            .copied()
            .expect("SequenceRng exhausted: the script drew more than the test scripted");
        self.index += 1;
        value
    }
}
