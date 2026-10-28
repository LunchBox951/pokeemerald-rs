//! Bounded reading of git's status output: the overlong-line discard and
//! the read errors `read_matching` must surface.

use super::*;

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
    let (lines, more, error) = checkout::read_matching(&mut reader, 8, |_| true);
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
fn read_matching_reports_a_read_error_while_discarding_an_overlong_line() {
    struct Flaky {
        stage: u8,
        tail: &'static [u8],
        buf: Vec<u8>,
    }
    impl std::io::Read for Flaky {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            unreachable!("read_matching reads through fill_buf")
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
    let (lines, more, error) = checkout::read_matching(&mut reader, 8, |_| true);
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

/// Stages `count` extra entries with ~2 KiB paths in `reference`'s index and
/// returns the last of them in index order.
fn stage_long_paths(reference: &Path, count: usize) -> String {
    use std::io::Write;
    let blob = git(reference, &["rev-parse", "HEAD:Makefile"]);
    let info = reference.join(".git").join("index-info");
    let long = "p".repeat(2000);
    {
        let mut out = std::io::BufWriter::new(fs::File::create(&info).unwrap());
        for i in 0..count {
            writeln!(out, "100644 {blob}\t{long}{i:06}").unwrap();
        }
    }
    let status = Command::new("git")
        .arg("-C")
        .arg(reference)
        .args(["update-index", "--add", "--index-info"])
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .stdin(fs::File::open(&info).unwrap())
        .status()
        .unwrap();
    assert!(status.success());
    fs::remove_file(&info).unwrap();
    format!("{long}{:06}", count - 1)
}

/// Largest single allocation `verify_reference` makes on this thread.
fn largest_verify_allocation(fx: &Fixture) -> (usize, RomBuildError) {
    MAX_ALLOC.with(|m| m.set(0));
    let err =
        checkout::verify_reference(&fx.config.reference, &fx.config.pinned_revision).unwrap_err();
    (MAX_ALLOC.with(std::cell::Cell::get), err)
}

#[test]
fn a_huge_index_listing_is_not_held_in_memory() {
    let fx = Fixture::new("huge-index", GOOD);
    // ~60 MiB of `ls-files -v` output, all of it visible.
    stage_long_paths(&fx.config.reference, 30_000);
    let (largest, err) = largest_verify_allocation(&fx);
    assert!(matches!(err, RomBuildError::DirtyTree { .. }), "{err}");
    // Collecting the listing would allocate it whole (tens of MiB at once).
    assert!(
        largest <= 1 << 20,
        "verifying allocated {largest} bytes at once"
    );
}

#[test]
fn a_hidden_entry_after_a_huge_index_listing_is_still_rejected() {
    let fx = Fixture::new("huge-index-hidden", GOOD);
    let reference = &fx.config.reference;
    let last = stage_long_paths(reference, 30_000);
    git(reference, &["update-index", "--assume-unchanged", &last]);
    let (largest, err) = largest_verify_allocation(&fx);
    let RomBuildError::HiddenIndexState { entries, .. } = &err else {
        panic!("{err}");
    };
    assert_eq!(entries.lines().count(), 1, "{err}");
    assert!(entries.starts_with("h ppp"), "{err}");
    assert!(
        largest <= 1 << 20,
        "verifying allocated {largest} bytes at once"
    );
}

#[test]
fn at_most_ten_hidden_entries_are_reported() {
    let fx = Fixture::new("many-hidden", GOOD);
    let reference = &fx.config.reference;
    stage_long_paths(reference, 30);
    git(
        reference,
        &["update-index", "--skip-worktree", "--", "Makefile"],
    );
    let paths: Vec<String> = git(reference, &["ls-files"])
        .lines()
        .filter(|path| path.starts_with('p'))
        .map(str::to_owned)
        .collect();
    for path in &paths[..12] {
        git(reference, &["update-index", "--assume-unchanged", path]);
    }
    let (_, err) = largest_verify_allocation(&fx);
    let RomBuildError::HiddenIndexState { entries, .. } = &err else {
        panic!("{err}");
    };
    let lines: Vec<&str> = entries.lines().collect();
    assert_eq!(lines.len(), 10, "{err}");
    assert_eq!(lines[0], "S Makefile");
    assert!(
        lines[1..].iter().all(|line| line.starts_with("h ppp")),
        "{err}"
    );
}
