//! Splits the music test suite by independent seam: frame-driven playback,
//! packed-song conversion, the mGBA oracle comparison, and PSG fade.

mod shared;

mod conversion;
mod fade;
mod oracle;
mod playback;
