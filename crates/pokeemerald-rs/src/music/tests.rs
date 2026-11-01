//! Splits the music test suite by independent seam: frame-driven playback,
//! packed-song conversion, the mGBA oracle comparison, PSG fade, and the
//! player output tail, fade, and reverb context.

mod player_shared;
mod shared;

mod conversion;
mod fade;
mod oracle;
mod playback;
mod player_fade;
mod player_output_tail;
mod player_reverb_context;
