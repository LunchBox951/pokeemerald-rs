//! Pin lookup, reference verification, and isolated materialisation.
//!
//! The reference checkout is only ever read. The build tree is a fresh clone
//! of it at the pinned commit, so the build's writes land in scratch storage.

use std::ffi::OsStr;
use std::io::{BufRead, BufReader, ErrorKind, Read};
use std::path::Path;
use std::process::{Command, Stdio};

use super::RomBuildError;

/// Settings applied to every git invocation so repo-local or host-global
/// configuration cannot rewrite what the comparison sees.
const HARDENED: [&str; 12] = [
    "--no-replace-objects",
    "-c",
    "core.autocrlf=false",
    "-c",
    "core.eol=lf",
    "-c",
    "core.excludesFile=/dev/null",
    "-c",
    "core.fsmonitor=false",
    "-c",
    "core.attributesFile=/dev/null",
    "-c",
];

const PIN_PREFIX: &str = "POKEEMERALD_REF=\"${POKEEMERALD_REF:-";

/// Reads the pinned upstream revision from the repository's `init.sh`.
pub(super) fn read_pin(init_sh: &Path) -> Result<String, RomBuildError> {
    let unreadable = |reason: String| RomBuildError::PinUnreadable {
        path: init_sh.to_path_buf(),
        reason,
    };
    let text = std::fs::read_to_string(init_sh).map_err(|err| unreadable(err.to_string()))?;
    text.lines()
        .find_map(|line| line.strip_prefix(PIN_PREFIX)?.strip_suffix("}\""))
        .filter(|sha| is_full_sha(sha))
        .map(str::to_owned)
        .ok_or_else(|| unreadable("no 40-hex `POKEEMERALD_REF` default found".to_owned()))
}

fn is_full_sha(text: &str) -> bool {
    text.len() == 40 && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// How many `git status` entries a dirty-tree report lists.
const DIRTY_ENTRIES: usize = 20;
/// How many hidden index entries a rejection lists.
const HIDDEN_ENTRIES: usize = 10;
/// The longest `git status` line kept; longer lines are cut.
const MAX_STATUS_LINE: u64 = 8192;
/// How much of a git failure's stderr is kept.
const MAX_GIT_STDERR: u64 = 64 * 1024;

/// A hardened git invocation in `cwd`, with an empty environment beside
/// `PATH` and no host or system configuration.
fn git_command(cwd: &Path) -> Command {
    let mut command = Command::new("git");
    command
        .args(HARDENED)
        .arg("core.checkStat=default")
        .arg("-C")
        .arg(cwd)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .stdin(Stdio::null());
    command
}

fn spawn_failure(err: &std::io::Error) -> RomBuildError {
    if err.kind() == ErrorKind::NotFound {
        RomBuildError::MissingTool("git".to_owned())
    } else {
        RomBuildError::Git(err.to_string())
    }
}

/// Consume input through the next newline (or EOF) without retaining it:
/// memory stays bounded by the reader's fixed buffer, however long the line.
fn discard_line<R: BufRead>(reader: &mut R) -> std::io::Result<()> {
    loop {
        let (used, done) = {
            let buf = match reader.fill_buf() {
                Ok(buf) => buf,
                Err(err) if err.kind() == ErrorKind::Interrupted => continue,
                Err(err) => return Err(err),
            };
            if buf.is_empty() {
                return Ok(());
            }
            match buf.iter().position(|&b| b == b'\n') {
                Some(i) => (i + 1, true),
                None => (buf.len(), false),
            }
        };
        reader.consume(used);
        if done {
            return Ok(());
        }
    }
}

/// Reads at most `limit` lines `keep` accepts from `reader`, each cut to
/// `MAX_STATUS_LINE` bytes, and reports whether more matching lines followed
/// and any read error. Lines `keep` rejects are read through and dropped, and
/// an over-long line's remainder is discarded without being retained, so
/// memory stays bounded however long the listing.
pub(super) fn read_matching<R: BufRead>(
    reader: &mut R,
    limit: usize,
    keep: impl Fn(&str) -> bool,
) -> (Vec<String>, bool, Option<std::io::Error>) {
    let mut lines = Vec::new();
    loop {
        let mut line = Vec::new();
        match (&mut *reader)
            .take(MAX_STATUS_LINE)
            .read_until(b'\n', &mut line)
        {
            Ok(0) => return (lines, false, None),
            Ok(_) => {}
            Err(err) => return (lines, false, Some(err)),
        }
        if !line.ends_with(b"\n") && line.len() as u64 == MAX_STATUS_LINE {
            // Skip the rest of an over-long line; a failure there is a read
            // error like any other, or its remainder would parse as new lines.
            if let Err(err) = discard_line(reader) {
                return (lines, false, Some(err));
            }
        }
        let line = String::from_utf8_lossy(&line);
        if !keep(&line) {
            continue;
        }
        if lines.len() == limit {
            return (lines, true, None);
        }
        lines.push(line.trim_end().to_owned());
    }
}

/// Runs git in `cwd` and returns at most the first `limit` lines of its
/// stdout, and whether more followed. It stops reading once past `limit`
/// and kills git, so a huge listing is never held in memory.
fn git_head(cwd: &Path, args: &[&str], limit: usize) -> Result<(Vec<String>, bool), RomBuildError> {
    git_head_matching(cwd, args, limit, |_| true)
}

/// As [`git_head`], but only stdout lines `keep` accepts are counted and
/// returned; git is still read to its end, one bounded line at a time, unless
/// more than `limit` lines match.
fn git_head_matching(
    cwd: &Path,
    args: &[&str],
    limit: usize,
    keep: impl Fn(&str) -> bool,
) -> Result<(Vec<String>, bool), RomBuildError> {
    let mut child = git_command(cwd)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| spawn_failure(&err))?;
    // Drain stderr beside stdout so a chatty git cannot fill its pipe and
    // stall; keep a bounded prefix for the error.
    let stderr = child.stderr.take().map(|stream| {
        std::thread::spawn(move || {
            let mut kept = Vec::new();
            let mut stream = stream;
            let _ = (&mut stream).take(MAX_GIT_STDERR).read_to_end(&mut kept);
            let _ = std::io::copy(&mut stream, &mut std::io::sink());
            kept
        })
    });
    let (lines, more, read_error) = match child.stdout.take() {
        Some(stdout) => read_matching(&mut BufReader::new(stdout), limit, keep),
        None => (Vec::new(), false, None),
    };
    if more {
        let _ = child.kill();
    }
    let status = child
        .wait()
        .map_err(|err| RomBuildError::Git(err.to_string()))?;
    let stderr = stderr
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default();
    if more {
        return Ok((lines, true));
    }
    if let Some(err) = read_error {
        return Err(RomBuildError::Git(err.to_string()));
    }
    if !status.success() {
        return Err(RomBuildError::Git(
            String::from_utf8_lossy(&stderr).trim().to_owned(),
        ));
    }
    Ok((lines, false))
}

/// Runs git in `cwd` and returns trimmed stdout.
fn git(cwd: &Path, args: &[&OsStr]) -> Result<String, RomBuildError> {
    let output = git_command(cwd)
        .args(args)
        .output()
        .map_err(|err| spawn_failure(&err))?;
    if !output.status.success() {
        return Err(RomBuildError::Git(
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn git_str(cwd: &Path, args: &[&str]) -> Result<String, RomBuildError> {
    let args: Vec<&OsStr> = args.iter().map(OsStr::new).collect();
    git(cwd, &args)
}

/// Turns a git failure into "this tree cannot be verified"; a missing `git`
/// stays a missing tool.
fn unverifiable(dir: &Path) -> impl Fn(RomBuildError) -> RomBuildError + '_ {
    move |err| match err {
        RomBuildError::Git(reason) => RomBuildError::Unverifiable {
            path: dir.to_path_buf(),
            reason,
        },
        other => other,
    }
}

/// Asserts `dir` is a real git checkout at `pin` with a clean tree.
pub(super) fn verify_reference(dir: &Path, pin: &str) -> Result<(), RomBuildError> {
    if !dir.is_dir() {
        return Err(RomBuildError::MissingReference(dir.to_path_buf()));
    }
    let fail = unverifiable(dir);
    let top = git_str(dir, &["rev-parse", "--show-toplevel"]).map_err(&fail)?;
    // A tarball extraction would resolve to the enclosing product repository.
    let same_tree = match (Path::new(&top).canonicalize(), dir.canonicalize()) {
        (Ok(top), Ok(dir)) => top == dir,
        _ => false,
    };
    if !same_tree {
        return Err(RomBuildError::Unverifiable {
            path: dir.to_path_buf(),
            reason: "no git metadata of its own".to_owned(),
        });
    }
    let actual = git_str(dir, &["rev-parse", "HEAD"]).map_err(&fail)?;
    if actual != pin {
        return Err(RomBuildError::PinMismatch {
            path: dir.to_path_buf(),
            expected: pin.to_owned(),
            actual,
        });
    }
    if !git_str(dir, &["for-each-ref", "refs/replace"])
        .map_err(&fail)?
        .is_empty()
    {
        return Err(RomBuildError::Unverifiable {
            path: dir.to_path_buf(),
            reason: "has git replacement refs".to_owned(),
        });
    }
    ensure_visible_index(dir)?;
    ensure_clean(dir)
}

/// Rejects assume-unchanged (lowercase) and skip-worktree (`S`) index entries:
/// they make `git status` blind to edits of tracked files.
fn ensure_visible_index(dir: &Path) -> Result<(), RomBuildError> {
    let (hidden, _) = git_head_matching(dir, &["ls-files", "-v"], HIDDEN_ENTRIES, |line| {
        line.starts_with(|c: char| c.is_ascii_lowercase() || c == 'S')
    })
    .map_err(unverifiable(dir))?;
    if hidden.is_empty() {
        return Ok(());
    }
    Err(RomBuildError::HiddenIndexState {
        path: dir.to_path_buf(),
        entries: hidden.join("\n"),
    })
}

fn ensure_clean(dir: &Path) -> Result<(), RomBuildError> {
    let (entries, more) = git_head(
        dir,
        &[
            "status",
            "--porcelain",
            "--ignored",
            "--untracked-files=all",
        ],
        DIRTY_ENTRIES,
    )
    .map_err(unverifiable(dir))?;
    if entries.iter().all(String::is_empty) && !more {
        return Ok(());
    }
    let mut status = entries.join("\n");
    if more {
        status.push_str("\n... and more");
    }
    Err(RomBuildError::DirtyTree {
        path: dir.to_path_buf(),
        status,
    })
}

/// Clones `reference` into `dest` and checks out `pin`, then re-verifies the
/// result is exactly the pinned, clean tree.
pub(super) fn materialize(reference: &Path, pin: &str, dest: &Path) -> Result<(), RomBuildError> {
    let parent = dest.parent().unwrap_or(dest);
    let fail = |err: RomBuildError| match err {
        RomBuildError::Git(reason) => RomBuildError::Materialize(reason),
        other => other,
    };
    git(
        parent,
        &[
            OsStr::new("clone"),
            OsStr::new("--no-hardlinks"),
            OsStr::new("--no-checkout"),
            OsStr::new("--"),
            reference.as_os_str(),
            dest.as_os_str(),
        ],
    )
    .map_err(fail)?;
    git_str(dest, &["checkout", "--detach", pin]).map_err(fail)?;
    let head = git_str(dest, &["rev-parse", "HEAD"]).map_err(fail)?;
    if head != pin {
        return Err(RomBuildError::PinMismatch {
            path: dest.to_path_buf(),
            expected: pin.to_owned(),
            actual: head,
        });
    }
    ensure_clean(dest)
}
