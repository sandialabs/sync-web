use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

use anyhow::{bail, Context, Result};

use crate::records::{IndexedGraphRecord, RecordReader, RecordSelector};

use super::{
    crypto::b64,
    schedule::{canonical_event_key, generated_future_keys},
    signing::sign_record_with_event_key,
    IntegrityAlignment, IntegrityFutureKey, IntegrityKey, IntegrityState, IntegrityStatus,
    ALGORITHM,
};

/// Replace local future-key state with a new root key at the current backend end.
///
/// Existing records remain verifiable with their previous key. No special record
/// is written; subsequent appends carry the new key id.
pub fn rekey_state(
    reader: &dyn RecordReader,
    state_path: impl AsRef<Path>,
    key: &IntegrityKey,
) -> Result<IntegrityStatus> {
    let state_path = state_path.as_ref();
    let mut state = load_state(state_path)?;
    if reconcile_state(reader, &mut state)? {
        store_state(state_path, &state)?;
    }
    let seed_index = state.next_index;
    state.key_id = key.key_id().to_string();
    state.future_keys = vec![IntegrityFutureKey {
        source: seed_index,
        target: seed_index,
        level: 0,
        key: b64(&key.event_key(seed_index)),
    }];
    store_state(state_path, &state)?;
    integrity_status(reader, &state)
}

/// Compare local integrity state with backend record alignment without mutation.
pub fn integrity_status(
    reader: &dyn RecordReader,
    state: &IntegrityState,
) -> Result<IntegrityStatus> {
    let backend_next_index = contiguous_backend_next_index(reader)?;
    let alignment = if backend_next_index == state.next_index {
        IntegrityAlignment::Aligned
    } else if backend_next_index == state.next_index.saturating_add(1) {
        IntegrityAlignment::OneStepRepairable
    } else if backend_next_index < state.next_index {
        IntegrityAlignment::StateAheadOfBackend
    } else {
        IntegrityAlignment::BackendTooFarAhead
    };

    Ok(IntegrityStatus {
        algorithm: state.algorithm.clone(),
        key_id: state.key_id.clone(),
        state_next_index: state.next_index,
        backend_next_index,
        backend_latest_index: backend_next_index.checked_sub(1),
        pending_key_count: state.future_keys.len(),
        alignment,
    })
}

/// Load private integrity state from disk.
pub fn load_state(path: impl AsRef<Path>) -> Result<IntegrityState> {
    let text = fs::read_to_string(path.as_ref())
        .with_context(|| format!("reading integrity state {}", path.as_ref().display()))?;
    serde_json::from_str(&text)
        .with_context(|| format!("parsing integrity state {}", path.as_ref().display()))
}

pub(crate) fn load_or_create_state(
    path: &Path,
    init_key: Option<&IntegrityKey>,
) -> Result<IntegrityState> {
    if path.exists() {
        let state = load_state(path)?;
        if state.algorithm != ALGORITHM {
            bail!("unsupported integrity state algorithm");
        }
        if let Some(key) = init_key {
            if state.key_id != key.key_id() {
                bail!("integrity state key-id does not match provided key");
            }
        }
        if state
            .future_keys
            .iter()
            .any(|key| key.target < state.next_index)
        {
            bail!("integrity state contains expired future keys");
        }
        return Ok(state);
    }

    let Some(key) = init_key else {
        bail!("integrity state does not exist; provide --integrity-key-env or --integrity-key to initialize it");
    };
    let state = IntegrityState {
        algorithm: ALGORITHM.to_string(),
        key_id: key.key_id().to_string(),
        next_index: 0,
        future_keys: vec![IntegrityFutureKey {
            source: 0,
            target: 0,
            level: 0,
            key: b64(&key.initial_event_key()),
        }],
    };
    store_state(path, &state)?;
    Ok(state)
}

pub(crate) fn reconcile_state(
    reader: &dyn RecordReader,
    state: &mut IntegrityState,
) -> Result<bool> {
    let next = state.next_index;
    if let Some(record) = try_read_one(reader, next)? {
        if try_read_one(reader, next + 1)?.is_some() {
            bail!(
                "integrity state is more than one record behind backend at index {next}; refusing automatic recovery"
            );
        }
        repair_state_for_written_record(state, record)?;
        return Ok(true);
    }

    if next > 0 && try_read_one(reader, next - 1)?.is_none() {
        bail!("integrity state next-index {next} is ahead of backend; refusing to append");
    }
    Ok(false)
}

fn repair_state_for_written_record(
    state: &mut IntegrityState,
    indexed: IndexedGraphRecord,
) -> Result<()> {
    if indexed.index != state.next_index {
        bail!(
            "cannot repair integrity state index {} from backend index {}",
            state.next_index,
            indexed.index
        );
    }
    let mut repaired = state.clone();
    let consumed = consume_event_keys(&mut repaired, indexed.index)?;
    let event_key = canonical_event_key(indexed.index, &consumed)?;
    let generated = generated_future_keys(indexed.index, &event_key);
    let expected =
        sign_record_with_event_key(&indexed.record, indexed.index, &repaired.key_id, &event_key)?;
    if indexed.record.integrity != expected.integrity {
        bail!(
            "backend record {} does not match recoverable integrity state",
            indexed.index
        );
    }
    advance_state(&mut repaired, indexed.index, generated)?;
    *state = repaired;
    Ok(())
}

pub(crate) fn consume_event_keys(
    state: &mut IntegrityState,
    index: u64,
) -> Result<Vec<IntegrityFutureKey>> {
    if state.next_index != index {
        bail!("integrity state next-index mismatch");
    }
    let mut consumed = Vec::new();
    let mut retained = Vec::new();
    for key in state.future_keys.drain(..) {
        if key.target == index {
            consumed.push(key);
        } else {
            retained.push(key);
        }
    }
    if consumed.is_empty() {
        bail!("integrity state is missing event key for index {index}");
    }
    state.future_keys = retained;
    Ok(consumed)
}

pub(crate) fn advance_state(
    state: &mut IntegrityState,
    index: u64,
    generated: Vec<IntegrityFutureKey>,
) -> Result<()> {
    state.next_index = index
        .checked_add(1)
        .ok_or_else(|| anyhow::anyhow!("integrity index exhausted u64 range"))?;
    state.future_keys.retain(|key| key.target > index);
    state.future_keys.extend(generated);
    state
        .future_keys
        .sort_by_key(|key| (key.target, key.source, key.level));
    Ok(())
}

pub(crate) fn contiguous_backend_next_index(reader: &dyn RecordReader) -> Result<u64> {
    let mut index = 0u64;
    loop {
        if try_read_one(reader, index)?.is_none() {
            return Ok(index);
        }
        index = index
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("backend index exhausted u64 range"))?;
    }
}

pub(crate) fn read_one(reader: &dyn RecordReader, index: u64) -> Result<IndexedGraphRecord> {
    try_read_one(reader, index)?.ok_or_else(|| anyhow::anyhow!("missing record at index {index}"))
}

fn try_read_one(reader: &dyn RecordReader, index: u64) -> Result<Option<IndexedGraphRecord>> {
    let mut found = None;
    reader.read(RecordSelector::Index(index), &mut |record| {
        found = Some(record);
        Ok(())
    })?;
    Ok(found)
}

pub(crate) fn store_state(path: &Path, state: &IntegrityState) -> Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(state)?;
    let tmp_path = path.with_extension(format!(
        "{}.tmp",
        path.extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("integrity")
    ));
    {
        let mut file = File::create(&tmp_path).with_context(|| {
            format!("creating integrity state temp file {}", tmp_path.display())
        })?;
        file.write_all(text.as_bytes())
            .with_context(|| format!("writing integrity state temp file {}", tmp_path.display()))?;
        file.write_all(b"\n")?;
        file.sync_all()
            .with_context(|| format!("syncing integrity state temp file {}", tmp_path.display()))?;
    }
    fs::rename(&tmp_path, path).with_context(|| {
        format!(
            "renaming integrity state temp file {} to {}",
            tmp_path.display(),
            path.display()
        )
    })?;
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        if let Ok(directory) = File::open(parent) {
            let _ = directory.sync_all();
        }
    }
    Ok(())
}
