use super::super::open;
use super::*;

/// Windows's `FILE_ATTRIBUTE_REPARSE_POINT`, checked below as this test
/// module's own confirmation that the fixture built a reparse point,
/// independent of `open`'s copy of the same flag.
#[cfg(windows)]
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;

/// A directory registered as a cloud-files sync root, unregistered again on
/// every exit path (drop runs on a panic unwind too).
///
/// Declare it after the [`TempDir`] it registers so it drops first: a sync
/// root cannot be unregistered from a directory that no longer exists.
#[cfg(windows)]
struct SyncRootGuard {
    root: Vec<u16>,
}

#[cfg(windows)]
impl Drop for SyncRootGuard {
    fn drop(&mut self) {
        // SAFETY: `root` is the NUL-terminated wide path this guard registered.
        let result = unsafe {
            windows_sys::Win32::Storage::CloudFilters::CfUnregisterSyncRoot(self.root.as_ptr())
        };
        if result < 0 {
            // Not a panic: this may already be a panic unwind, and a leaked registration
            // must not mask the assertion that started it.
            eprintln!("CfUnregisterSyncRoot failed with HRESULT {result:#010x}; the sync root stays registered");
        }
    }
}

/// `PHCM_EXPOSE_PLACEHOLDERS` from `ntifs.h`: the process sees every
/// placeholder as the reparse point it is.
#[cfg(windows)]
const PHCM_EXPOSE_PLACEHOLDERS: i8 = 2;

/// Opts this process out of placeholder disguise, under which the cloud
/// filter hides `FILE_ATTRIBUTE_REPARSE_POINT` on a fully hydrated
/// placeholder and hands legacy callers a plain file: a `OneDrive` save is
/// exposed to the game only once dehydrated, and this fixture keeps its
/// placeholder hydrated so the read can be served with no sync provider
/// connected, so the test process asks for the exposed view instead.
///
/// Process-wide and idempotent; `RtlSetProcessPlaceholderCompatibilityMode`
/// returns the previous mode, or a negative code on failure.
#[cfg(windows)]
fn expose_placeholders_to_this_process() {
    use windows_sys::Wdk::Storage::FileSystem::RtlSetProcessPlaceholderCompatibilityMode;

    // SAFETY: the call takes one plain integer and touches no memory of ours.
    let previous = unsafe { RtlSetProcessPlaceholderCompatibilityMode(PHCM_EXPOSE_PLACEHOLDERS) };
    assert!(
        previous >= 0,
        "RtlSetProcessPlaceholderCompatibilityMode(PHCM_EXPOSE_PLACEHOLDERS) failed with {previous}"
    );
}

/// Registers `dir` as a cloud-files sync root with hydration allowed and the
/// namespace declared complete, so the filter never waits on the sync
/// provider this test does not connect.
///
/// Panics on any registration failure, with no skip path: on a
/// save-data-risk change a Windows leg that cannot build the fixture must
/// fail, not read green while the read-path tests never ran.
#[cfg(windows)]
fn register_sync_root(dir: &TempDir, label: &str) -> SyncRootGuard {
    use windows_sys::Win32::Storage::CloudFilters::{
        CF_HYDRATION_POLICY_FULL, CF_POPULATION_POLICY_ALWAYS_FULL,
    };

    expose_placeholders_to_this_process();
    match try_register_sync_root(
        &dir.path,
        label,
        CF_HYDRATION_POLICY_FULL,
        CF_POPULATION_POLICY_ALWAYS_FULL,
    ) {
        Ok(guard) => guard,
        Err(result) => panic!(
            "sync root registration refused: CfRegisterSyncRoot on {} failed with HRESULT {result:#010x}",
            dir.path.display()
        ),
    }
}

/// Registers `root` with the given hydration and population policies,
/// returning the failing HRESULT instead of panicking.
#[cfg(windows)]
fn try_register_sync_root(
    root: &Path,
    label: &str,
    hydration: windows_sys::Win32::Storage::CloudFilters::CF_HYDRATION_POLICY_PRIMARY,
    population: windows_sys::Win32::Storage::CloudFilters::CF_POPULATION_POLICY_PRIMARY,
) -> Result<SyncRootGuard, i32> {
    use std::os::windows::ffi::OsStrExt as _;
    use windows_sys::Win32::Storage::CloudFilters::{
        CfRegisterSyncRoot, CF_REGISTER_FLAG_NONE, CF_SYNC_POLICIES, CF_SYNC_REGISTRATION,
    };

    let wide = |text: &std::ffi::OsStr| -> Vec<u16> { text.encode_wide().chain(Some(0)).collect() };
    let root = wide(root.as_os_str());
    let provider_name = wide(std::ffi::OsStr::new(&format!(
        "pokeemerald-rs-save-test-{label}-{}",
        std::process::id()
    )));
    let provider_version = wide(std::ffi::OsStr::new("1"));

    // A GUID unique to this process and label, so concurrent tests never share a provider id.
    let label_hash = label
        .bytes()
        .fold(0u8, |acc, byte| acc.wrapping_mul(31).wrapping_add(byte));
    let registration = CF_SYNC_REGISTRATION {
        StructSize: u32::try_from(std::mem::size_of::<CF_SYNC_REGISTRATION>()).unwrap(),
        ProviderName: provider_name.as_ptr(),
        ProviderVersion: provider_version.as_ptr(),
        ProviderId: windows_sys::core::GUID {
            data1: std::process::id(),
            data2: 0x1399,
            data3: 0x4a6e,
            data4: [0x91, 0x5d, 0x0b, 0x1c, 0x53, 0x76, 0x41, label_hash],
        },
        ..Default::default()
    };

    let mut policies = CF_SYNC_POLICIES {
        StructSize: u32::try_from(std::mem::size_of::<CF_SYNC_POLICIES>()).unwrap(),
        ..Default::default()
    };
    policies.Hydration.Primary = hydration;
    policies.Population.Primary = population;

    // SAFETY: every pointer names a live, NUL-terminated buffer or struct that outlives the call.
    let result = unsafe {
        CfRegisterSyncRoot(
            root.as_ptr(),
            &raw const registration,
            &raw const policies,
            CF_REGISTER_FLAG_NONE,
        )
    };
    if result < 0 {
        return Err(result);
    }
    Ok(SyncRootGuard { root })
}

/// Converts the hydrated file at `path` in place with `CfConvertToPlaceholder`
/// and returns the attributes it reads afterwards, or the failing HRESULT.
#[cfg(windows)]
fn try_convert_to_placeholder(path: &Path) -> Result<u32, i32> {
    use std::os::windows::fs::MetadataExt as _;
    use std::os::windows::io::AsRawHandle as _;
    use windows_sys::Win32::Storage::CloudFilters::{CfConvertToPlaceholder, CF_CONVERT_FLAG_NONE};

    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .expect("the file to convert opens for conversion");
    // SAFETY: the handle is live for the call; no file identity, USN out-pointer, or
    // overlapped is passed, so the call is synchronous.
    let result = unsafe {
        CfConvertToPlaceholder(
            file.as_raw_handle(),
            std::ptr::null(),
            0,
            CF_CONVERT_FLAG_NONE,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    drop(file);
    if result < 0 {
        return Err(result);
    }
    Ok(std::fs::symlink_metadata(path)
        .expect("the converted file is still there")
        .file_attributes())
}

/// Converts the hydrated file at `path`, inside a registered sync root, into
/// a genuine cloud-files placeholder that stays hydrated: its cloud-files
/// filter still serves the bytes on an ordinary read while
/// `FILE_ATTRIBUTE_REPARSE_POINT` is visible to callers, the same shape of
/// object a `OneDrive` placeholder is.
///
/// Asserts the conversion succeeded and the entry now reads as a reparse
/// point, so a fixture that never reaches the hydrating-reopen branch fails
/// loudly here instead of letting the read below pass for an unrelated reason.
#[cfg(windows)]
fn convert_into_a_hydrated_placeholder(path: &Path) {
    use std::os::windows::fs::MetadataExt as _;

    let before = std::fs::symlink_metadata(path)
        .expect("the file to convert is there")
        .file_attributes();
    let attributes = match try_convert_to_placeholder(path) {
        Ok(attributes) => attributes,
        Err(result) => panic!(
            "CfConvertToPlaceholder on {} failed with HRESULT {result:#010x}",
            path.display()
        ),
    };
    assert!(
        attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0,
        "CfConvertToPlaceholder must turn {} into a reparse point for this test to mean anything; \
         attributes {before:#x} before, {attributes:#x} after",
        path.display(),
    );
}

/// A non-symlink Windows reparse point at the save path must load through
/// its filter rather than being refused as
/// [`SaveFileError::SavePathNotAPlainFile`], which would disable saving
/// under a synced folder whose provider dehydrates files it has not touched
/// recently.
#[cfg(windows)]
#[test]
fn reading_a_non_symlink_reparse_point_in_the_files_place_loads_through_its_filter() {
    let dir = TempDir::new("read-cloud-placeholder");
    let path = dir.join(SAVE_FILE_NAME);
    let (store, _, _) = saved_store();
    SaveFile::at(&path).write(&store).unwrap();
    let _sync_root = register_sync_root(&dir, "read-cloud-placeholder");
    convert_into_a_hydrated_placeholder(&path);

    let reloaded = SaveFile::at(&path)
        .read()
        .expect("a non-symlink reparse point must load through its filter, not be refused")
        .expect("the file was just written");
    assert_eq!(reloaded.flash_image(), store.flash_image());
}

/// A reparse point's hydrating reopen must read through the object identity
/// already verified, not whatever now occupies the path's name: `open`'s
/// `open_verified_for_read_with` lands a swap -- the original entry renamed
/// aside, a different file taking its name -- right before the reopen that
/// would otherwise be a second, redirectable lookup of the path. The swap
/// must have no effect on what comes back.
#[cfg(windows)]
#[test]
fn a_reparse_points_hydrating_reopen_reads_through_the_verified_object_despite_a_path_swap() {
    use std::io::Read as _;

    let dir = TempDir::new("read-windows-reopen-ignores-a-path-swap");
    let path = dir.join(SAVE_FILE_NAME);
    let (original, _, _) = saved_store();
    SaveFile::at(&path).write(&original).unwrap();
    let _sync_root = register_sync_root(&dir, "reopen-path-swap");
    convert_into_a_hydrated_placeholder(&path);

    let carried_off = dir.join("carried-off.sav");
    let outcome = open::open_verified_for_read_with(&path, |_verified_handle| {
        // Renamed aside, not removed: a volume without POSIX delete semantics would only
        // mark a same-name-while-open remove pending, refusing the create below with access
        // denied until the verified handle closes, which would test that refusal instead of
        // the swap this test means to exercise.
        std::fs::rename(&path, &carried_off).expect("the verified entry is renamed aside");
        std::fs::write(&path, vec![0xFFu8; FLASH_IMAGE_LEN])
            .expect("a different file takes the same name");
    });

    let mut file = outcome
        .expect("a path swap after the handle was verified must not fail the read")
        .expect("the reparse point was verified to exist");
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .expect("the reopened handle is still readable");
    assert_eq!(
        bytes,
        original.flash_image(),
        "the hydrating reopen must read through the already-verified object, not whatever now \
         occupies the path's name"
    );
}
