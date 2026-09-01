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
//! convention) rather than a process-wide override -- the same pin, reaching
//! every lazily-loaded scene, dialog, warp, and map-connection load the
//! title screen's own [`title::load_repo`](crate::title::load_repo)/
//! [`title::load_default`](crate::title::load_default) split already models
//! for the one scene [`crate::App::boot`] loads eagerly.

use std::path::PathBuf;

use assets::{AssetPack, PackError};

/// An explicit choice of where an [`AssetPack`] load reads from, carried by
/// [`crate::App`] and threaded through every scene load reachable after
/// construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PackSource {
    /// [`AssetPack::load_default`]'s runtime resolver order.
    Runtime,
    /// [`AssetPack::load_repo`]: always this checkout's own extracted pack,
    /// regardless of environment or an installed user pack.
    Repo,
}

impl PackSource {
    /// The path this source resolves to -- [`AssetPack::default_path`] for
    /// [`Self::Runtime`], [`AssetPack::repo_pack_path`] for [`Self::Repo`].
    /// Split out from [`Self::load`] so the resolution itself is checkable
    /// without a pack on disk (see this module's tests).
    #[must_use]
    fn path(self) -> PathBuf {
        match self {
            Self::Runtime => AssetPack::default_path(),
            Self::Repo => AssetPack::repo_pack_path(),
        }
    }

    /// Load the pack this source resolves to.
    ///
    /// # Errors
    ///
    /// See [`AssetPack::load`].
    pub(crate) fn load(self) -> Result<AssetPack, PackError> {
        AssetPack::load(&self.path())
    }
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
}
