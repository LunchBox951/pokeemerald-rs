//! Platform subsystem (S-1): window, input mapping, frame pacing, and
//! `softbuffer` presentation. Audio output is explicitly deferred to S-3
//! (`cpal`, its own future sign-off per Discussion #17) and is out of scope
//! here.
//!
//! Four owned types, one per concern `(oop-boundaries)`:
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
//!
//! CI is headless, so nothing here opens a real window in a test; only
//! [`window::Platform`] touches `winit`/`softbuffer` directly; the other
//! three modules are pure logic with full unit-test coverage.

pub mod error;
pub mod input;
pub mod pacing;
pub mod present;
pub mod window;

pub use error::PlatformError;
pub use input::{ButtonState, Buttons, Keymap};
pub use pacing::{FramePacer, GBA_FRAME_PERIOD};
pub use present::{Frame, Letterbox, GBA_HEIGHT, GBA_WIDTH, PIXEL_COUNT};
pub use window::Platform;
