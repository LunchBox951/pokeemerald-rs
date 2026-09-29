//! Splits the voicegroup test suite by independent validation seam: the
//! shared voice fixtures, key-split tables, slot-count and codec framing,
//! `DirectSound` pan, square duty, noise period, and CGB envelope domains.

mod shared;

mod cgb_envelope;
mod direct_sound_pan;
mod key_split;
mod noise_period;
mod slot_count_codec;
mod square_duty;
