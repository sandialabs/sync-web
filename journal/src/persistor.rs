#[cfg(not(feature = "wasm-kernel"))]
use crate::config::Config;
use once_cell::sync::Lazy;
use rand::RngCore;
#[cfg(not(feature = "wasm-kernel"))]
use rand::prelude::SliceRandom;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
#[cfg(not(feature = "wasm-kernel"))]
use std::ffi::OsString;
#[cfg(not(feature = "wasm-kernel"))]
use std::fs;
#[cfg(not(feature = "wasm-kernel"))]
use std::path::Path;
use std::sync::{Arc, RwLock};

#[cfg(not(feature = "wasm-kernel"))]
use rocksdb::{ColumnFamilyDescriptor, DB, Options, WriteBatch};

#[cfg(not(feature = "wasm-kernel"))]
mod database;
mod memory;
mod merge;
#[cfg(test)]
mod tests;

#[cfg(not(feature = "wasm-kernel"))]
pub use database::DatabasePersistor;
pub use memory::MemoryPersistor;
use merge::MergePlan;

#[cfg(not(feature = "wasm-kernel"))]
pub(crate) static PERSISTOR: Lazy<Arc<dyn BoundedPersistor + Send + Sync>> = Lazy::new(|| {
    let config = Config::new();
    match config.database.as_str() {
        "" => Arc::new(MemoryPersistor::new()),
        path => Arc::new(DatabasePersistor::new(path)),
    }
});

#[cfg(feature = "wasm-kernel")]
pub(crate) static PERSISTOR: Lazy<Box<dyn Persistor + Send + Sync>> =
    Lazy::new(|| Box::new(HostPersistor));

/// Size, in bytes, of the global hash algorithm (currently SHA-256)
pub const SIZE: usize = 32;

/// Byte array describing a hash pointer (currently SHA-256)
pub type Word = [u8; SIZE];

const STORAGE_FORMAT_FILE: &str = ".sync-node-format";
const STORAGE_FORMAT: &[u8] = b"sync-node-v2\n";
const MAX_TOTAL_WAL_SIZE: u64 = 536_870_912;
pub(crate) const DEFAULT_MAX_LEAF_BYTES: usize = 64 * 1024 * 1024;
const MIN_MAX_LEAF_BYTES: usize = 62;
const PERSISTENT_COLUMN_FAMILIES: [&str; 5] =
    ["roots", "branches", "leaves", "stumps", "references"];

#[cfg(not(feature = "wasm-kernel"))]
static MAX_LEAF_BYTES: Lazy<Result<usize, String>> =
    Lazy::new(|| parse_max_leaf_bytes(std::env::var("SYNC_WEB_MAX_LEAF_BYTES").ok().as_deref()));

#[cfg(not(feature = "wasm-kernel"))]
fn parse_max_leaf_bytes(value: Option<&str>) -> Result<usize, String> {
    let maximum = match value {
        Some(value) => value
            .parse::<usize>()
            .map_err(|_| "SYNC_WEB_MAX_LEAF_BYTES must be a decimal byte count".to_string())?,
        None => DEFAULT_MAX_LEAF_BYTES,
    };
    if !(MIN_MAX_LEAF_BYTES..=DEFAULT_MAX_LEAF_BYTES).contains(&maximum) {
        return Err(format!(
            "SYNC_WEB_MAX_LEAF_BYTES must be between {MIN_MAX_LEAF_BYTES} and {DEFAULT_MAX_LEAF_BYTES}"
        ));
    }
    Ok(maximum)
}

#[cfg(not(feature = "wasm-kernel"))]
pub(crate) fn maximum_leaf_bytes() -> Result<usize, PersistorAccessError> {
    MAX_LEAF_BYTES
        .as_ref()
        .copied()
        .map_err(|error| PersistorAccessError(error.clone()))
}

#[cfg(feature = "wasm-kernel")]
pub(crate) fn maximum_leaf_bytes() -> Result<usize, PersistorAccessError> {
    Ok(DEFAULT_MAX_LEAF_BYTES)
}

fn check_leaf_length(length: usize) -> Result<(), PersistorAccessError> {
    if length > maximum_leaf_bytes()? {
        return Err(PersistorAccessError("Maximum leaf size exceeded".into()));
    }
    Ok(())
}

#[cfg(not(feature = "wasm-kernel"))]
fn has_rocksdb_artifacts<I>(entries: I) -> bool
where
    I: IntoIterator<Item = std::io::Result<OsString>>,
{
    entries.into_iter().any(|entry| {
        let name =
            entry.unwrap_or_else(|error| panic!("Failed to inspect database entry: {error}"));
        let name = name.to_string_lossy();
        name == "CURRENT"
            || name == "IDENTITY"
            || name == "LOG"
            || name.starts_with("LOG.old.")
            || name.starts_with("MANIFEST-")
            || name.starts_with("OPTIONS-")
            || name.ends_with(".log")
            || name.ends_with(".sst")
            || name.ends_with(".blob")
    })
}

#[cfg(not(feature = "wasm-kernel"))]
fn initialize_storage_format(path: &Path) {
    fs::create_dir_all(path).expect("Failed to create database directory");
    let marker = path.join(STORAGE_FORMAT_FILE);
    match fs::read(&marker) {
        Ok(format) if format == STORAGE_FORMAT => return,
        Ok(format) => panic!(
            "Unsupported Journal storage format: {}",
            String::from_utf8_lossy(&format)
        ),
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            panic!("Failed to read Journal storage format: {error}")
        }
        Err(_) => {}
    }

    let has_rocksdb_data = has_rocksdb_artifacts(
        fs::read_dir(path)
            .expect("Failed to inspect database directory")
            .map(|entry| entry.map(|entry| entry.file_name())),
    );
    if has_rocksdb_data {
        panic!("Existing Journal database predates sync-node-v2; start with fresh storage");
    }
    fs::write(marker, STORAGE_FORMAT).expect("Failed to write Journal storage format");
}

fn branch_encoding(digest: Word, left: Word, right: Word) -> [u8; SIZE * 3] {
    let mut encoded = [0; SIZE * 3];
    encoded[..SIZE].copy_from_slice(&digest);
    encoded[SIZE..SIZE * 2].copy_from_slice(&left);
    encoded[SIZE * 2..].copy_from_slice(&right);
    encoded
}

fn branch_word(digest: Word, left: Word, right: Word) -> Word {
    Sha256::digest(branch_encoding(digest, left, right)).into()
}

fn leaf_word(content: &[u8]) -> Word {
    Sha256::digest(Sha256::digest(content)).into()
}

fn stump_word(digest: Word) -> Word {
    let mut encoded = [0; SIZE * 2];
    encoded[..SIZE].copy_from_slice(&digest);
    encoded[SIZE..].copy_from_slice(&digest);
    Sha256::digest(encoded).into()
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct PersistorAccessError(pub String);

#[cfg(feature = "wasm-kernel")]
struct HostPersistor;

#[cfg(feature = "wasm-kernel")]
unsafe extern "C" {
    fn sync_web_host_persist(
        operation: u32,
        request: *const u8,
        request_len: usize,
        response: *mut u8,
        response_cap: usize,
    ) -> isize;
}

#[cfg(feature = "wasm-kernel")]
fn host_persist(
    operation: u32,
    request: &[u8],
    limit: usize,
) -> Result<Vec<u8>, PersistorAccessError> {
    let mut response = vec![0; limit.min(4096)];
    for _ in 0..2 {
        let length = unsafe {
            sync_web_host_persist(
                operation,
                request.as_ptr(),
                request.len(),
                response.as_mut_ptr(),
                response.len(),
            )
        };
        if length < 0 || length as usize > limit {
            return Err(PersistorAccessError(format!(
                "bounded host persistor operation {operation} failed"
            )));
        }
        if length as usize <= response.len() {
            response.truncate(length as usize);
            return Ok(response);
        }
        response.resize(length as usize, 0);
    }
    Err(PersistorAccessError(format!(
        "bounded host persistor operation {operation} changed size"
    )))
}

#[cfg(feature = "wasm-kernel")]
fn exact_word(bytes: &[u8]) -> Result<Word, PersistorAccessError> {
    bytes
        .try_into()
        .map_err(|_| PersistorAccessError("host returned invalid word".to_string()))
}

#[cfg(feature = "wasm-kernel")]
pub(crate) fn host_node_get(word: Word) -> Result<Vec<u8>, PersistorAccessError> {
    host_persist(11, &word, DEFAULT_MAX_LEAF_BYTES + 1)
}

pub trait Persistor {
    fn root_list(&self) -> Vec<Word>;
    fn root_new(&self, handle: Word, root: Word) -> Result<Word, PersistorAccessError>;
    fn root_temp(&self, root: Word) -> Result<Word, PersistorAccessError>;
    fn root_get(&self, handle: Word) -> Result<Word, PersistorAccessError>;
    fn root_set(
        &self,
        handle: Word,
        old: Word,
        new: Word,
        source: &dyn Persistor,
    ) -> Result<Word, PersistorAccessError>;
    fn root_delete(&self, handle: Word) -> Result<(), PersistorAccessError>;
    fn branch_set(
        &self,
        left: Word,
        right: Word,
        digest: Word,
    ) -> Result<Word, PersistorAccessError>;
    fn branch_get(&self, branch: Word) -> Result<(Word, Word, Word), PersistorAccessError>;
    fn leaf_set(&self, content: Vec<u8>) -> Result<Word, PersistorAccessError>;
    fn leaf_get(&self, leaf: Word) -> Result<Vec<u8>, PersistorAccessError>;
    fn stump_set(&self, digest: Word) -> Result<Word, PersistorAccessError>;
    fn stump_get(&self, stump: Word) -> Result<Word, PersistorAccessError>;
}

#[cfg(not(feature = "wasm-kernel"))]
#[allow(dead_code)]
pub(crate) trait BoundedPersistor: Persistor {
    fn root_list_bounded(&self, limit: usize) -> Result<Vec<Word>, PersistorAccessError>;
    fn leaf_get_bounded(&self, leaf: Word, limit: usize) -> Result<Vec<u8>, PersistorAccessError>;
}

#[cfg(not(feature = "wasm-kernel"))]
#[allow(dead_code)]
pub(crate) fn root_list_bounded(limit: usize) -> Result<Vec<Word>, PersistorAccessError> {
    PERSISTOR.root_list_bounded(limit)
}

#[cfg(not(feature = "wasm-kernel"))]
#[allow(dead_code)]
pub(crate) fn leaf_get_bounded(leaf: Word, limit: usize) -> Result<Vec<u8>, PersistorAccessError> {
    PERSISTOR.leaf_get_bounded(leaf, limit)
}

#[cfg(feature = "wasm-kernel")]
impl Persistor for HostPersistor {
    fn root_list(&self) -> Vec<Word> {
        host_persist(1, &[], 32 * 65_536)
            .expect("host root-list failed")
            .chunks_exact(SIZE)
            .map(|word| word.try_into().expect("host root-list word"))
            .collect()
    }

    fn root_new(&self, handle: Word, root: Word) -> Result<Word, PersistorAccessError> {
        let mut request = Vec::from(handle);
        request.extend(root);
        exact_word(&host_persist(2, &request, SIZE)?)
    }

    fn root_temp(&self, _root: Word) -> Result<Word, PersistorAccessError> {
        Err(PersistorAccessError(
            "temporary roots stay native".to_string(),
        ))
    }

    fn root_get(&self, handle: Word) -> Result<Word, PersistorAccessError> {
        exact_word(&host_persist(3, &handle, SIZE)?)
    }

    fn root_set(
        &self,
        _handle: Word,
        _old: Word,
        _new: Word,
        _source: &dyn Persistor,
    ) -> Result<Word, PersistorAccessError> {
        Err(PersistorAccessError(
            "durable commit stays native".to_string(),
        ))
    }

    fn root_delete(&self, handle: Word) -> Result<(), PersistorAccessError> {
        host_persist(4, &handle, 0).map(|_| ())
    }

    fn branch_set(
        &self,
        left: Word,
        right: Word,
        digest: Word,
    ) -> Result<Word, PersistorAccessError> {
        let mut request = Vec::from(left);
        request.extend(right);
        request.extend(digest);
        exact_word(&host_persist(5, &request, SIZE)?)
    }

    fn branch_get(&self, branch: Word) -> Result<(Word, Word, Word), PersistorAccessError> {
        let response = host_persist(6, &branch, SIZE * 3)?;
        if response.len() != SIZE * 3 {
            return Err(PersistorAccessError(
                "host returned invalid branch".to_string(),
            ));
        }
        Ok((
            exact_word(&response[..SIZE])?,
            exact_word(&response[SIZE..SIZE * 2])?,
            exact_word(&response[SIZE * 2..])?,
        ))
    }

    fn leaf_set(&self, content: Vec<u8>) -> Result<Word, PersistorAccessError> {
        exact_word(&host_persist(7, &content, SIZE)?)
    }

    fn leaf_get(&self, leaf: Word) -> Result<Vec<u8>, PersistorAccessError> {
        host_persist(8, &leaf, DEFAULT_MAX_LEAF_BYTES)
    }

    fn stump_set(&self, digest: Word) -> Result<Word, PersistorAccessError> {
        exact_word(&host_persist(9, &digest, SIZE)?)
    }

    fn stump_get(&self, stump: Word) -> Result<Word, PersistorAccessError> {
        exact_word(&host_persist(10, &stump, SIZE)?)
    }
}
