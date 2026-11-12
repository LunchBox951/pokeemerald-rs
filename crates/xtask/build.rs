//! Records the Cargo profile this binary was built with, so `e2e --release`
//! can tell a release build from a debug one (F-3, V-1). Debug assertions
//! and target-directory names cannot: both are configurable per profile.
//!
//! It also records the source identity of the checkout being compiled, so
//! `record-snapshot` can name the source its pixels came from rather than
//! whatever the checkout holds when the binary later runs.

#[path = "src/record_snapshot/source_identity.rs"]
mod source_identity;

use std::path::{Path, PathBuf};

fn main() {
    println!("cargo::rerun-if-env-changed=PROFILE");
    let profile = std::env::var("PROFILE").unwrap_or_default();
    println!("cargo::rustc-env=XTASK_BUILD_PROFILE={profile}");

    // Commits, edits, and index flags all change the identity, and no short list
    // of watched paths covers them. A path that never exists makes Cargo rerun
    // this script, and so rebuild the binary, on every invocation.
    let out_dir = std::env::var_os("OUT_DIR").map_or_else(std::env::temp_dir, PathBuf::from);
    println!(
        "cargo::rerun-if-changed={}",
        out_dir.join("always-rerun-source-identity").display()
    );

    let identity = checkout_root()
        .and_then(|root| source_identity::git_sha(&root))
        .unwrap_or_else(|| "unknown".to_owned());
    println!("cargo::rustc-env=XTASK_SOURCE_IDENTITY={identity}");
}

/// The workspace root, two levels above this crate's manifest.
fn checkout_root() -> Option<PathBuf> {
    let manifest = std::env::var_os("CARGO_MANIFEST_DIR")?;
    Path::new(&manifest)
        .parent()?
        .parent()
        .map(Path::to_path_buf)
}
