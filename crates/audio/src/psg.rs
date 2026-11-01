//! Sample-rate waveform generators for the four CGB programmable sound channels.

mod common;
mod frame_sequencer;
mod noise;
mod square;
mod sweep;
mod wave;

pub use frame_sequencer::FrameSequencer128Hz;
pub use noise::NoiseChannel;
pub use square::SquareChannel;
pub use sweep::{Sweep, SweepResult};
pub use wave::WaveChannel;
