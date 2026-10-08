//! Which asset pack this session's scene loads read from (issue #412): an
//! owned, explicit choice threaded from [`crate::App`] construction through
//! [`crate::flow::advance_scene`] into every scene load it performs, instead
//! of a process-wide `$POKEEMERALD_PACK` mutation.
//!
//! [`crate::App::new`] resolves the ordinary runtime order
//! ([`assets::AssetPack::load_default`]: `$POKEEMERALD_PACK`, then the OS
//! user-data directory, then the executable's directory, then the checkout's
//! own pack). [`crate::App::new_headless_real`] pins every one of those
//! loads to the checkout's own extracted pack
//! ([`assets::AssetPack::load_repo`]) instead: the scenario and e2e gates
//! that boot through it promise fixed inputs (`docs/scenarios.md`) and must
//! never validate an installed user pack, or one an inherited
//! `$POKEEMERALD_PACK` happens to name, that shadows the checkout's own.
//!
//! [`crate::App`] resolves this choice once, at construction, and carries it
//! as plain owned data (`crates/README.md`'s `no-global-mutable-state`
//! convention) rather than a process-wide override. What is pinned is the
//! source *choice* -- `Runtime` re-resolves the precedence rungs on each
//! load, so a pack published mid-session is picked up. The pinned choice
//! reaches every lazily-loaded scene, dialog, warp, and map-connection load the
//! title screen's own [`title::load_repo`](crate::title::load_repo)/
//! [`title::load_default`](crate::title::load_default) split already models
//! for the one scene [`crate::App::boot`] loads eagerly.

use std::path::PathBuf;

use assets::{AssetPack, PackError};

/// An explicit choice of where an [`AssetPack`] load reads from, carried by
/// [`crate::App`] and threaded through every scene load reachable after
/// construction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PackSource {
    /// [`AssetPack::load_default`]'s runtime resolver order.
    Runtime,
    /// [`AssetPack::load_repo`]: always this checkout's own extracted pack,
    /// regardless of environment or an installed user pack.
    Repo,
    /// A fixed path, for a test that must drive a real pack load through
    /// the production dialog-open path (`OverworldPhase::resolve_step_events`)
    /// without touching [`Self::Runtime`]'s or [`Self::Repo`]'s real
    /// resolved locations. The path must outlive the test: callers
    /// `Box::leak` it once.
    #[cfg(test)]
    Test(&'static std::path::Path),
    /// A caller-owned pack at a fixed path ([`crate::App::new_headless_real_at`]):
    /// a checkout-extracted pack or a ROM-imported one the caller placed
    /// itself. Never consults the environment or any other location, so a
    /// missing file is a load error, not a fallback. The path is an owned,
    /// cheaply cloneable `Arc`, so it is freed once the last clone (a failed
    /// boot's argument, or the dropped [`crate::App`]) is gone.
    Explicit(std::sync::Arc<std::path::Path>),
}

impl PackSource {
    /// The path this source resolves to -- [`AssetPack::default_path`] for
    /// [`Self::Runtime`], [`AssetPack::repo_pack_path`] for [`Self::Repo`],
    /// the carried path itself for [`Self::Test`]. Split out from
    /// [`Self::load`] so the resolution itself is checkable without a pack
    /// on disk (see this module's tests).
    #[must_use]
    fn path(&self) -> PathBuf {
        match self {
            Self::Runtime => AssetPack::default_path(),
            Self::Repo => AssetPack::repo_pack_path(),
            #[cfg(test)]
            Self::Test(path) => path.to_path_buf(),
            Self::Explicit(path) => path.to_path_buf(),
        }
    }

    /// Load the pack this source resolves to.
    ///
    /// # Errors
    ///
    /// See [`AssetPack::load`].
    pub(crate) fn load(&self) -> Result<AssetPack, PackError> {
        #[cfg(test)]
        PACK_LOADS.with(|loads| loads.set(loads.get() + 1));
        AssetPack::load(&self.path())
    }
}

#[cfg(test)]
thread_local! {
    /// How many times [`PackSource::load`] has run on this test thread.
    static PACK_LOADS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Test-only: [`PackSource::load`] calls on this thread so far, so a test
/// can pin how many full pack reads a code path performs. Per-thread, so
/// concurrently running tests never see each other's loads.
#[cfg(test)]
pub(crate) fn pack_loads_on_this_thread() -> usize {
    PACK_LOADS.with(std::cell::Cell::get)
}

#[cfg(test)]
mod tests {
    use super::PackSource;

    /// The seam itself, pack-free: each source must resolve through the
    /// path its own name promises, not silently share the other's -- the
    /// exact mistake that would leave a headless-real scenario reading an
    /// installed user pack, or an ordinary runtime boot reading the
    /// checkout's, again.
    #[test]
    fn each_source_resolves_through_its_own_named_path() {
        assert_eq!(
            PackSource::Runtime.path(),
            assets::AssetPack::default_path()
        );
        assert_eq!(PackSource::Repo.path(), assets::AssetPack::repo_pack_path());
    }

    /// [`PackSource::Repo`]'s whole point: unlike [`PackSource::Runtime`],
    /// its path can never be redirected by `$POKEEMERALD_PACK` or an
    /// installed user pack -- see [`assets::AssetPack::repo_pack_path`]'s
    /// own docs for why a checkout-validation gate asks for it by name
    /// rather than through the runtime resolver.
    #[test]
    fn repo_pins_to_the_checkout_path_pack_format_itself_names() {
        assert_eq!(PackSource::Repo.path(), pack_format::repo_pack_path());
    }

    /// An explicit source resolves to exactly the path it was given, never
    /// the runtime or checkout location.
    #[test]
    fn explicit_resolves_to_its_own_path_alone() {
        let path = std::path::Path::new("/nonexistent/explicit-source.pack");
        let source = PackSource::Explicit(path.into());
        assert_eq!(source.path(), path);
        assert_ne!(source.path(), PackSource::Repo.path());
        assert_ne!(source.path(), PackSource::Runtime.path());
    }

    /// A missing explicit pack is a load error, never a fallback to
    /// another source.
    #[test]
    fn explicit_missing_pack_fails_to_load() {
        let source =
            PackSource::Explicit(std::path::Path::new("/nonexistent/explicit-source.pack").into());
        assert!(source.load().is_err());
    }
}
