//! The fixture's git helper must not follow repository-routing variables the
//! test process inherited (a git hook running `cargo test` sets `GIT_DIR` and
//! `GIT_INDEX_FILE`).

use super::*;

/// A disposable repository holding one tracked file, with its state captured.
struct Sentinel {
    root: PathBuf,
}

impl Sentinel {
    fn new(name: &str) -> Self {
        let root = temp_dir(name);
        git(&root, &["init", "-q"]);
        fs::write(root.join("tracked"), name).unwrap();
        git(&root, &["add", "tracked"]);
        git(&root, &["commit", "-q", "-m", name]);
        Self { root }
    }

    fn git_dir(&self) -> PathBuf {
        self.root.join(".git")
    }

    fn index(&self) -> PathBuf {
        self.git_dir().join("index")
    }

    /// HEAD, the full ref listing and the raw index bytes.
    fn state(&self) -> (String, String, Vec<u8>) {
        (
            git(&self.root, &["rev-parse", "HEAD"]),
            git(&self.root, &["for-each-ref"]),
            fs::read(self.index()).unwrap(),
        )
    }
}

impl Drop for Sentinel {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// Re-runs one fixture test in a child that inherits `GIT_DIR` and
/// `GIT_INDEX_FILE` naming the given sentinels, and checks the child passes
/// and left both sentinels untouched.
fn fixture_test_ignores_inherited_routing(tag: &str, test: &str) {
    let dir_sentinel = Sentinel::new(&format!("{tag}-git-dir"));
    let index_sentinel = Sentinel::new(&format!("{tag}-git-index"));
    let (dir_before, index_before) = (dir_sentinel.state(), index_sentinel.state());
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", test, "--test-threads=1"])
        .env("GIT_DIR", dir_sentinel.git_dir())
        .env("GIT_INDEX_FILE", index_sentinel.index())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success() && stdout.contains("1 passed"),
        "child failed under inherited GIT_DIR/GIT_INDEX_FILE:\n{stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(dir_sentinel.state(), dir_before, "GIT_DIR repository");
    assert_eq!(
        index_sentinel.state(),
        index_before,
        "GIT_INDEX_FILE repository"
    );
}

#[test]
#[cfg(unix)]
fn a_good_build_ignores_an_inherited_git_dir_and_index_file() {
    fixture_test_ignores_inherited_routing(
        "env-good",
        "rom_build::tests::a_good_build_reports_bytes_symbols_and_provenance_in_isolation",
    );
}

#[test]
#[cfg(unix)]
fn a_mismatched_reference_under_inherited_routing_leaves_the_sentinels_alone() {
    fixture_test_ignores_inherited_routing(
        "env-mismatch",
        "rom_build::tests::a_reference_at_another_revision_fails_before_any_build",
    );
}
