//! The production driver of [`Transport`]: the `cpal` host and the thread
//! that steps the transport against the wall clock.

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Instant;

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
        match device.id() {
            Ok(id) => Ok(Some((id.to_string(), device))),
            Err(err)
                if matches!(
                    err.kind(),
                    cpal::ErrorKind::DeviceNotAvailable | cpal::ErrorKind::HostUnavailable
                ) =>
            {
                Ok(None)
            }
            Err(err) => Err(err),
        }
    }
}

/// The thread driving one [`Transport`], joined on drop.
pub(in crate::audio) struct Worker {
    thread: Option<JoinHandle<()>>,
    shared: Arc<Shared>,
}

impl Worker {
    /// Start the driver thread; it connects without the caller waiting.
    pub(in crate::audio) fn spawn<H: OutputHost>(
        host: H,
        producer: Producer,
        shared: Arc<Shared>,
    ) -> Result<Self, PlatformError> {
        let driver_shared = Arc::clone(&shared);
        let thread = thread::Builder::new()
            .name("audio-transport".into())
            .spawn(move || {
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
            // A panicked driver has nothing left to clean up.
            let _ = thread.join();
        }
    }
}
