//! Platform subsystem (S-1): window, input mapping, frame pacing,
//! `softbuffer` presentation, and audio output.
//!
//! Owned types, one per concern `(oop-boundaries)`:
//!
//! - [`input::Buttons`] / [`input::ButtonState`] / [`input::Keymap`] — the
//!   GBA button bitmask (mirroring `pokeemerald/include/gba/io_reg.h`), the
//!   held/newly-pressed frame state (mirroring `JOY_HELD`/`JOY_NEW`), and the
//!   default keyboard mapping. Remapping is out of scope.
//! - [`pacing::FramePacer`] — a fixed-cadence wait targeting the GBA's real
//!   ~59.7275 Hz refresh rate (`WaitForVBlank`'s behaviour), built on an
//!   injectable time seam so it is unit-testable without ever sleeping.
//! - [`present::Letterbox`] and [`present::blit`] — integer-scale +
//!   letterbox math and the blit that expands a native 240x160 buffer into a
//!   window-sized `softbuffer` surface.
//! - [`window::Platform`] — the window itself: owns the `winit` event loop,
//!   the native window, and the `softbuffer` surface, and exposes all of the
//!   above behind a small per-frame API (pump events, read button state,
//!   present a frame, pace to the next one).
//! - [`audio::AudioOutput`] — the audio output device (or a headless null
//!   stand-in): owns at most one `cpal` output stream and exposes a
//!   [`ring::Producer`] handle the future `audio` crate (M4A engine, S-3)
//!   fills from its own thread. `cpal` is owner-approved for exactly this
//!   crate and use in Discussion #78.
//!
//! CI is headless, so nothing here opens a real window or a real `cpal`
//! stream in a test; only [`window::Platform`] and [`audio::AudioOutput`]
//! touch `winit`/`softbuffer`/`cpal` directly, and `AudioOutput` only does so
//! behind its explicit `open` constructor — its `null` constructor and the
//! `ring`/`resample` modules it is built on are pure logic with full
//! unit-test coverage.

pub mod audio;
pub mod error;
pub mod input;
pub mod pacing;
pub mod present;
pub mod resample;
pub mod ring;
pub mod window;

pub use audio::AudioOutput;
pub use error::PlatformError;
pub use input::{ButtonState, Buttons, Keymap};
pub use pacing::{FramePacer, GBA_FRAME_PERIOD};
pub use present::{Frame, Letterbox, GBA_HEIGHT, GBA_WIDTH, PIXEL_COUNT};
pub use resample::Resampler;
pub use ring::{ring_buffer, Consumer, Producer};
pub use window::Platform;
