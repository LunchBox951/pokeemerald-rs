//! The production driver of [`Transport`]: the `cpal` host and the thread
//! that steps the transport against the wall clock.

use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait};

use crate::error::PlatformError;
use crate::ring::Producer;

use super::{OutputHost, Shared, Transport, TransportState};

/// The OS audio host. The host handle is rebuilt for every query so a
/// restarted audio server is picked up instead of a dead connection reused.
pub(in crate::audio) struct CpalHost;

impl OutputHost for CpalHost {
    type Device = cpal::Device;

    fn default_output(&mut self) -> Result<Option<(String, cpal::Device)>, cpal::Error> {
        let host = cpal::default_host();
        let Some(device) = host.default_output_device() else {
            return Ok(None);
        };
        // Only an absent default is `None`. A failed identity query (CoreAudio
        // reports a timed-out property read as `DeviceNotAvailable`) stays an
        // error: an attached stream ignores it, and an attempt still
        // classifies these kinds as `NoOutput`.
        let id = device.id()?;
        Ok(Some((id.to_string(), device)))
    }
}

/// The longest dropping a [`Worker`] waits for its driver to release the
/// stream. Shutdown is seen only between transport steps, and no OS audio
/// call can be interrupted, so a driver blocked in one is detached instead.
pub(in crate::audio) const SHUTDOWN_GRACE: Duration = Duration::from_millis(100);

/// The thread driving one [`Transport`]. Dropping it stops the driver and
/// joins it once it has released its stream, waiting at most
/// [`SHUTDOWN_GRACE`]; a driver still blocked in the audio stack by then is
/// detached and releases the stream itself when that call returns.
pub(in crate::audio) struct Worker {
    thread: Option<JoinHandle<()>>,
    shared: Arc<Shared>,
    /// Disconnected once the driver has dropped its transport and stream.
    released: Mutex<Receiver<()>>,
}

impl Worker {
    /// Start the driver thread; it connects without the caller waiting.
    pub(in crate::audio) fn spawn<H: OutputHost>(
        host: H,
        producer: Producer,
        shared: Arc<Shared>,
    ) -> Result<Self, PlatformError> {
        let driver_shared = Arc::clone(&shared);
        let (release, released) = mpsc::channel::<()>();
        let thread = thread::Builder::new()
            .name("audio-transport".into())
            .spawn(move || {
                // Declared first so it drops last, after the transport and
                // its stream.
                let _release = release;
                // Built here because a stream is not `Send` on every host.
                let origin = Instant::now();
                let mut transport = Transport::new(host, producer, Arc::clone(&driver_shared))
                    .with_clock(move || origin.elapsed());
                while !driver_shared.shutdown.load(Ordering::Acquire) {
                    let next = transport.step(origin.elapsed());
                    thread::park_timeout(next.saturating_sub(origin.elapsed()));
                }
            })
            .map_err(|err| {
                PlatformError::Audio(cpal::Error::with_message(
                    cpal::ErrorKind::ResourceExhausted,
                    format!("audio transport thread: {err}"),
                ))
            })?;
        Ok(Self {
            thread: Some(thread),
            shared,
            released: Mutex::new(released),
        })
    }

    /// Ask the driver to re-check intent now rather than at its next tick.
    pub(in crate::audio) fn wake(&self) {
        if let Some(thread) = &self.thread {
            thread.thread().unpark();
        }
    }

    pub(in crate::audio) fn poll(&self) -> Option<TransportState> {
        self.shared.pop_event()
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.shared.shutdown.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            thread.thread().unpark();
            let released = self
                .released
                .get_mut()
                .unwrap_or_else(PoisonError::into_inner);
            // Nothing is ever sent: the channel disconnects when the driver
            // has dropped its stream, or times out while it is blocked.
            if released.recv_timeout(SHUTDOWN_GRACE) == Err(RecvTimeoutError::Disconnected) {
                // A panicked driver has nothing left to clean up.
                let _ = thread.join();
            }
        }
    }
}
