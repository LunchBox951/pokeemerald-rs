//! Splits the pack-reader test suite by independent seam: the synthetic
//! fixture builders every other module shares, generic container/accessor
//! basics, layout/font/text-window integration, audio-schema accessor
//! integration, and the ignored real-pack round trip.

mod shared;

mod audio;
mod container;
mod layout;
mod real_pack;
