//! Synthetic failure-class tests: a throwaway git repository stands in for the
//! reference checkout and a shell script stands in for `make`, so no
//! cross-compiler is needed.

use super::*;

use std::fs;
use std::process::Command;

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "pokeemerald-rs-rom-build-test-{name}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@example.invalid"])
        .args(["-c", "commit.gpgsign=false", "-C"])
        .arg(dir)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}

struct Fixture {
    root: PathBuf,
    config: BuildConfig,
}

impl Fixture {
    /// A one-commit reference repo whose `rom.sha1` names [`GOOD`]'s image,
    /// and a fake `make` running `script`.
    fn new(name: &str, script: &str) -> Self {
        Self::with_checksums(name, script, GOOD_SHA1)
    }

    /// As [`Fixture::new`], with `listing` as the reference's `rom.sha1`.
    fn with_checksums(name: &str, script: &str, listing: &str) -> Self {
        let root = temp_dir(name);
        let reference = root.join("reference");
        fs::create_dir(&reference).unwrap();
        git(&reference, &["init", "-q"]);
        fs::write(reference.join("Makefile"), "all:\n").unwrap();
        fs::write(reference.join(CHECKSUM_FILE), listing).unwrap();
        git(&reference, &["add", "."]);
        git(&reference, &["commit", "-q", "-m", "pin"]);
        let pin = git(&reference, &["rev-parse", "HEAD"]);
        let make = root.join("fake-make.sh");
        fs::write(&make, format!("#!/bin/sh\n{script}\n")).unwrap();
        #[cfg(unix)]
        fs::set_permissions(&make, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        let scratch_root = root.join("scratch");
        fs::create_dir(&scratch_root).unwrap();
        let agbcc_source = root.join("agbcc-install");
        fs::create_dir_all(agbcc_source.join("bin")).unwrap();
        fs::write(agbcc_source.join("bin/agbcc"), "cc1").unwrap();
        let config = BuildConfig {
            reference,
            agbcc_source,
            pinned_revision: pin,
            scratch_root,
            make: make.clone().into_os_string(),
            required_tools: vec![make.into_os_string()],
            jobs: 1,
        };
        Self { root, config }
    }

    fn scratch_entries(&self) -> usize {
        fs::read_dir(&self.config.scratch_root).unwrap().count()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

const GOOD: &str =
    "printf 'ROMBYTES' > pokeemerald.gba\nprintf '  0x08000000  gStart\\n' > pokeemerald.map";
/// `rom.sha1` for [`GOOD`]'s image, in `sha1sum` format.
const GOOD_SHA1: &str = "0da67421f828e56a9e6e698f79acc5dad60ad3ee  pokeemerald.gba\n";

#[test]
#[cfg(unix)]
fn a_good_build_reports_bytes_symbols_and_provenance_in_isolation() {
    let fx = Fixture::new("good", GOOD);
    let artifact = build(&fx.config).unwrap();
    assert_eq!(artifact.rom(), b"ROMBYTES");
    assert_eq!(artifact.symbols().symbols_at(0x0800_0000), ["gStart"]);
    assert_eq!(
        artifact.provenance().pinned_revision,
        fx.config.pinned_revision
    );
    assert!(artifact.provenance().build_command.ends_with("-j1"));
    assert_eq!(fx.scratch_entries(), 0, "scratch tree is removed");
    assert!(!fx.config.reference.join("pokeemerald.gba").exists());
    assert_eq!(git(&fx.config.reference, &["status", "--porcelain"]), "");
    assert_eq!(
        git(&fx.config.reference, &["rev-parse", "HEAD"]),
        fx.config.pinned_revision
    );
}

#[test]
#[cfg(unix)]
fn the_artifact_debug_form_omits_the_image() {
    let fx = Fixture::new("debug", GOOD);
    let text = format!("{:?}", build(&fx.config).unwrap());
    assert!(text.contains("rom_len: 8"));
    assert!(!text.contains("ROMBYTES"));
}

#[test]
#[cfg(unix)]
fn a_reference_at_another_revision_fails_before_any_build() {
    let marker = temp_dir("mismatch-marker").join("ran");
    let script = format!("touch '{}'\n{GOOD}", marker.display());
    let mut fx = Fixture::new("mismatch", &script);
    fx.config.pinned_revision = "0".repeat(40);
    let err = build(&fx.config).unwrap_err();
    assert!(matches!(err, RomBuildError::PinMismatch { .. }), "{err}");
    assert!(!marker.exists(), "the build never ran");
    assert_eq!(fx.scratch_entries(), 0);
}

#[test]
#[cfg(unix)]
fn an_untracked_file_in_the_reference_is_a_dirty_tree() {
    let fx = Fixture::new("dirty", GOOD);
    fs::write(fx.config.reference.join("stray.txt"), "x").unwrap();
    let err = build(&fx.config).unwrap_err();
    assert!(matches!(err, RomBuildError::DirtyTree { .. }), "{err}");
}

#[test]
#[cfg(unix)]
fn an_edited_tracked_file_in_the_reference_is_a_dirty_tree() {
    let fx = Fixture::new("edited", GOOD);
    fs::write(fx.config.reference.join("Makefile"), "all:\n\ttrue\n").unwrap();
    let err = build(&fx.config).unwrap_err();
    assert!(matches!(err, RomBuildError::DirtyTree { .. }), "{err}");
}

#[test]
#[cfg(unix)]
fn a_reference_without_its_own_git_metadata_is_unverifiable() {
    let fx = Fixture::new("tarball", GOOD);
    let plain = fx.root.join("plain");
    fs::create_dir(&plain).unwrap();
    // Nested inside a git repository, as a tarball under the product repo is.
    let nested = fx.config.reference.join("nested");
    fs::create_dir(&nested).unwrap();
    for dir in [plain, nested] {
        let config = BuildConfig {
            reference: dir,
            ..fx.config.clone()
        };
        let err = build(&config).unwrap_err();
        assert!(matches!(err, RomBuildError::Unverifiable { .. }), "{err}");
    }
}

#[test]
fn a_missing_reference_is_reported() {
    let fx = Fixture::new("absent", GOOD);
    let config = BuildConfig {
        reference: fx.root.join("nope"),
        ..fx.config.clone()
    };
    assert!(matches!(
        build(&config).unwrap_err(),
        RomBuildError::MissingReference(_)
    ));
}

#[test]
#[cfg(unix)]
fn a_missing_tool_fails_before_materialising() {
    let mut fx = Fixture::new("tool", GOOD);
    fx.config
        .required_tools
        .push(OsString::from("no-such-arm-none-eabi-tool"));
    let err = build(&fx.config).unwrap_err();
    assert!(
        matches!(&err, RomBuildError::MissingTool(t) if t == "no-such-arm-none-eabi-tool"),
        "{err}"
    );
    assert_eq!(fx.scratch_entries(), 0);
}

#[test]
#[cfg(unix)]
fn a_failing_build_reports_its_stderr_and_cleans_up() {
    let fx = Fixture::new("fail", "echo 'boom: no agbcc' >&2\nexit 3");
    let err = build(&fx.config).unwrap_err();
    assert!(
        matches!(&err, RomBuildError::BuildFailed { reason } if reason.contains("boom: no agbcc")),
        "{err}"
    );
    assert_eq!(fx.scratch_entries(), 0);
}

#[test]
#[cfg(unix)]
fn a_build_that_emits_nothing_is_a_missing_output() {
    let fx = Fixture::new("nothing", "true");
    assert!(matches!(
        build(&fx.config).unwrap_err(),
        RomBuildError::MissingOutput(f) if f == ROM_FILE
    ));
}

#[test]
#[cfg(unix)]
fn a_build_without_a_map_is_a_missing_output() {
    let fx = Fixture::new("nomap", "printf 'ROMBYTES' > pokeemerald.gba");
    assert!(matches!(
        build(&fx.config).unwrap_err(),
        RomBuildError::MissingOutput(f) if f == MAP_FILE
    ));
}

#[test]
#[cfg(unix)]
fn an_empty_image_or_symbolless_map_is_invalid() {
    let fx = Fixture::new(
        "emptyrom",
        ": > pokeemerald.gba\nprintf '  0x08000000  gStart\\n' > pokeemerald.map",
    );
    assert!(matches!(
        build(&fx.config).unwrap_err(),
        RomBuildError::InvalidOutput { file, .. } if file == ROM_FILE
    ));
    let fx = Fixture::new(
        "nosyms",
        "printf 'ROMBYTES' > pokeemerald.gba\nprintf 'nothing\\n' > pokeemerald.map",
    );
    assert!(matches!(
        build(&fx.config).unwrap_err(),
        RomBuildError::InvalidOutput { file, .. } if file == MAP_FILE
    ));
}

#[test]
fn the_pin_is_read_from_init_sh() {
    let dir = temp_dir("pin");
    let sha = "83df84e40623b79281f2397faa611cbf044170bd";
    let init = dir.join("init.sh");
    fs::write(
        &init,
        format!("A=1\nPOKEEMERALD_REF=\"${{POKEEMERALD_REF:-{sha}}}\"\n"),
    )
    .unwrap();
    assert_eq!(checkout::read_pin(&init).unwrap(), sha);
    fs::write(&init, "POKEEMERALD_REF=\"${POKEEMERALD_REF:-notasha}\"\n").unwrap();
    assert!(matches!(
        checkout::read_pin(&init).unwrap_err(),
        RomBuildError::PinUnreadable { .. }
    ));
    assert!(checkout::read_pin(&dir.join("absent")).is_err());
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn the_repository_init_script_carries_a_pin() {
    let pin = checkout::read_pin(&crate::extract::repo_root().join("init.sh")).unwrap();
    assert_eq!(pin.len(), 40);
}

#[test]
#[cfg(feature = "rom-build-real")]
#[ignore = "needs a local `./init.sh` checkout, the arm-none-eabi toolchain, and POKEEMERALD_AGBCC"]
fn the_real_pinned_checkout_builds() {
    let artifact = run().unwrap_or_else(|err| panic!("{err}"));
    assert_eq!(
        artifact.provenance().pinned_revision,
        checkout::read_pin(&crate::extract::repo_root().join("init.sh")).unwrap()
    );
    assert!(!artifact.rom().is_empty());
    assert!(!artifact.symbols().is_empty());
}

#[test]
#[cfg(unix)]
fn agbcc_is_provisioned_into_the_scratch_tree_only() {
    let fx = Fixture::new(
        "agbcc",
        &format!("test -f tools/agbcc/bin/agbcc || exit 9\n{GOOD}"),
    );
    build(&fx.config).unwrap();
    assert!(!fx.config.reference.join("tools").exists());
}

#[test]
#[cfg(unix)]
fn a_missing_agbcc_install_is_a_missing_tool() {
    let mut fx = Fixture::new("noagbcc", GOOD);
    fx.config.agbcc_source = fx.root.join("absent");
    assert!(matches!(
        build(&fx.config).unwrap_err(),
        RomBuildError::MissingTool(t) if t.starts_with("agbcc")
    ));
    assert_eq!(fx.scratch_entries(), 0);
}

#[test]
fn the_devkitarm_bin_directory_is_searched_like_upstream() {
    let dir = temp_dir("devkit");
    fs::write(dir.join("arm-none-eabi-ld"), "").unwrap();
    let tool = OsString::from("arm-none-eabi-ld");
    let search = std::slice::from_ref(&dir);
    assert!(!on_path(&tool, &[], &[]));
    assert!(on_path(&tool, search, &[]));
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn a_suffixed_executable_resolves_only_with_its_suffix_listed() {
    let dir = temp_dir("suffix");
    fs::write(dir.join("make.exe"), "").unwrap();
    let tool = OsString::from("make");
    let search = std::slice::from_ref(&dir);
    assert!(!on_path(&tool, search, &[]));
    assert!(on_path(&tool, search, &[".exe".to_owned()]));
    assert!(on_path(
        &dir.join("make").into_os_string(),
        &[],
        &[".exe".to_owned()]
    ));
    assert_eq!(executable_suffixes().is_empty(), !cfg!(windows));
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn a_hidden_tracked_edit_in_the_reference_is_rejected() {
    for flag in ["--assume-unchanged", "--skip-worktree"] {
        let fx = Fixture::new("hidden-index", GOOD);
        git(&fx.config.reference, &["update-index", flag, "Makefile"]);
        fs::write(fx.config.reference.join("Makefile"), "all:\n\ttrue\n").unwrap();
        let err = build(&fx.config).unwrap_err();
        assert!(
            matches!(err, RomBuildError::HiddenIndexState { .. }),
            "{flag}: {err}"
        );
        assert_eq!(fx.scratch_entries(), 0);
    }
}

#[test]
#[cfg(unix)]
fn an_image_that_differs_from_the_pinned_checksum_is_rejected() {
    let fx = Fixture::new(
        "checksum",
        "printf 'OTHERROM' > pokeemerald.gba\nprintf '  0x08000000  gStart\\n' > pokeemerald.map",
    );
    let err = build(&fx.config).unwrap_err();
    assert!(
        matches!(&err, RomBuildError::ChecksumMismatch { expected, .. }
            if expected == "0da67421f828e56a9e6e698f79acc5dad60ad3ee"),
        "{err}"
    );
    assert_eq!(fx.scratch_entries(), 0);
}

#[test]
#[cfg(unix)]
fn a_checksum_listing_without_the_image_fails_closed() {
    let fx = Fixture::with_checksums(
        "checksum-unlisted",
        GOOD,
        "0da67421f828e56a9e6e698f79acc5dad60ad3ee  other.gba\n",
    );
    let err = build(&fx.config).unwrap_err();
    assert!(matches!(err, RomBuildError::Materialize(_)), "{err}");
}

#[test]
#[cfg(unix)]
fn an_oversized_image_is_rejected_by_its_size() {
    let fx = Fixture::new(
        "oversized",
        &format!(
            "truncate -s {} pokeemerald.gba\nprintf '  0x08000000  gStart\\n' > pokeemerald.map",
            MAX_ROM_BYTES + 1
        ),
    );
    let err = build(&fx.config).unwrap_err();
    assert!(
        matches!(&err, RomBuildError::InvalidOutput { file, reason }
            if *file == ROM_FILE && reason.starts_with(&format!("{} bytes", MAX_ROM_BYTES + 1))),
        "{err}"
    );
}

#[test]
#[cfg(unix)]
fn an_oversized_linker_map_is_rejected_before_it_is_read() {
    let fx = Fixture::new(
        "oversized-map",
        &format!(
            "printf 'ROMBYTES' > pokeemerald.gba\ntruncate -s {} pokeemerald.map",
            MAX_MAP_BYTES + 1
        ),
    );
    let err = build(&fx.config).unwrap_err();
    assert!(
        matches!(&err, RomBuildError::InvalidOutput { file, reason }
            if *file == MAP_FILE && reason.starts_with(&format!("{} bytes", MAX_MAP_BYTES + 1))),
        "{err}"
    );
}

#[test]
fn an_unbounded_image_stream_is_read_only_past_the_cartridge_size() {
    let bytes = read_bounded(std::io::repeat(0xFF), MAX_ROM_BYTES as u64).unwrap();
    assert_eq!(bytes.len(), MAX_ROM_BYTES + 1);
}

#[test]
fn an_unbounded_map_stream_is_read_only_past_the_map_bound() {
    let bytes = read_bounded(std::io::repeat(b'x'), MAX_MAP_BYTES).unwrap();
    assert_eq!(bytes.len() as u64, MAX_MAP_BYTES + 1);
}

#[test]
fn map_text_parses_through_the_symbol_map_and_needs_a_symbol() {
    let symbols = parse_map(b"  0x08000000  gStart\n".to_vec()).unwrap();
    assert_eq!(symbols.symbols_at(0x0800_0000), ["gStart"]);
    for bad in [&b"nothing\n"[..], &[0xFF, 0xFE][..]] {
        assert!(matches!(
            parse_map(bad.to_vec()).unwrap_err(),
            RomBuildError::InvalidOutput { file, .. } if file == MAP_FILE
        ));
    }
}

fn env_pairs(pairs: &[(&str, &str)]) -> Vec<(OsString, OsString)> {
    pairs
        .iter()
        .map(|(name, value)| (OsString::from(name), OsString::from(value)))
        .collect()
}

#[test]
fn the_build_inherits_the_windows_os_marker() {
    let lookup = |name: &str| (name == "OS").then(|| OsString::from("Windows_NT"));
    assert_eq!(
        inherited_env(lookup, []),
        env_pairs(&[("OS", "Windows_NT")])
    );
    assert!(inherited_env(|name| (name == "SECRET").then(OsString::new), []).is_empty());
}

#[test]
fn the_build_inherits_the_nix_shell_build_inputs() {
    let vars = env_pairs(&[
        ("NIX_CFLAGS_COMPILE", "-isystem /nix/store/x-libpng/include"),
        ("NIX_LDFLAGS", "-L/nix/store/x-libpng/lib"),
        ("NIX_CC_WRAPPER_TARGET_HOST_x86_64_unknown_linux_gnu", "1"),
        ("PKG_CONFIG_PATH", "/nix/store/x-libpng/lib/pkgconfig"),
        ("SECRET_TOKEN", "x"),
        ("CFLAGS", "-O0"),
    ]);
    let inherited = inherited_env(|_| None, vars.clone());
    assert_eq!(inherited, vars[..4]);
}

#[test]
#[cfg(unix)]
fn a_build_that_rewrites_rom_sha1_is_still_checked_against_the_pin() {
    let fx = Fixture::new(
        "checksum-rewrite",
        "printf 'OTHERROM' > pokeemerald.gba\n\
         printf '968036cea1fec0a66b60dd756c99a3bec1d6aab1  pokeemerald.gba\\n' > rom.sha1\n\
         printf '  0x08000000  gStart\\n' > pokeemerald.map",
    );
    let err = build(&fx.config).unwrap_err();
    assert!(
        matches!(&err, RomBuildError::ChecksumMismatch { expected, .. }
            if expected == "0da67421f828e56a9e6e698f79acc5dad60ad3ee"),
        "{err}"
    );
}

#[test]
#[cfg(unix)]
fn a_flooding_failed_build_reports_only_its_stderr_tail() {
    let fx = Fixture::new(
        "flood",
        "i=0\n\
         while [ $i -lt 20000 ]; do\n\
           echo \"stdout-noise-$i\"\n\
           echo \"stderr-line-$i\" >&2\n\
           i=$((i + 1))\n\
         done\n\
         echo 'final: boom' >&2\n\
         exit 4",
    );
    let err = build(&fx.config).unwrap_err();
    let RomBuildError::BuildFailed { reason } = &err else {
        panic!("{err}");
    };
    let (_, tail) = reason.split_once("exit status: 4: ").expect("exit status");
    assert!(tail.ends_with("final: boom\n"), "{tail:?}");
    assert!(tail.contains("stderr-line-19999"), "{tail:?}");
    assert!(!tail.contains("stderr-line-0\n"), "{tail:?}");
    assert!(!tail.contains("stdout-noise"), "{tail:?}");
    assert_eq!(tail.chars().count(), STDERR_TAIL_BYTES + 1);
    assert_eq!(fx.scratch_entries(), 0);
}

#[test]
fn the_stderr_tail_is_bounded_and_keeps_the_last_bytes() {
    let flood = std::io::repeat(b'x').take(10_000_000).chain(&b"END"[..]);
    let tail = Mutex::new(VecDeque::new());
    drain_tail(flood, &tail).unwrap();
    let tail: Vec<u8> = tail.into_inner().unwrap().into();
    assert_eq!(tail.len(), STDERR_KEEP_BYTES);
    assert_eq!(STDERR_KEEP_BYTES, 4 * (STDERR_TAIL_BYTES + 2));
    assert!(tail.ends_with(b"xEND"));
    let short = Mutex::new(VecDeque::new());
    drain_tail(&b"short"[..], &short).unwrap();
    assert_eq!(Vec::from(short.into_inner().unwrap()), b"short");
}

#[test]
#[cfg(unix)]
fn a_descendant_holding_stderr_open_does_not_stall_the_failed_build() {
    let fx = Fixture::new("held-stderr", "sleep 30 >&2 &\necho boom >&2\nexit 1");
    let started = std::time::Instant::now();
    let err = build(&fx.config).unwrap_err();
    let elapsed = started.elapsed();
    assert!(
        matches!(&err, RomBuildError::BuildFailed { reason } if reason.contains("boom")),
        "{err}"
    );
    assert!(elapsed < std::time::Duration::from_secs(10), "{elapsed:?}");
}

#[test]
#[cfg(unix)]
fn a_huge_untracked_listing_is_reported_truncated() {
    let fx = Fixture::new("many-untracked", GOOD);
    let stray = fx.config.reference.join("stray");
    fs::create_dir(&stray).unwrap();
    for i in 0..5000 {
        fs::write(stray.join(format!("f{i}")), "x").unwrap();
    }
    let err = build(&fx.config).unwrap_err();
    let RomBuildError::DirtyTree { status, .. } = &err else {
        panic!("{err}");
    };
    let lines: Vec<&str> = status.lines().collect();
    assert_eq!(lines.len(), 21, "{status}");
    assert!(lines[..20]
        .iter()
        .all(|line| line.starts_with("?? stray/f")));
    assert_eq!(lines[20], "... and more");
    assert_eq!(fx.scratch_entries(), 0);
}

#[test]
#[cfg(unix)]
fn a_few_untracked_files_are_listed_in_full() {
    let fx = Fixture::new("few-untracked", GOOD);
    for name in ["a.txt", "b.txt"] {
        fs::write(fx.config.reference.join(name), "x").unwrap();
    }
    let err = build(&fx.config).unwrap_err();
    assert!(
        matches!(&err, RomBuildError::DirtyTree { status, .. }
            if status == "?? a.txt\n?? b.txt"),
        "{err}"
    );
}

#[test]
#[cfg(unix)]
fn a_fifo_output_is_refused_without_blocking() {
    let cases = [
        (
            "fifo-rom",
            "mkfifo pokeemerald.gba\nprintf '  0x08000000  gStart\\n' > pokeemerald.map",
            ROM_FILE,
        ),
        (
            "fifo-map",
            "printf 'ROMBYTES' > pokeemerald.gba\nmkfifo pokeemerald.map",
            MAP_FILE,
        ),
    ];
    for (name, script, output) in cases {
        let fx = Fixture::new(name, script);
        let started = std::time::Instant::now();
        let err = build(&fx.config).unwrap_err();
        assert!(
            matches!(&err, RomBuildError::InvalidOutput { file, reason }
                if *file == output && reason.starts_with("not a regular file")),
            "{name}: {err}"
        );
        assert!(started.elapsed() < std::time::Duration::from_secs(10));
    }
}

#[test]
#[cfg(unix)]
fn a_symlinked_output_is_refused() {
    let fx = Fixture::new(
        "symlink-rom",
        "printf 'ROMBYTES' > real.gba\nln -s real.gba pokeemerald.gba\n\
         printf '  0x08000000  gStart\\n' > pokeemerald.map",
    );
    let err = build(&fx.config).unwrap_err();
    assert!(
        matches!(&err, RomBuildError::InvalidOutput { file, .. } if *file == ROM_FILE),
        "{err}"
    );
}

thread_local! {
    /// Largest single allocation made on this thread since it was last reset.
    static MAX_ALLOC: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

struct TrackingAlloc;

// SAFETY: every call forwards unchanged to the system allocator, which upholds
// the `GlobalAlloc` contract; the added thread-local counter never allocates.
unsafe impl std::alloc::GlobalAlloc for TrackingAlloc {
    unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
        MAX_ALLOC.with(|m| m.set(m.get().max(layout.size())));
        // SAFETY: same layout the caller passed, caller's contract applies.
        unsafe { std::alloc::System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: std::alloc::Layout) {
        // SAFETY: `ptr`/`layout` come from a matching `alloc` above.
        unsafe { std::alloc::System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: std::alloc::Layout, new: usize) -> *mut u8 {
        MAX_ALLOC.with(|m| m.set(m.get().max(new)));
        // SAFETY: `ptr`/`layout` come from a matching `alloc`; `new` is the caller's.
        unsafe { std::alloc::System.realloc(ptr, layout, new) }
    }
}

#[global_allocator]
static ALLOC: TrackingAlloc = TrackingAlloc;

/// Endless run of non-newline bytes, `left` long.
struct Endless {
    left: u64,
}

impl std::io::Read for Endless {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        let n = usize::try_from(self.left).map_or(out.len(), |left| out.len().min(left));
        out[..n].fill(b'x');
        self.left -= n as u64;
        Ok(n)
    }
}

#[test]
fn overlong_status_line_is_discarded_through_a_bounded_buffer() {
    use std::io::{BufReader, Read};
    // 64 MiB of line with no newline, then a newline-terminated tail.
    let tail = std::io::Cursor::new(b"\nnext\n".to_vec());
    let mut reader = BufReader::with_capacity(4096, Endless { left: 64 << 20 }.chain(tail));
    MAX_ALLOC.with(|m| m.set(0));
    let (lines, more, error) = checkout::read_head(&mut reader, 8);
    let largest = MAX_ALLOC.with(std::cell::Cell::get);
    assert!(error.is_none(), "{error:?}");
    assert!(!more);
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0], "x".repeat(8192));
    assert_eq!(lines[1], "next");
    // An unbounded `read_until` into a Vec would allocate the whole 64 MiB.
    assert!(
        largest <= 64 << 10,
        "discarding allocated {largest} bytes at once"
    );
}

#[test]
fn read_head_reports_a_read_error_while_discarding_an_overlong_line() {
    struct Flaky {
        stage: u8,
        tail: &'static [u8],
        buf: Vec<u8>,
    }
    impl std::io::Read for Flaky {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            unreachable!("read_head reads through fill_buf")
        }
    }
    impl std::io::BufRead for Flaky {
        fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
            match self.stage {
                0 => Ok(&self.buf),
                1 => {
                    self.stage = 2;
                    Err(std::io::Error::other("transient"))
                }
                _ => Ok(self.tail),
            }
        }
        fn consume(&mut self, n: usize) {
            if self.stage == 0 {
                self.buf.drain(..n);
                if self.buf.is_empty() {
                    self.stage = 1;
                }
            } else {
                self.tail = &self.tail[n..];
            }
        }
    }

    let mut reader = Flaky {
        stage: 0,
        tail: b"tail\n",
        buf: vec![b'a'; 8192],
    };
    let (lines, more, error) = checkout::read_head(&mut reader, 8);
    assert!(
        error.is_some(),
        "a discard failure must surface as a read error; lines = {:?}",
        lines.iter().map(String::len).collect::<Vec<_>>()
    );
    assert!(!more, "a failed read reports nothing beyond it");
    assert!(
        lines.iter().all(|l| l != "tail"),
        "the overlong line's remainder must not parse as a new line"
    );
}
