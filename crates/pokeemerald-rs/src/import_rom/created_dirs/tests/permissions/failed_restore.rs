//! Rollback of what `create_directories` returns when a level's
//! owner-access restore fails for real: a seccomp filter on the test's own
//! thread fails the `fchmodat(2)` both Linux setters issue, under a umask
//! that took the owner's write.

use std::ffi::{c_int, c_ulong};
use std::os::unix::fs::MetadataExt as _;

use super::super::{entry_names, fs, TempDir};

/// The `fchmodat(2)` number on the architectures
/// [`fail_fchmodat_on_this_thread`] knows, with the `AUDIT_ARCH_*` value a
/// seccomp filter sees there.
#[cfg(target_arch = "x86_64")]
const FCHMODAT: (u32, u32) = (268, 0xC000_003E);
#[cfg(target_arch = "aarch64")]
const FCHMODAT: (u32, u32) = (53, 0xC000_00B7);

/// A classic BPF instruction, as `<linux/filter.h>` lays out `sock_filter`.
#[repr(C)]
struct SockFilter {
    code: u16,
    jt: u8,
    jf: u8,
    k: u32,
}

/// An instruction that falls through when true and skips `jf` when false.
const fn bpf(code: u16, jf: u8, k: u32) -> SockFilter {
    SockFilter { code, jt: 0, jf, k }
}

/// `<linux/filter.h>`'s `sock_fprog`.
#[repr(C)]
struct SockFprog {
    len: u16,
    filter: *const SockFilter,
}

extern "C" {
    fn prctl(option: c_int, ...) -> c_int;
}

/// Makes every `fchmodat(2)` this thread issues from now on fail with
/// `EACCES`, through a seccomp filter only this thread carries: both of
/// `restore_directory_owner_access`'s Linux setters (`fchmodat(level, ".")`
/// and the `/proc/self/fd` fallback) issue it, as with the owner's search
/// gone and `/proc` unmounted, while rollback's quarantine uses `fchmod(2)`
/// and is unaffected. Mode bits and capabilities do not bypass a filter,
/// so this holds under root too.
fn fail_fchmodat_on_this_thread() {
    const PR_SET_SECCOMP: c_int = 22;
    const PR_SET_NO_NEW_PRIVS: c_int = 38;
    const SECCOMP_MODE_FILTER: c_ulong = 2;
    const ON: c_ulong = 1;
    const UNUSED: c_ulong = 0;
    const LOAD_WORD_AT: u16 = 0x20; // BPF_LD | BPF_W | BPF_ABS
    const JUMP_IF_EQUAL: u16 = 0x15; // BPF_JMP | BPF_JEQ | BPF_K
    const RETURN: u16 = 0x06; // BPF_RET | BPF_K
    const RET_ALLOW: u32 = 0x7FFF_0000;
    const RET_ERRNO: u32 = 0x0005_0000;
    let (number, arch) = FCHMODAT;
    let eacces = u32::try_from(rustix::io::Errno::ACCESS.raw_os_error()).unwrap();
    let filter = [
        bpf(LOAD_WORD_AT, 0, 4),     // `seccomp_data.arch`
        bpf(JUMP_IF_EQUAL, 3, arch), // another ABI: allow
        bpf(LOAD_WORD_AT, 0, 0),     // `seccomp_data.nr`
        bpf(JUMP_IF_EQUAL, 1, number),
        bpf(RETURN, 0, RET_ERRNO | eacces),
        bpf(RETURN, 0, RET_ALLOW),
    ];
    let program = SockFprog {
        len: u16::try_from(filter.len()).unwrap(),
        filter: filter.as_ptr(),
    };
    // SAFETY: `PR_SET_NO_NEW_PRIVS` takes plain integer arguments and
    // touches no memory; it is per-thread, so only the thread running this
    // test (alone in its child process) is affected.
    let no_new_privs = unsafe { prctl(PR_SET_NO_NEW_PRIVS, ON, UNUSED, UNUSED, UNUSED) };
    assert_eq!(no_new_privs, 0, "{}", std::io::Error::last_os_error());
    // SAFETY: `program` points at `filter`, a live array of `len`
    // `sock_filter`s laid out as the kernel expects; the kernel copies it
    // during the call, so neither need outlive it.
    let installed = unsafe { prctl(PR_SET_SECCOMP, SECCOMP_MODE_FILTER, &raw const program) };
    assert_eq!(installed, 0, "{}", std::io::Error::last_os_error());
}

/// Creates `a/b` under `mask`, an owner-stripping umask, with `b`'s
/// owner-access restore failing for real, and rolls back what creation
/// returned: the error, `b` recorded last with both pins and without owner
/// write, and `a` with its access restored. Both are removed, `b` without
/// any capture, with nothing left beside them.
fn assert_failed_restore_is_rolled_back(label: &str, mask: u32) {
    let dir = TempDir::new(label);
    let level = dir.join("a").join("b");
    // The umask is process-wide; this runs alone in its child process.
    let previous = rustix::process::umask(rustix::fs::Mode::from_raw_mode(mask));
    let mut reopens = 0;
    let result = super::super::create_directories_with_hooks(
        &level,
        super::super::MAX_PINNED_LEVELS,
        &mut || {},
        &mut || {
            reopens += 1;
            if reopens == 2 {
                fail_fchmodat_on_this_thread();
            }
            None
        },
        &mut || None,
    );
    let Err((created, source)) = result else {
        panic!("creation succeeded with the owner-access restore failing");
    };

    assert_eq!(
        source.kind(),
        std::io::ErrorKind::PermissionDenied,
        "{source}"
    );
    assert_eq!(
        created.len(),
        2,
        "creation did not stop at the failing level"
    );
    let failed = &created[1];
    assert_eq!(failed.path, level);
    assert!(
        failed.parent.is_some() && failed.own.is_some(),
        "the failing level was not recorded with both pins"
    );
    let mode = |path: &std::path::Path| fs::metadata(path).unwrap().mode() & 0o777;
    assert_eq!(
        mode(&level),
        0o777 & !mask,
        "the failing level's owner access was restored"
    );
    assert_eq!(
        mode(&dir.join("a")) & 0o700,
        0o700,
        "the ancestor was not restored"
    );

    // Put back before rollback: what a mask that takes the owner's read does
    // to the quarantine made for `a` is not what this covers.
    rustix::process::umask(previous);
    let stages = super::rollback_stages(&created);

    assert!(!level.exists(), "the level whose restore failed was left");
    assert_eq!(entry_names(&dir.path), Vec::<String>::new());
    // `b` was the first level to reach the take-back, and was never
    // captured; `a` was second, through the quarantine.
    assert!(
        stages.iter().all(|&(taken, _)| taken == 2),
        "the level without owner write was moved into a quarantine: {stages:?}"
    );
}

#[test]
fn a_level_left_at_no_mode_bits_by_a_failed_restore_is_removed_with_its_ancestors_silently() {
    if super::assert_silent(
        "import_rom::created_dirs::tests::permissions::failed_restore::a_level_left_at_no_mode_bits_by_a_failed_restore_is_removed_with_its_ancestors_silently",
    ) {
        return;
    }
    assert_failed_restore_is_rolled_back("restore-failed-000", 0o777);
}

#[test]
fn a_level_left_without_owner_write_by_a_failed_restore_is_removed_with_its_ancestors_silently() {
    if super::assert_silent(
        "import_rom::created_dirs::tests::permissions::failed_restore::a_level_left_without_owner_write_by_a_failed_restore_is_removed_with_its_ancestors_silently",
    ) {
        return;
    }
    assert_failed_restore_is_rolled_back("restore-failed-0500", 0o277);
}
