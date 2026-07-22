//! The [`Platform`] window type (S-1): owns the `winit` event loop, the
//! native window, and the `softbuffer` presentation surface, and glues them
//! together with [`crate::input`] and [`crate::pacing`] behind a small
//! per-frame API.
//!
//! The windowed path itself is not unit tested directly — CI is headless and
//! must never open a real window. [`crate::input`], [`crate::pacing`], and
//! [`crate::present`] carry all the tested logic; this module is thin glue
//! over `winit`/`softbuffer` plus a manual event pump (see
//! [`Platform::pump`]) so a caller stays in control of its own frame loop
//! rather than handing control to `winit`.
//!
//! [`Platform::new_headless`] (F-3, V-1) is the explicit, always-available
//! null backend for tests and CI — mirroring [`crate::audio::AudioOutput`]'s
//! `null` constructor: no cargo feature flag, just a second constructor that
//! opens no OS window/event loop/surface. It is what backs `xtask`'s `e2e
//! --suite smoke` run (see `pokeemerald_rs::App::new_headless`).
//!
//! Quit is a platform-level concept, not a GBA button: the OS window-close
//! control and the Escape key both end the loop via [`Platform::pump`]
//! returning `false`, independent of [`crate::input::Keymap`]'s GBA button
//! bindings. The null backend never signals this on its own — there is no
//! window to close — so [`Platform::pump`] always reports "keep going".

use std::num::NonZeroU32;
use std::rc::Rc;
use std::time::{Duration, Instant};

use softbuffer::{Context, Surface};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::platform::pump_events::{EventLoopExtPumpEvents, PumpStatus};
use winit::window::{Window, WindowId};

use crate::error::PlatformError;
use crate::input::{ButtonState, Buttons, Keymap};
use crate::pacing::FramePacer;
use crate::present::{self, Frame, Letterbox};

/// The window and presentation surface, created lazily once `winit` resumes
/// the app (required on some platforms, e.g. Android; harmless everywhere
/// else).
struct Inner {
    window: Rc<Window>,
    // Must outlive `surface`; never read again after construction, but
    // dropping it invalidates the surface.
    _context: Context<Rc<Window>>,
    surface: Surface<Rc<Window>, Rc<Window>>,
}

/// The `winit` [`ApplicationHandler`] driving window/input events.
///
/// Keyboard events accumulate into `frame_held` as they arrive; [`Platform`]
/// folds that into a [`ButtonState`] once per [`Platform::pump`] call, which
/// is the held/newly-pressed frame boundary this crate exposes.
struct WinitApp {
    title: String,
    keymap: Keymap,
    frame_held: Buttons,
    buttons: ButtonState,
    inner: Option<Inner>,
    init_error: Option<PlatformError>,
}

impl WinitApp {
    fn new(title: String) -> Self {
        Self {
            title,
            keymap: Keymap::default_keymap(),
            frame_held: Buttons::NONE,
            buttons: ButtonState::new(),
            inner: None,
            init_error: None,
        }
    }
}

impl ApplicationHandler for WinitApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // Winit may deliver redundant back-to-back `Resumed` events; only
        // (re)create the window/surface once, and don't retry after a
        // recorded failure.
        if self.inner.is_some() || self.init_error.is_some() {
            return;
        }

        let attributes = Window::default_attributes()
            .with_title(self.title.clone())
            .with_inner_size(LogicalSize::new(
                f64::from(present::GBA_WIDTH * 2),
                f64::from(present::GBA_HEIGHT * 2),
            ))
            .with_resizable(true);

        let created = (|| -> Result<Inner, PlatformError> {
            let window = Rc::new(event_loop.create_window(attributes)?);
            let context = Context::new(window.clone())?;
            let surface = Surface::new(&context, window.clone())?;
            Ok(Inner {
                window,
                _context: context,
                surface,
            })
        })();

        match created {
            Ok(inner) => self.inner = Some(inner),
            Err(err) => self.init_error = Some(err),
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            // Winit does not synthesize key-release events on focus loss
            // (macOS/Wayland), so a key held across an alt-tab would stay
            // stuck in `frame_held` forever. Drop all held state instead;
            // still-held keys re-register on the next real press.
            WindowEvent::Focused(false) => self.frame_held = Buttons::NONE,
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    // Escape is a dev/emulator quit affordance, not a GBA
                    // button (there is no hardware key it could map to), so
                    // it is handled here directly rather than through
                    // `Keymap` — same exit path as a window-close request.
                    if code == KeyCode::Escape && event.state == ElementState::Pressed {
                        event_loop.exit();
                        return;
                    }
                    if let Some(button) = self.keymap.lookup(code) {
                        match event.state {
                            ElementState::Pressed => self.frame_held |= button,
                            ElementState::Released => self.frame_held &= !button,
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

/// The `winit` event loop plus (lazily created, see [`WinitApp`]) native
/// window and `softbuffer` surface backing a windowed [`Platform`].
///
/// Boxed inside [`Backend::Window`] so the null variant (just a
/// [`ButtonState`]) doesn't force every [`Platform`] — headless ones
/// included — to be sized for `winit`'s much larger `EventLoop`
/// (`clippy::large_enum_variant`).
struct WindowBackend {
    event_loop: EventLoop<()>,
    app: WinitApp,
}

/// Which concrete backend a [`Platform`] is driving: a real OS window, or
/// the explicit headless/null stand-in (see [`Platform::new_headless`]).
enum Backend {
    /// A real OS window — see [`WindowBackend`].
    Window(Box<WindowBackend>),
    /// No OS window, event loop, or surface — see [`Platform::new_headless`].
    Null { buttons: ButtonState },
}

/// An open native window (or, headlessly, `platform`'s null backend): input,
/// frame pacing, and `softbuffer` presentation, exposed as a small
/// per-frame API.
///
/// Typical usage:
///
/// ```no_run
/// use platform::{Platform, present};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let mut platform = Platform::new("pokeemerald-rs")?;
/// let frame = present::test_pattern();
///
/// while platform.pump()? {
///     if platform.buttons().is_held(platform::Buttons::START) {
///         break;
///     }
///     platform.present(&frame)?;
///     platform.wait_for_next_frame();
/// }
/// # Ok(())
/// # }
/// ```
pub struct Platform {
    backend: Backend,
    pacer: FramePacer,
}

impl Platform {
    /// Open a resizable native window titled `title`.
    ///
    /// The window and presentation surface are created lazily on the first
    /// [`Platform::pump`] call — `winit` requires this on some platforms,
    /// which only allow surface creation once the app has been "resumed".
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError::EventLoop`] if the platform's windowing
    /// event loop could not be created.
    pub fn new(title: impl Into<String>) -> Result<Self, PlatformError> {
        Ok(Self {
            backend: Backend::Window(Box::new(WindowBackend {
                event_loop: EventLoop::new()?,
                app: WinitApp::new(title.into()),
            })),
            pacer: FramePacer::new(),
        })
    }

    /// An explicit headless/null backend (F-3, V-1): opens no `winit` event
    /// loop, native window, or `softbuffer` surface.
    ///
    /// Always available (no display server required), so this is the only
    /// backend `cargo test`/CI's `xtask e2e --suite smoke` run may
    /// construct — mirrors [`crate::audio::AudioOutput::null`]'s pattern.
    /// [`Platform::pump`] always reports "keep going" (there is no
    /// window-close event to simulate), [`Platform::buttons`] always reads
    /// as nothing held (there is no keyboard to inject from),
    /// [`Platform::present`] is a no-op (there is no surface to draw into),
    /// and [`Platform::wait_for_next_frame`] never sleeps (there is no real
    /// display to pace against) — a headless caller drives frames
    /// back-to-back instead.
    #[must_use]
    pub fn new_headless() -> Self {
        Self {
            backend: Backend::Null {
                buttons: ButtonState::new(),
            },
            pacer: FramePacer::new(),
        }
    }

    /// Pump pending OS/window events and advance the button state for this
    /// frame, mirroring upstream's per-vblank `heldKeys`/`newKeys` update.
    ///
    /// Non-blocking: returns immediately after draining whatever events are
    /// already pending. Returns `false` once the window has been asked to
    /// close (via the OS close control or the Escape key — see the module
    /// docs), at which point the caller should stop calling into this
    /// `Platform` and drop it. Always returns `true` for the null backend
    /// (see [`Platform::new_headless`]).
    ///
    /// # Errors
    ///
    /// Returns an error if window or presentation-surface creation failed.
    /// That failure happens asynchronously (once `winit` resumes the app),
    /// so it is only observable via this method's return value, typically
    /// on the first call. Never errors for the null backend.
    pub fn pump(&mut self) -> Result<bool, PlatformError> {
        match &mut self.backend {
            Backend::Window(window) => {
                let status = window
                    .event_loop
                    .pump_app_events(Some(Duration::ZERO), &mut window.app);
                if let Some(err) = window.app.init_error.take() {
                    return Err(err);
                }
                window.app.buttons.update(window.app.frame_held);
                Ok(!matches!(status, PumpStatus::Exit(_)))
            }
            Backend::Null { buttons } => {
                // No OS event source to drain, so the held set never
                // changes — but still folded through `ButtonState::update`
                // each call so `newly_pressed` behaves identically to the
                // windowed path (always empty here, since nothing is ever
                // held).
                buttons.update(Buttons::NONE);
                Ok(true)
            }
        }
    }

    /// The button state as of the most recent [`Platform::pump`] call.
    #[must_use]
    pub fn buttons(&self) -> &ButtonState {
        match &self.backend {
            Backend::Window(window) => &window.app.buttons,
            Backend::Null { buttons } => buttons,
        }
    }

    /// Block until it is time to present the next frame, paced to the GBA's
    /// real refresh cadence (see [`crate::pacing`]) rather than wall-clock
    /// 60 Hz — analogous to upstream's `WaitForVBlank`. A no-op for the null
    /// backend (see [`Platform::new_headless`]): there is no real display to
    /// pace against, so a headless caller drives frames back-to-back rather
    /// than sleeping between them.
    pub fn wait_for_next_frame(&mut self) {
        if matches!(self.backend, Backend::Null { .. }) {
            return;
        }
        let wait = self.pacer.tick(Instant::now());
        if !wait.is_zero() {
            std::thread::sleep(wait);
        }
    }

    /// Present a native 240x160 frame, integer-scaled and letterboxed to fit
    /// the current window size (see [`crate::present`]).
    ///
    /// A no-op if the window/surface has not been created yet (i.e. before
    /// the first successful [`Platform::pump`]) or the window is currently
    /// zero-sized (e.g. minimized) — and always a no-op for the null backend
    /// (see [`Platform::new_headless`]), which has no surface to draw into.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError::SoftBuffer`] if the presentation surface
    /// could not be resized or presented. Never errors for the null backend.
    pub fn present(&mut self, frame: &Frame) -> Result<(), PlatformError> {
        let Backend::Window(window) = &mut self.backend else {
            return Ok(());
        };
        let Some(inner) = window.app.inner.as_mut() else {
            return Ok(());
        };
        let size = inner.window.inner_size();
        let (Some(width), Some(height)) =
            (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
        else {
            return Ok(());
        };

        inner.surface.resize(width, height)?;
        let mut buffer = inner.surface.buffer_mut()?;
        let letterbox = Letterbox::compute(size.width, size.height);
        present::blit(frame, &letterbox, size.width, size.height, &mut buffer[..]);
        buffer.present()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::Platform;
    use crate::input::Buttons;
    use crate::present::test_pattern;

    #[test]
    fn headless_pump_always_reports_keep_going() {
        let mut platform = Platform::new_headless();
        for _ in 0..5 {
            assert!(platform.pump().expect("null backend never errors"));
        }
    }

    #[test]
    fn headless_buttons_start_and_stay_unheld() {
        let mut platform = Platform::new_headless();
        platform.pump().expect("null backend never errors");
        assert_eq!(platform.buttons().held(), Buttons::NONE);
        assert_eq!(platform.buttons().newly_pressed(), Buttons::NONE);
    }

    #[test]
    fn headless_present_accepts_a_frame_without_erroring() {
        let mut platform = Platform::new_headless();
        let frame = test_pattern();
        platform.present(&frame).expect("null backend never errors");
    }

    #[test]
    fn headless_wait_for_next_frame_never_blocks() {
        // Regression: if this ever started sleeping, a smoke run driving
        // many frames headlessly would slow down for no visible benefit
        // (there is no real display to pace against).
        let mut platform = Platform::new_headless();
        let start = std::time::Instant::now();
        for _ in 0..1000 {
            platform.wait_for_next_frame();
        }
        assert!(start.elapsed() < std::time::Duration::from_millis(50));
    }
}
