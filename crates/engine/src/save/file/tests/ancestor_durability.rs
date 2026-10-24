use super::*;

/// The container of every level from the filesystem root down to `target`,
/// outermost first, computed independently of [`SaveFile::ancestor_chain`]
/// and [`SaveFile::directory_containing`] via [`Path::ancestors`].
fn expected_ancestor_containers(target: &Path) -> Vec<PathBuf> {
    let mut containers: Vec<PathBuf> = target
        .ancestors()
        .filter_map(|level| level.parent().map(Path::to_path_buf))
        .collect();
    containers.reverse();
    containers
}

#[test]
fn ancestor_chain_lists_every_level_outermost_first() {
    let dir = TempDir::new("ancestor-chain");
    let target = dir.join("one").join("two");

    let chain = SaveFile::ancestor_chain(&target);
    let mut expected: Vec<PathBuf> = target.ancestors().map(Path::to_path_buf).collect();
    expected.reverse();
    assert_eq!(
        chain, expected,
        "the chain must list every level from the filesystem root to the target, \
         outermost first, with nothing skipped or reordered"
    );
}

#[test]
fn locking_a_fresh_multi_level_root_syncs_the_whole_ancestor_chain() {
    let dir = TempDir::new("sync-created");
    let target = dir.join("one").join("two");
    let file = SaveFile::at(target.join(SAVE_FILE_NAME));

    let synced = std::cell::RefCell::new(Vec::new());
    let guard = file
        .lock_with(|path| synced.borrow_mut().push(path.to_path_buf()))
        .expect("a fresh multi-level root must be lockable");
    drop(guard);

    assert!(target.is_dir());
    assert_eq!(
        synced.into_inner(),
        expected_ancestor_containers(&target),
        "on a first save, every level's directory entry in its own container must be \
         synced, outermost first, and nothing else -- otherwise a created directory's \
         entry can be unsynced and vanish after a power loss, or an unrelated \
         directory can be synced unintentionally"
    );
}

#[test]
fn a_first_save_under_an_absolute_path_never_syncs_the_working_directory() {
    let dir = TempDir::new("absolute-no-cwd-sync");
    let file = SaveFile::at(dir.join(SAVE_FILE_NAME));
    assert!(dir.path.is_absolute(), "the temp root must be absolute");

    let synced = std::cell::RefCell::new(Vec::new());
    let guard = file
        .lock_with(|path| synced.borrow_mut().push(path.to_path_buf()))
        .expect("a fresh absolute path must be lockable");
    drop(guard);

    assert!(
        !synced.into_inner().contains(&PathBuf::from(".")),
        "a first save under an absolute path must never sync the process's working \
         directory -- the filesystem root has no container to record its own entry in"
    );
}

#[test]
fn directory_containing_has_no_entry_to_record_for_a_level_that_already_exists() {
    for level in [Path::new("/"), Path::new("."), Path::new("..")] {
        assert_eq!(
            SaveFile::directory_containing(level),
            None,
            "{level:?} always exists already, so it has no directory entry that \
             `create_dir_all` could have made and no container to synchronise"
        );
    }
}

#[test]
fn directory_containing_returns_the_parent_for_a_level_create_dir_all_can_make() {
    assert_eq!(
        SaveFile::directory_containing(Path::new("/tmp")),
        Some(Path::new("/"))
    );
    assert_eq!(
        SaveFile::directory_containing(Path::new("sub")),
        Some(Path::new(".")),
        "a bare relative name's entry lives in the working directory"
    );
    assert_eq!(
        SaveFile::directory_containing(Path::new("../saves")),
        Some(Path::new("..")),
        "the entry this level actually adds lives in its literal parent, not in \
         the working directory the whole relative path is resolved against"
    );
    assert_eq!(
        SaveFile::directory_containing(Path::new("./saves")),
        Some(Path::new(".")),
    );
}

#[test]
fn a_locker_that_wins_the_race_syncs_ancestors_an_earlier_contender_left_unsynced() {
    let dir = TempDir::new("race");
    let target = dir.join("one").join("two");
    let path = target.join(SAVE_FILE_NAME);

    // An earlier contender created the hierarchy but was pre-empted before
    // it could sync or lock -- its directories now exist on disk with
    // nobody yet having synced them.
    let first = SaveFile::at(&path);
    first.create_parent_directory().unwrap();

    let second = SaveFile::at(&path);
    let synced = std::cell::RefCell::new(Vec::new());
    let guard = second
        .lock_with(|p| synced.borrow_mut().push(p.to_path_buf()))
        .unwrap();
    drop(guard);

    assert_eq!(
        synced.into_inner(),
        expected_ancestor_containers(&target),
        "a locker must sync every ancestor's container of a first save regardless of \
         who created it on disk, and nothing else -- otherwise an earlier contender's \
         unsynced work can be reported as a successful save"
    );
}

#[test]
fn locking_after_a_successful_first_save_syncs_nothing_more() {
    let dir = TempDir::new("sync-after-first-save");
    let file = SaveFile::at(dir.join("one").join("two").join(SAVE_FILE_NAME));
    let (store, _, _) = saved_store();

    let guard = file.lock().unwrap();
    file.write(&store).unwrap();
    drop(guard);

    let synced = std::cell::RefCell::new(Vec::new());
    let guard = file
        .lock_with(|path| synced.borrow_mut().push(path.to_path_buf()))
        .unwrap();
    drop(guard);

    assert!(
        synced.into_inner().is_empty(),
        "once a save file exists, this is no longer a first save, so ancestors must \
         not be resynced on every subsequent lock"
    );
}

#[test]
fn locking_synchronises_ancestors_only_once_the_lock_is_held() {
    let dir = TempDir::new("sync-order");
    let path = dir.join("nested").join("deeper").join(SAVE_FILE_NAME);
    let file = SaveFile::at(&path);

    let synced_while_locked = std::cell::Cell::new(false);
    let lock_path = file.lock_path();
    let guard = file
        .lock_with(|_ancestor_parent| {
            let probe = std::fs::OpenOptions::new()
                .write(true)
                .open(&lock_path)
                .expect("the lock file must already exist while ancestors are synced");
            synced_while_locked.set(matches!(
                probe.try_lock(),
                Err(std::fs::TryLockError::WouldBlock)
            ));
        })
        .expect("locking must create the missing hierarchy");
    drop(guard);

    assert!(
        synced_while_locked.get(),
        "ancestors must be synced only after this call holds the exclusive lock -- \
         syncing before locking would let a second, concurrent locker report a \
         successful save before either locker had made them durable"
    );
}

#[test]
fn locking_before_any_directory_exists_creates_the_whole_hierarchy() {
    let dir = TempDir::new("lock-mkdir");
    let path = dir.join("nested").join("deeper").join(SAVE_FILE_NAME);
    let file = SaveFile::at(&path);

    let guard = file
        .lock()
        .expect("locking must create the missing hierarchy");
    assert!(path.parent().unwrap().is_dir());
    assert!(file.lock_path().exists());

    let (store, _, _) = saved_store();
    file.write(&store).unwrap();
    drop(guard);

    let reloaded = file.read().unwrap().expect("the file was just written");
    assert_eq!(reloaded.flash_image(), store.flash_image());
}
