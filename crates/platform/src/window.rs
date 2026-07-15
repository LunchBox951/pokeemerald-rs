//! The [`Platform`] window type (S-1): owns the `winit` event loop, the
//! native window, and the `softbuffer` presentation surface, and glues them
//! together with [`crate::input`] and [`crate::pacing`] behind a small
//! per-frame API.
//!
//! Not unit tested directly — CI is headless and must never open a real
//! window. [`crate::input`], [`crate::pacing`], and [`crate::present`] carry
//! all the tested logic; this module is thin glue over `winit`/`softbuffer`
//! plus a manual event pump (see [`Platform::pump`]) so a caller stays in
//! control of its own frame loop rather than handing control to `winit`.

use std::num::NonZeroU32;
use std::rc::Rc;
use std::time::{Duration, Instant};

use softbuffer::{Context, Surface};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::PhysicalKey;
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
struct App {
    title: String,
    keymap: Keymap,
    frame_held: Buttons,
    buttons: ButtonState,
    inner: Option<Inner>,
    init_error: Option<PlatformError>,
}

impl App {
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

impl ApplicationHandler for App {
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

/// An open native window: input, frame pacing, and `softbuffer`
/// presentation, exposed as a small per-frame API.
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
    event_loop: EventLoop<()>,
    app: App,
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
            event_loop: EventLoop::new()?,
            app: App::new(title.into()),
            pacer: FramePacer::new(),
        })
    }

    /// Pump pending OS/window events and advance the button state for this
    /// frame, mirroring upstream's per-vblank `heldKeys`/`newKeys` update.
    ///
    /// Non-blocking: returns immediately after draining whatever events are
    /// already pending. Returns `false` once the window has been asked to
    /// close, at which point the caller should stop calling into this
    /// `Platform` and drop it.
    ///
    /// # Errors
    ///
    /// Returns an error if window or presentation-surface creation failed.
    /// That failure happens asynchronously (once `winit` resumes the app),
    /// so it is only observable via this method's return value, typically
    /// on the first call.
    pub fn pump(&mut self) -> Result<bool, PlatformError> {
        let status = self
            .event_loop
            .pump_app_events(Some(Duration::ZERO), &mut self.app);
        if let Some(err) = self.app.init_error.take() {
            return Err(err);
        }
        self.app.buttons.update(self.app.frame_held);
        Ok(!matches!(status, PumpStatus::Exit(_)))
    }

    /// The button state as of the most recent [`Platform::pump`] call.
    #[must_use]
    pub fn buttons(&self) -> &ButtonState {
        &self.app.buttons
    }

    /// Block until it is time to present the next frame, paced to the GBA's
    /// real refresh cadence (see [`crate::pacing`]) rather than wall-clock
    /// 60 Hz — analogous to upstream's `WaitForVBlank`.
    pub fn wait_for_next_frame(&mut self) {
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
    /// zero-sized (e.g. minimized).
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError::SoftBuffer`] if the presentation surface
    /// could not be resized or presented.
    pub fn present(&mut self, frame: &Frame) -> Result<(), PlatformError> {
        let Some(inner) = self.app.inner.as_mut() else {
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
