use super::{
    BoundedPersistor, DEFAULT_MAX_LEAF_BYTES, DatabasePersistor, MAX_TOTAL_WAL_SIZE,
    MemoryPersistor, PERSISTENT_COLUMN_FAMILIES, Persistor, SIZE, STORAGE_FORMAT_FILE, Sha256,
    Word, branch_word, check_leaf_length, has_rocksdb_artifacts, initialize_storage_format,
    leaf_word, parse_max_leaf_bytes, stump_word,
};
use rocksdb::{ColumnFamilyDescriptor, DB, IteratorMode, Options};
use sha2::Digest as _;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::sync::RwLock;

#[test]
fn test_storage_format_directory_entry_errors_fail_closed() {
    let entries: Vec<io::Result<OsString>> = vec![Err(io::Error::new(
        io::ErrorKind::PermissionDenied,
        "synthetic directory-entry failure",
    ))];
    let result = catch_unwind(AssertUnwindSafe(|| has_rocksdb_artifacts(entries)));
    assert!(result.is_err());
}

#[test]
fn test_storage_format_marker_read_errors_fail_closed() {
    let path = ".test-storage-format-marker-read-errors-fail-closed";
    let _ = fs::remove_dir_all(path);
    fs::create_dir_all(Path::new(path).join(STORAGE_FORMAT_FILE)).expect("marker directory");
    let result = catch_unwind(AssertUnwindSafe(|| {
        initialize_storage_format(Path::new(path))
    }));
    assert!(result.is_err());
    assert!(Path::new(path).join(STORAGE_FORMAT_FILE).is_dir());
    let _ = fs::remove_dir_all(path);
}

fn assert_bounded_reads(persistor: &dyn BoundedPersistor) {
    for value in 1..=3_u8 {
        persistor.root_new([value; SIZE], [0; SIZE]).unwrap();
    }
    assert!(persistor.root_list_bounded(2).is_err());
    assert_eq!(persistor.root_list_bounded(3).unwrap().len(), 3);
    persistor.root_temp([0; SIZE]).unwrap();
    assert!(persistor.root_list_bounded(3).is_err());
    assert_eq!(persistor.root_list().len(), 3);
    let content = vec![9; 1024];
    let leaf = persistor.leaf_set(content.clone()).unwrap();
    assert!(persistor.leaf_get_bounded(leaf, 1023).is_err());
    assert_eq!(persistor.leaf_get_bounded(leaf, 1024).unwrap(), content);
}

#[test]
fn test_max_leaf_configuration_is_bounded_and_fail_closed() {
    assert_eq!(parse_max_leaf_bytes(None), Ok(64 * 1024 * 1024));
    assert_eq!(parse_max_leaf_bytes(Some("4096")), Ok(4096));
    assert_eq!(parse_max_leaf_bytes(Some("62")), Ok(62));
    assert!(parse_max_leaf_bytes(Some("61")).is_err());
    assert!(parse_max_leaf_bytes(Some("0")).is_err());
    assert!(parse_max_leaf_bytes(Some("invalid")).is_err());
    assert!(parse_max_leaf_bytes(Some("67108865")).is_err());
    assert!(check_leaf_length(DEFAULT_MAX_LEAF_BYTES).is_ok());
    assert_eq!(
        check_leaf_length(DEFAULT_MAX_LEAF_BYTES + 1).unwrap_err().0,
        "Maximum leaf size exceeded"
    );
}

#[test]
fn test_memory_and_database_bounded_reads() {
    assert_bounded_reads(&MemoryPersistor::new());
    let path = ".test-bounded-reads";
    let _ = fs::remove_dir_all(path);
    {
        let persistor = DatabasePersistor::new(path);
        assert_bounded_reads(&persistor);
        let content = vec![4; 2048];
        let leaf = persistor.leaf_set(content.clone()).unwrap();
        assert_eq!(persistor.leaf_get(leaf).unwrap(), content);
        let db = persistor.db.write().unwrap();
        db.put_cf(db.cf_handle("leaves").unwrap(), leaf, vec![5; 2048])
            .unwrap();
        drop(db);
        assert_eq!(
            persistor.leaf_get_bounded(leaf, 2048).unwrap_err().0,
            "Leaf digest mismatch"
        );
        assert_eq!(
            persistor.leaf_get(leaf).unwrap_err().0,
            "Leaf digest mismatch"
        );
    }
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn test_sync_node_word_vectors() {
    let zeros = [0; SIZE];
    let left = [0x11; SIZE];
    let right = [0x22; SIZE];

    assert_eq!(
        hex::encode(leaf_word(b"")),
        "5df6e0e2761359d30a8275058e299fcc0381534545f55cf43e41983f5d4c9456"
    );
    assert_eq!(
        hex::encode(leaf_word(b"abc")),
        "4f8b42c22dd3729b519ba6f68d2da7cc5b2d606d05daed5ad5128cc03e6c6358"
    );
    assert_eq!(
        hex::encode(stump_word(zeros)),
        "f5a5fd42d16a20302798ef6ed309979b43003d2320d9f0e8ea9831a92759fb4b"
    );
    assert_eq!(
        hex::encode(stump_word([0x33; SIZE])),
        "79bd7d7fd684b399857c582b1b7172ddf277d4fe1b027ec52b28da3ae381e675"
    );
    assert_eq!(
        hex::encode(branch_word(zeros, left, right)),
        "b647d2614ad2099840c899399acf3b264e8f40f9eb0d0f997c2e6e1c3ffc2da6"
    );
    assert_eq!(
        hex::encode(branch_word(zeros, right, left)),
        "91c93c2c7a5bac27d74ff7d087b8de5abc5a7c722c234a24ef837c3ff7eaffc0"
    );
}

#[test]
fn test_sync_node_word_kinds_are_structurally_separate() {
    let zeros = [0; SIZE];
    let leaf = leaf_word(&[0; SIZE * 2]);
    let stump = stump_word(zeros);
    let branch = branch_word(stump, zeros, zeros);

    assert_ne!(leaf, stump);
    assert_ne!(leaf, branch);
    assert_ne!(stump, branch);
}

fn test_cross_kind_lifecycle(persistor: Box<dyn Persistor>) {
    let content = b"same semantic digest".to_vec();
    let digest: Word = Sha256::digest(&content).into();
    let leaf = persistor.leaf_set(content.clone()).expect("leaf");
    let stump = persistor.stump_set(digest).expect("stump");
    assert_ne!(leaf, stump);
    assert_eq!(persistor.leaf_get(leaf).expect("leaf content"), content);
    assert_eq!(persistor.stump_get(stump).expect("stump digest"), digest);

    let branch = persistor.branch_set(leaf, stump, digest).expect("branch");
    let handle = [0x55; SIZE];
    persistor.root_new(handle, branch).expect("root");
    persistor.root_delete(handle).expect("delete root");
    assert!(persistor.branch_get(branch).is_err());
    assert!(persistor.leaf_get(leaf).is_err());
    assert!(persistor.stump_get(stump).is_err());
}

#[test]
fn test_memory_cross_kind_lifecycle() {
    test_cross_kind_lifecycle(Box::new(MemoryPersistor::new()));
}

#[test]
fn test_database_cross_kind_lifecycle() {
    let path = ".test-database-cross-kind-lifecycle";
    let _ = fs::remove_dir_all(path);
    test_cross_kind_lifecycle(Box::new(DatabasePersistor::new(path)));
    let _ = fs::remove_dir_all(path);
}

#[test]
fn test_database_records_total_wal_limit() {
    let path = ".test-database-records-total-wal-limit";
    let _ = fs::remove_dir_all(path);
    drop(DatabasePersistor::new(path));
    let configured = fs::read_dir(path)
        .expect("database directory")
        .map(|entry| entry.expect("database entry"))
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("OPTIONS-"))
        .map(|entry| fs::read_to_string(entry.path()).expect("options file"))
        .any(|options| options.contains(&format!("max_total_wal_size={MAX_TOTAL_WAL_SIZE}")));
    assert!(configured);
    let _ = fs::remove_dir_all(path);
}

#[test]
fn test_database_reopens_new_storage_format() {
    let path = ".test-database-reopens-new-storage-format";
    let _ = fs::remove_dir_all(path);
    let handle = [0x44; SIZE];
    let content = b"reopen".to_vec();
    let leaf = {
        let persistor = DatabasePersistor::new(path);
        let leaf = persistor.leaf_set(content.clone()).expect("leaf");
        persistor.root_new(handle, leaf).expect("root");
        leaf
    };
    let persistor = DatabasePersistor::new(path);
    assert_eq!(persistor.root_get(handle).expect("root"), leaf);
    assert_eq!(persistor.leaf_get(leaf).expect("leaf"), content);
    drop(persistor);
    let _ = fs::remove_dir_all(path);
}

fn directory_snapshot(path: &str) -> Vec<(String, Vec<u8>)> {
    fn collect(root: &Path, path: &Path, snapshot: &mut Vec<(String, Vec<u8>)>) {
        for entry in fs::read_dir(path)
            .expect("directory")
            .map(|entry| entry.expect("directory entry"))
        {
            let path = entry.path();
            if entry.file_type().expect("file type").is_dir() {
                collect(root, &path, snapshot);
            } else {
                snapshot.push((
                    path.strip_prefix(root)
                        .expect("relative path")
                        .to_string_lossy()
                        .into_owned(),
                    fs::read(path).expect("file"),
                ));
            }
        }
    }

    let root = Path::new(path);
    let mut snapshot = Vec::new();
    collect(root, root, &mut snapshot);
    snapshot.sort_by(|left, right| left.0.cmp(&right.0));
    snapshot
}

#[test]
fn test_database_rejects_unversioned_persisted_nodes() {
    let path = ".test-database-rejects-unversioned-persisted-nodes";
    let _ = fs::remove_dir_all(path);
    let mut options = Options::default();
    options.create_if_missing(true);
    options.create_missing_column_families(true);
    let descriptors = PERSISTENT_COLUMN_FAMILIES
        .iter()
        .map(|name| ColumnFamilyDescriptor::new(*name, Options::default()));
    let db = DB::open_cf_descriptors(&options, path, descriptors).expect("old database");
    db.put_cf(
        db.cf_handle("leaves").expect("leaves"),
        leaf_word(b"old"),
        b"old",
    )
    .expect("old leaf");
    drop(db);

    let before = directory_snapshot(path);
    let result = catch_unwind(AssertUnwindSafe(|| DatabasePersistor::new(path)));
    assert!(result.is_err());
    assert_eq!(directory_snapshot(path), before);
    assert!(!Path::new(path).join(STORAGE_FORMAT_FILE).exists());
    let _ = fs::remove_dir_all(path);
}

#[test]
fn test_memory_and_database_words_match() {
    let path = ".test-memory-and-database-words-match";
    let _ = fs::remove_dir_all(path);
    let memory = MemoryPersistor::new();
    let database = DatabasePersistor::new(path);
    let content = b"matching words".to_vec();
    let memory_leaf = memory.leaf_set(content.clone()).expect("memory leaf");
    let database_leaf = database.leaf_set(content).expect("database leaf");
    assert_eq!(memory_leaf, database_leaf);
    let memory_stump = memory.stump_set(memory_leaf).expect("memory stump");
    let database_stump = database.stump_set(database_leaf).expect("database stump");
    assert_eq!(memory_stump, database_stump);
    assert_eq!(
        memory
            .branch_set(memory_leaf, memory_stump, memory_leaf)
            .expect("memory branch"),
        database
            .branch_set(database_leaf, database_stump, database_leaf)
            .expect("database branch")
    );
    drop(database);
    let _ = fs::remove_dir_all(path);
}

fn test_persistence(persistor: Box<dyn Persistor>) {
    let zeros: Word = [0 as u8; SIZE];
    let handle = persistor
        .root_new(zeros, zeros)
        .expect("Failed to create new root");

    assert!(
        persistor
            .root_delete(
                persistor
                    .root_temp(zeros)
                    .expect("Failed to create temp root")
            )
            .expect("Failed to delete root")
            == ()
    );

    assert!(
        persistor
            .root_get(
                persistor
                    .root_set(handle, zeros, zeros, persistor.as_ref())
                    .expect("Failed to set root"),
            )
            .expect("Failed to get root")
            == zeros
    );

    assert!(
        persistor
            .branch_get(
                persistor
                    .branch_set(zeros, zeros, zeros)
                    .expect("Failed to set branch"),
            )
            .expect("Failed to get branch")
            == (zeros, zeros, zeros)
    );

    assert!(
        persistor
            .leaf_get(persistor.leaf_set(vec!(0)).expect("Failed to set leaf"),)
            .expect("Failed to get leaf")
            == vec!(0)
    );
}

#[test]
fn test_memory_persistence() {
    test_persistence(Box::new(MemoryPersistor::new()));
}

#[test]
fn test_database_persistence() {
    let db = ".test-database-persistence";
    let _ = fs::remove_dir_all(db);
    test_persistence(Box::new(DatabasePersistor::new(db)));
    let _ = fs::remove_dir_all(db);
}

#[test]
fn test_memory_garbage() {
    let persistor = MemoryPersistor::new();
    let zeros: Word = [0 as u8; SIZE];
    let handle: Word = [0 as u8; SIZE];

    let leaf_0 = persistor.leaf_set(vec![0]).expect("Failed to set leaf 0");
    let leaf_1 = persistor.leaf_set(vec![1]).expect("Failed to set leaf 1");
    let leaf_2 = persistor.leaf_set(vec![2]).expect("Failed to set leaf 2");

    let stump_0 = persistor
        .stump_set([0 as u8; SIZE])
        .expect("Failed to set stump 0");

    let branch_a = persistor
        .branch_set(leaf_0, leaf_1, zeros)
        .expect("Failed to set branch A");
    let branch_b = persistor
        .branch_set(branch_a, leaf_2, zeros)
        .expect("Failed to set branch B");
    let branch_c = persistor
        .branch_set(branch_b, stump_0, zeros)
        .expect("Failed to set branch B");

    persistor
        .root_new(handle, branch_c)
        .expect("Failed to create new root");

    assert!(persistor.roots.read().expect("Failed to lock roots").len() == 1);
    assert!(
        persistor
            .branches
            .read()
            .expect("Failed to lock branches")
            .len()
            == 3
    );
    assert!(
        persistor
            .leaves
            .read()
            .expect("Failed to lock leaves")
            .len()
            == 3
    );
    assert!(
        persistor
            .stumps
            .read()
            .expect("Failed to lock leaves")
            .len()
            == 1
    );
    assert!(
        persistor
            .references
            .read()
            .expect("Failed to lock references")
            .len()
            == 7
    );

    let leaf_3 = persistor.leaf_set(vec![3]).expect("Failed to set leaf 3");
    let branch_d = persistor
        .branch_set(leaf_2, leaf_3, zeros)
        .expect("Failed to set branch D");
    persistor
        .root_set(handle, branch_c, branch_d, &persistor)
        .expect("Failed to set root");

    assert!(persistor.roots.read().expect("Failed to lock roots").len() == 1);
    assert!(
        persistor
            .branches
            .read()
            .expect("Failed to lock branches")
            .len()
            == 1
    );
    assert!(
        persistor
            .leaves
            .read()
            .expect("Failed to lock leaves")
            .len()
            == 2
    );
    assert!(
        persistor
            .stumps
            .read()
            .expect("Failed to lock stumps")
            .len()
            == 0
    );
    assert!(
        persistor
            .references
            .read()
            .expect("Failed to lock references")
            .len()
            == 3
    );
}

#[test]
fn test_database_garbage() {
    let db = ".test-database-garbage";
    let _ = fs::remove_dir_all(db);
    let persistor = DatabasePersistor::new(db);
    let zeros: Word = [0 as u8; SIZE];
    let handle: Word = [0 as u8; SIZE];
    let leaf_0 = persistor.leaf_set(vec![0]).expect("Failed to set leaf 0");
    let leaf_1 = persistor.leaf_set(vec![1]).expect("Failed to set leaf 1");
    let leaf_2 = persistor.leaf_set(vec![2]).expect("Failed to set leaf 2");

    let stump_0 = persistor
        .stump_set([0 as u8; SIZE])
        .expect("Failed to set stump 0");

    let branch_a = persistor
        .branch_set(leaf_0, leaf_1, zeros)
        .expect("Failed to set branch A");
    let branch_b = persistor
        .branch_set(branch_a, leaf_2, zeros)
        .expect("Failed to set branch B");
    let branch_c = persistor
        .branch_set(branch_b, stump_0, zeros)
        .expect("Failed to set branch C");

    persistor
        .root_new(handle, branch_c)
        .expect("Failed to create new root");

    let cf_count = |db: &RwLock<DB>, cf| {
        let db_ = db.read().expect("Failed to lock database");
        db_.iterator_cf(
            db_.cf_handle(cf).expect("Failed to get CF handle"),
            IteratorMode::Start,
        )
        .count()
    };

    {
        assert!(cf_count(&persistor.db, "roots") == 1);
        assert!(cf_count(&persistor.db, "branches") == 3);
        assert!(cf_count(&persistor.db, "leaves") == 3);
        assert!(cf_count(&persistor.db, "stumps") == 1);
        assert!(cf_count(&persistor.db, "references") == 7);
    }

    let leaf_3 = persistor.leaf_set(vec![3]).expect("Failed to set leaf 3");
    let branch_d = persistor
        .branch_set(leaf_2, leaf_3, zeros)
        .expect("Failed to set branch D");
    persistor
        .root_set(handle, branch_c, branch_d, &persistor)
        .expect("Failed to set root");

    {
        assert!(cf_count(&persistor.db, "roots") == 1);
        assert!(cf_count(&persistor.db, "branches") == 1);
        assert_eq!(cf_count(&persistor.db, "leaves"), 2);
        assert!(cf_count(&persistor.db, "stumps") == 0);
        assert!(cf_count(&persistor.db, "references") == 3);
    }

    let _ = fs::remove_dir_all(db);
}

fn test_root_set_merge_from_source(persistor: Box<dyn Persistor>) {
    let zeros: Word = [0 as u8; SIZE];
    let handle: Word = [1 as u8; SIZE];
    let target = Box::new(MemoryPersistor::new());

    let leaf_0 = persistor.leaf_set(vec![0]).expect("Failed to set leaf 0");
    let leaf_1 = persistor.leaf_set(vec![1]).expect("Failed to set leaf 1");
    let branch = persistor
        .branch_set(leaf_0, leaf_1, zeros)
        .expect("Failed to set source branch");

    target
        .root_new(handle, zeros)
        .expect("Failed to create target root");
    target
        .root_set(handle, zeros, branch, persistor.as_ref())
        .expect("Failed to merge root from source");

    assert!(target.root_get(handle).expect("Failed to get target root") == branch);
    assert!(
        target
            .branch_get(branch)
            .expect("Failed to get merged branch")
            == (leaf_0, leaf_1, zeros)
    );
    assert!(
        target
            .leaf_get(leaf_0)
            .expect("Failed to get merged leaf 0")
            == vec![0]
    );
    assert!(
        target
            .leaf_get(leaf_1)
            .expect("Failed to get merged leaf 1")
            == vec![1]
    );
}

#[test]
fn test_memory_root_set_merge_from_source() {
    test_root_set_merge_from_source(Box::new(MemoryPersistor::new()));
}

#[test]
fn test_database_root_set_merge_from_source() {
    let db = ".test-database-root-set-merge-from-source";
    let _ = fs::remove_dir_all(db);
    test_root_set_merge_from_source(Box::new(DatabasePersistor::new(db)));
    let _ = fs::remove_dir_all(db);
}

fn test_root_set_rejects_missing_referenced_node(target: Box<dyn Persistor>) {
    let zeros: Word = [0; SIZE];
    let handle: Word = [8; SIZE];
    let missing_left: Word = [9; SIZE];
    let missing_right: Word = [10; SIZE];
    let source = MemoryPersistor::new();
    let root = source
        .branch_set(missing_left, missing_right, zeros)
        .expect("incomplete branch");
    target.root_new(handle, zeros).expect("target root");

    let result = target.root_set(handle, zeros, root, &source);
    assert!(result.is_err());
    assert_eq!(target.root_get(handle).expect("unchanged root"), zeros);
}

#[test]
fn test_memory_root_set_rejects_missing_referenced_node() {
    test_root_set_rejects_missing_referenced_node(Box::new(MemoryPersistor::new()));
}

#[test]
fn test_database_merge_preserves_bounded_leaf() {
    let path = ".test-database-merge-preserves-bounded-leaf";
    let _ = fs::remove_dir_all(path);
    let source = MemoryPersistor::new();
    let content = vec![3; 4096];
    let leaf = source.leaf_set(content.clone()).unwrap();
    let target = DatabasePersistor::new(path);
    let handle = [12; SIZE];
    target.root_new(handle, [0; SIZE]).unwrap();
    target.root_set(handle, [0; SIZE], leaf, &source).unwrap();
    assert_eq!(target.leaf_get_bounded(leaf, 4096).unwrap(), content);
    assert!(target.leaf_get_bounded(leaf, 4095).is_err());
    drop(target);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn test_database_root_set_rejects_missing_referenced_node() {
    let db = ".test-database-root-set-rejects-missing-node";
    let _ = fs::remove_dir_all(db);
    test_root_set_rejects_missing_referenced_node(Box::new(DatabasePersistor::new(db)));
    let _ = fs::remove_dir_all(db);
}

#[test]
fn test_database_repeated_shared_history_updates_remain_complete() {
    let db = ".test-database-repeated-shared-history";
    let _ = fs::remove_dir_all(db);
    let target = DatabasePersistor::new(db);
    let zeros: Word = [0; SIZE];
    let handle: Word = [11; SIZE];
    let code = target.leaf_set(vec![0]).expect("code");
    let empty = target.leaf_set(Vec::new()).expect("empty history");
    let mut state = target
        .branch_set(code, empty, zeros)
        .expect("initial state");
    target.root_new(handle, state).expect("record root");
    let mut values = Vec::new();

    for index in 0..500_u32 {
        let temporary = target.root_temp(state).expect("temporary root");
        let source = MemoryPersistor::new();
        let value = source
            .leaf_set(index.to_le_bytes().to_vec())
            .expect("history value");
        values.push(value);
        let history = target.branch_get(state).expect("state branch").1;
        let next_history = source
            .branch_set(history, value, zeros)
            .expect("next history");
        let next_state = source
            .branch_set(code, next_history, zeros)
            .expect("next state");
        target
            .root_set(handle, state, next_state, &source)
            .expect("advance state");
        target.root_delete(temporary).expect("release temporary");
        state = next_state;
    }

    let mut history = target.branch_get(state).expect("final state").1;
    for expected in values.iter().rev() {
        let (previous, value, _) = target.branch_get(history).expect("history entry");
        assert_eq!(&value, expected);
        target.leaf_get(value).expect("retained history value");
        history = previous;
    }
    assert_eq!(history, empty);
    drop(target);
    let _ = fs::remove_dir_all(db);
}

#[test]
fn test_memory_root_set_merge_from_source_garbage() {
    let zeros: Word = [0 as u8; SIZE];
    let handle: Word = [0 as u8; SIZE];
    let target = MemoryPersistor::new();
    let source = MemoryPersistor::new();

    let leaf_0 = target
        .leaf_set(vec![0])
        .expect("Failed to set target leaf 0");
    let leaf_1 = target
        .leaf_set(vec![1])
        .expect("Failed to set target leaf 1");
    let leaf_2 = target
        .leaf_set(vec![2])
        .expect("Failed to set target leaf 2");
    let stump_0 = target
        .stump_set([0 as u8; SIZE])
        .expect("Failed to set target stump 0");
    let branch_a = target
        .branch_set(leaf_0, leaf_1, zeros)
        .expect("Failed to set target branch A");
    let branch_b = target
        .branch_set(branch_a, leaf_2, zeros)
        .expect("Failed to set target branch B");
    let branch_c = target
        .branch_set(branch_b, stump_0, zeros)
        .expect("Failed to set target branch C");
    target
        .root_new(handle, branch_c)
        .expect("Failed to create target root");

    let leaf_2_source = source
        .leaf_set(vec![2])
        .expect("Failed to set source leaf 2");
    let leaf_3_source = source
        .leaf_set(vec![3])
        .expect("Failed to set source leaf 3");
    let branch_d = source
        .branch_set(leaf_2_source, leaf_3_source, zeros)
        .expect("Failed to set source branch D");

    target
        .root_set(handle, branch_c, branch_d, &source)
        .expect("Failed to merge source root into target");

    assert!(target.roots.read().expect("Failed to lock roots").len() == 1);
    assert!(
        target
            .branches
            .read()
            .expect("Failed to lock branches")
            .len()
            == 1
    );
    assert!(target.leaves.read().expect("Failed to lock leaves").len() == 2);
    assert!(target.stumps.read().expect("Failed to lock stumps").len() == 0);
    assert!(
        target
            .references
            .read()
            .expect("Failed to lock references")
            .len()
            == 3
    );
}
