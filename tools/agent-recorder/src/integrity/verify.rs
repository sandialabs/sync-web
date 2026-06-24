use std::collections::HashMap;

use anyhow::{bail, Result};
use sha2::{Digest, Sha256};

use crate::{
    records::{IndexedGraphRecord, RecordReader},
    GraphRecord,
};

use super::{
    crypto::{b64, hmac_bytes},
    signing::canonical_payload_bytes,
    state::read_one,
    IntegrityKey, VerificationResult, VerificationStatus, ALGORITHM,
};

/// Read and verify one record by absolute backend index.
///
/// Returns the indexed record on success and an error for missing or mismatched
/// integrity data.
pub fn verify_indexed_record(
    reader: &dyn RecordReader,
    index: u64,
    key: &IntegrityKey,
) -> Result<IndexedGraphRecord> {
    let mut verifier = IndexedVerifier::new(reader, key);
    verifier.verify_index(index)
}

/// Verify multiple indexed records while reusing fetched records.
pub fn verify_indexed_records(
    reader: &dyn RecordReader,
    indices: impl IntoIterator<Item = u64>,
    key: &IntegrityKey,
) -> Result<Vec<IndexedGraphRecord>> {
    let mut verifier = IndexedVerifier::new(reader, key);
    indices
        .into_iter()
        .map(|index| verifier.verify_index(index))
        .collect()
}

struct IndexedVerifier<'a> {
    reader: &'a dyn RecordReader,
    key: &'a IntegrityKey,
    records: HashMap<u64, IndexedGraphRecord>,
}

impl<'a> IndexedVerifier<'a> {
    fn new(reader: &'a dyn RecordReader, key: &'a IntegrityKey) -> Self {
        Self {
            reader,
            key,
            records: HashMap::new(),
        }
    }

    fn verify_index(&mut self, index: u64) -> Result<IndexedGraphRecord> {
        let record = self.record(index)?.clone();
        let verification = verify_record(record.index, &record.record, self.key)?;
        if verification.status != VerificationStatus::Verified {
            bail!(
                "integrity verification failed at index {}: {:?}",
                record.index,
                verification.status
            );
        }
        Ok(record)
    }

    fn record(&mut self, index: u64) -> Result<&IndexedGraphRecord> {
        if !self.records.contains_key(&index) {
            let record = read_one(self.reader, index)?;
            self.records.insert(index, record);
        }
        self.records
            .get(&index)
            .ok_or_else(|| anyhow::anyhow!("missing record at index {index}"))
    }
}

/// Verify an already-loaded record against its expected backend index.
///
/// Unlike [`verify_indexed_record`], this returns a structured status instead
/// of failing for ordinary verification mismatches.
pub fn verify_record(
    indexed: u64,
    record: &GraphRecord,
    key: &IntegrityKey,
) -> Result<VerificationResult> {
    let Some(integrity) = &record.integrity else {
        return Ok(result(indexed, VerificationStatus::MissingIntegrity, None));
    };
    if integrity.algorithm != ALGORITHM {
        return Ok(result(
            indexed,
            VerificationStatus::UnsupportedAlgorithm,
            None,
        ));
    }
    if integrity.index != indexed {
        return Ok(result(
            indexed,
            VerificationStatus::IndexMismatch,
            Some(format!(
                "record integrity index {} != log index {indexed}",
                integrity.index
            )),
        ));
    }
    if integrity.key_id != key.key_id() {
        return Ok(result(
            indexed,
            VerificationStatus::UnsupportedAlgorithm,
            Some("key-id does not match provided key".to_string()),
        ));
    }

    let payload_hash = Sha256::digest(canonical_payload_bytes(record)?);
    if b64(&payload_hash) != integrity.payload_hash {
        return Ok(result(
            indexed,
            VerificationStatus::PayloadHashMismatch,
            None,
        ));
    }
    let event_key = key.event_key(integrity.index);
    let authenticator = hmac_bytes(
        &event_key,
        &[
            b"agent-recorder/integrity/authenticator/v1",
            &integrity.index.to_be_bytes(),
            &payload_hash,
        ],
    );
    if b64(&authenticator) != integrity.authenticator {
        return Ok(result(
            indexed,
            VerificationStatus::AuthenticatorMismatch,
            None,
        ));
    }
    Ok(result(indexed, VerificationStatus::Verified, None))
}

fn result(index: u64, status: VerificationStatus, message: Option<String>) -> VerificationResult {
    VerificationResult {
        index,
        status,
        message,
    }
}
