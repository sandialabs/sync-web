use anyhow::Result;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{canonical_json_bytes, GraphRecord};

use super::{
    crypto::{b64, hmac_bytes},
    IntegrityKey, IntegrityMetadata, ALGORITHM,
};

/// Return a copy of `record` with integrity metadata for `index`.
///
/// The payload hash covers canonical JSON for the whole record with the
/// `integrity` block removed. The authenticator is an HMAC over the index and
/// payload hash using the derived per-index event key.
pub fn sign_record(record: &GraphRecord, index: u64, key: &IntegrityKey) -> Result<GraphRecord> {
    let event_key = key.event_key(index);
    sign_record_with_event_key(record, index, key.key_id(), &event_key)
}

pub(crate) fn sign_record_with_event_key(
    record: &GraphRecord,
    index: u64,
    key_id: &str,
    event_key: &[u8],
) -> Result<GraphRecord> {
    let payload_bytes = canonical_payload_bytes(record)?;
    let payload_hash = Sha256::digest(&payload_bytes);
    let payload_hash_b64 = b64(&payload_hash);
    let authenticator = hmac_bytes(
        event_key,
        &[
            b"agent-recorder/integrity/authenticator/v1",
            &index.to_be_bytes(),
            &payload_hash,
        ],
    );

    let mut signed = record.clone();
    signed.integrity = None;
    signed.integrity = Some(IntegrityMetadata {
        algorithm: ALGORITHM.to_string(),
        key_id: key_id.to_string(),
        index,
        payload_hash: payload_hash_b64,
        authenticator: b64(&authenticator),
    });
    Ok(signed)
}

pub(crate) fn canonical_payload_bytes(record: &GraphRecord) -> Result<Vec<u8>> {
    let mut value = serde_json::to_value(record)?;
    if let Value::Object(object) = &mut value {
        object.remove("integrity");
    }
    normalize_payload_value(&mut value);
    canonical_json_bytes(&value)
}

fn normalize_payload_value(value: &mut Value) {
    match value {
        Value::Object(object) => {
            let keys = object.keys().cloned().collect::<Vec<_>>();
            for key in keys {
                if let Some(child) = object.get_mut(&key) {
                    normalize_payload_value(child);
                }
                let remove_default_metadata = key == "metadata"
                    && object.get(&key).is_some_and(|value| {
                        matches!(value, Value::Null) || crate::is_empty_object(value)
                    });
                if remove_default_metadata {
                    object.remove(&key);
                }
            }
        }
        Value::Array(values) => {
            for value in values {
                normalize_payload_value(value);
            }
        }
        _ => {}
    }
}
